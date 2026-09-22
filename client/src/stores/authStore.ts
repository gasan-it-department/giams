import { create } from 'zustand'
import { api, ApiError, setAuthHandlers } from '@/lib/api'
import { secureStore } from '@/lib/secureStore'
import { sessionSchema, type UserSummary } from '@/schemas/auth'

/**
 * Who is signed in.
 *
 * Deliberately **not** persisted. The access token lives here in memory and
 * nowhere else — no `localStorage`, no cookie, no Zustand `persist`. Anything
 * written to disk can be read off a shared workstation later; the refresh
 * token is the one thing that has to survive a reload, and it goes to the
 * Windows Credential Manager instead (see `lib/secureStore.ts`).
 */

interface AuthState {
  user: UserSummary | null
  accessToken: string | null
  /** True until the stored refresh token has been checked on first load. */
  initialising: boolean

  signIn: (username: string, password: string) => Promise<void>
  signOut: () => Promise<void>
  restore: () => Promise<void>
  can: (permission: string) => boolean
}

/** Identifies this workstation on the session record. */
function workstationName(): string | undefined {
  if (typeof navigator === 'undefined') return undefined
  return navigator.userAgent.slice(0, 120)
}

export const useAuthStore = create<AuthState>((set, get) => ({
  user: null,
  accessToken: null,
  initialising: true,

  async signIn(username, password) {
    const raw = await api.post<unknown>('/api/auth/login', {
      username,
      password,
      workstation: workstationName(),
    })

    const session = sessionSchema.parse(raw)
    await secureStore.set(session.refresh_token)
    set({ user: session.user, accessToken: session.access_token })
  },

  async signOut() {
    const refresh = await secureStore.get()

    // Clear locally first. If the server call fails — the node is unreachable,
    // say — the person must still end up signed out on this machine; the
    // session expires server-side on its own.
    set({ user: null, accessToken: null })
    await secureStore.clear()

    if (refresh) {
      try {
        await api.post('/api/auth/logout', { refresh_token: refresh })
      } catch {
        /* already signed out locally */
      }
    }
  },

  /**
   * Restores a session on start-up by spending the stored refresh token.
   *
   * A failure here is normal, not exceptional: the token may have expired, or
   * been revoked, or this may simply be a first run. Any failure means "show
   * the sign-in screen".
   */
  async restore() {
    const refresh = await secureStore.get()
    if (!refresh) {
      set({ initialising: false })
      return
    }

    try {
      const raw = await api.post<unknown>('/api/auth/refresh', { refresh_token: refresh })
      const session = sessionSchema.parse(raw)
      await secureStore.set(session.refresh_token)
      set({ user: session.user, accessToken: session.access_token, initialising: false })
    } catch (e) {
      // Keep the token only when the node could not be reached — the session
      // may still be perfectly good once the office server is back.
      if (!(e instanceof ApiError && e.code === 'NODE_UNREACHABLE')) {
        await secureStore.clear()
      }
      set({ user: null, accessToken: null, initialising: false })
    }
  },

  can(permission) {
    return get().user?.permissions.includes(permission) ?? false
  },
}))

/**
 * Wires the store into the fetch layer.
 *
 * Done through registered callbacks rather than importing the store from
 * `api.ts`, which would be a cycle. `onRefresh` is what lets a request that
 * met an expired access token retry transparently, so a fifteen-minute token
 * never interrupts someone mid-voucher.
 */
setAuthHandlers({
  getAccessToken: () => useAuthStore.getState().accessToken,

  async onRefresh() {
    const refresh = await secureStore.get()
    if (!refresh) return null

    const raw = await api.post<unknown>('/api/auth/refresh', { refresh_token: refresh })
    const session = sessionSchema.parse(raw)

    await secureStore.set(session.refresh_token)
    useAuthStore.setState({ user: session.user, accessToken: session.access_token })

    return session.access_token
  },

  onSignedOut() {
    void secureStore.clear()
    useAuthStore.setState({ user: null, accessToken: null })
  },
})
