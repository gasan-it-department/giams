/**
 * The single door to the accounting server.
 *
 * No component calls `fetch` directly. Everything goes through here so that
 * error handling, the offline case, and the plain-language message contract
 * exist in exactly one place.
 */

/** Mirrors `ErrorBody` in `server/crates/api/src/error.rs`. */
export interface ApiErrorBody {
  /** Stable machine identifier. Branch on it; never show it to a user. */
  code:      string
  /** Plain language, already written for a clerk. Show this. */
  message:   string
  /** What to do about it. */
  hint?:     string
  /** Per-field messages, keyed by form field name. Feed to react-hook-form. */
  fields?:   Record<string, string>
  /** Short correlation code, also in the server log. */
  reference: string
}

/**
 * Every failure the UI can encounter, already carrying a message fit to show.
 *
 * The constructor is the only place that decides what a user reads, which is
 * what stops a raw `TypeError: Failed to fetch` reaching the screen.
 */
export class ApiError extends Error {
  readonly code:      string
  readonly hint?:     string
  readonly fields:    Record<string, string>
  readonly reference: string
  readonly status:    number

  constructor(body: ApiErrorBody, status: number) {
    super(body.message)
    this.name      = 'ApiError'
    this.code      = body.code
    this.hint      = body.hint
    this.fields    = body.fields ?? {}
    this.reference = body.reference
    this.status    = status
  }

  /** True when retrying might genuinely help. Never true for a rejected rule. */
  get isTransient(): boolean {
    return this.code === 'NODE_UNREACHABLE' || this.status >= 500
  }

  /** True when the user has been signed out and needs to sign in again. */
  get needsSignIn(): boolean {
    return this.code === 'UNAUTHENTICATED'
  }
}

/**
 * Builds an ApiError for a failure that never reached the server.
 *
 * The browser's own message here is `Failed to fetch`, which tells a clerk
 * nothing. This is the offline case, and it is phrased to reassure rather
 * than alarm — under the architecture their work is kept locally as a draft.
 */
function offlineError(): ApiError {
  return new ApiError(
    {
      code:      'NODE_UNREACHABLE',
      message:   'The accounting office server cannot be reached.',
      hint:      'Your work is saved on this computer and will be sent automatically once the connection returns.',
      reference: 'LOCAL',
    },
    0,
  )
}

function unreadableError(reference: string): ApiError {
  return new ApiError(
    {
      code:      'UNEXPECTED',
      message:   'Something went wrong and this could not be completed.',
      hint:      `Nothing was saved. Try again, and if it keeps happening give your system administrator this reference: ${reference}`,
      reference,
    },
    0,
  )
}

/**
 * Statuses that mean "the node did not answer", as opposed to "the node
 * refused". 502/503/504 come from whatever sits between the client and the
 * node; 522/523/524 are the equivalents some proxies use.
 */
const GATEWAY_STATUSES = new Set([502, 503, 504, 522, 523, 524])

interface RequestOptions extends Omit<RequestInit, 'body'> {
  body?: unknown
  /** Internal: set when retrying after a token refresh, to stop a loop. */
  _retried?: boolean
}

// ── Auth wiring ────────────────────────────────────────────────────────────
// Registered by the auth store. Callbacks rather than an import, because
// importing the store here would make a cycle.

interface AuthHandlers {
  getAccessToken: () => string | null
  onRefresh: () => Promise<string | null>
  onSignedOut: () => void
}

let authHandlers: AuthHandlers | null = null

export function setAuthHandlers(handlers: AuthHandlers): void {
  authHandlers = handlers
}

/**
 * In-flight refresh, shared by every request that hits an expired token.
 *
 * Without this, a screen firing five queries at once when the access token
 * has just expired would send five refreshes. The first rotates the token and
 * the other four present one that is now retired — which the server correctly
 * reads as token theft and revokes every session. Single-flighting is not an
 * optimisation here; it is what stops the client logging itself out.
 */
let refreshInFlight: Promise<string | null> | null = null

function refreshOnce(): Promise<string | null> {
  if (!authHandlers) return Promise.resolve(null)

  refreshInFlight ??= authHandlers
    .onRefresh()
    .catch(() => null)
    .finally(() => {
      refreshInFlight = null
    })

  return refreshInFlight
}

/** Endpoints that must never trigger a refresh attempt of their own. */
function isAuthEndpoint(path: string): boolean {
  return path.startsWith('/api/auth/')
}

async function request<T>(path: string, options: RequestOptions = {}): Promise<T> {
  const { body, headers, _retried, ...rest } = options
  const token = authHandlers?.getAccessToken() ?? null

  let response: Response
  try {
    response = await fetch(path, {
      ...rest,
      headers: {
        'Content-Type': 'application/json',
        ...(token ? { Authorization: `Bearer ${token}` } : {}),
        ...headers,
      },
      body: body === undefined ? undefined : JSON.stringify(body),
      credentials: 'same-origin',
    })
  } catch {
    // Network-level failure: no server, no DNS, no route. Never surfaced raw.
    throw offlineError()
  }

  // An expired access token is the ordinary case, not an error: they last
  // fifteen minutes. Refresh once and replay the request so the person
  // filling in a voucher never sees it happen.
  if (response.status === 401 && !_retried && !isAuthEndpoint(path)) {
    const renewed = await refreshOnce()
    if (renewed) {
      return request<T>(path, { ...options, _retried: true })
    }
    authHandlers?.onSignedOut()
  }

  // A gateway status means something in front of the node answered but the
  // node itself did not. From the clerk's point of view that is identical to
  // having no connection at all, so it gets the reassuring offline message
  // rather than an alarming generic failure. Reverse proxies, load balancers,
  // and captive portals all produce these.
  if (GATEWAY_STATUSES.has(response.status)) {
    throw offlineError()
  }

  if (response.status === 204) {
    return undefined as T
  }

  const raw = await response.text()

  if (!response.ok) {
    try {
      throw new ApiError(JSON.parse(raw) as ApiErrorBody, response.status)
    } catch (e) {
      if (e instanceof ApiError) throw e
      // The server failed in a way that did not produce our error shape --
      // an HTML error page, or a crash before the handler ran. Never show the
      // body: it could be a stack trace or a SQL fragment.
      throw unreadableError(`HTTP-${response.status}`)
    }
  }

  try {
    return JSON.parse(raw) as T
  } catch {
    throw unreadableError('BAD-JSON')
  }
}

/**
 * Fetches a file and saves it, carrying the same session as every other call.
 *
 * A plain link cannot be used: the access token lives in memory rather than in
 * a cookie, precisely so that a stolen cookie cannot be replayed. So the file
 * is fetched like any other request — refresh-on-401 and all — and handed to
 * the browser as a blob.
 *
 * The filename comes from the server's `Content-Disposition`, so a saved
 * export says what it is and what period it covers without being renamed.
 */
async function download(path: string, fallbackName: string, retried = false): Promise<void> {
  const token = authHandlers?.getAccessToken() ?? null

  let response: Response
  try {
    response = await fetch(path, {
      headers: token ? { Authorization: `Bearer ${token}` } : {},
      credentials: 'same-origin',
    })
  } catch {
    throw offlineError()
  }

  if (response.status === 401 && !retried) {
    const renewed = await refreshOnce()
    if (renewed) return download(path, fallbackName, true)
    authHandlers?.onSignedOut()
  }

  if (GATEWAY_STATUSES.has(response.status)) throw offlineError()

  if (!response.ok) {
    // The failure body is still JSON in our error shape, so the person gets
    // the same plain-language message they would from any other screen.
    const raw = await response.text()
    try {
      throw new ApiError(JSON.parse(raw) as ApiErrorBody, response.status)
    } catch (e) {
      if (e instanceof ApiError) throw e
      throw unreadableError(`HTTP-${response.status}`)
    }
  }

  const disposition = response.headers.get('Content-Disposition') ?? ''
  const named = /filename="?([^"]+)"?/.exec(disposition)
  const filename = named?.[1] ?? fallbackName

  const blob = await response.blob()
  const url = URL.createObjectURL(blob)

  try {
    const anchor = document.createElement('a')
    anchor.href = url
    anchor.download = filename
    document.body.append(anchor)
    anchor.click()
    anchor.remove()
  } finally {
    // Revoked on the next tick: revoking synchronously can cancel the save in
    // some browsers before it has read the blob.
    setTimeout(() => URL.revokeObjectURL(url), 0)
  }
}

export const api = {
  get:  <T>(path: string)                => request<T>(path),
  download,
  post: <T>(path: string, body?: unknown) => request<T>(path, { method: 'POST',   body }),
  put:  <T>(path: string, body?: unknown) => request<T>(path, { method: 'PUT',    body }),
  del:  <T>(path: string)                => request<T>(path, { method: 'DELETE' }),
}
