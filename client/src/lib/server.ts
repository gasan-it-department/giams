/**
 * Where the accounting server is.
 *
 * In a browser the pages come from the server (or the dev proxy in front of
 * it), so every call is same-origin and the address is empty. The desktop app
 * serves its pages itself and has to be told where the office's server is:
 * this computer by default, or another one on the office network, chosen on
 * the sign-in screen.
 *
 * Kept in localStorage — it is an address, not a secret, and it should
 * survive restarts.
 */

const KEY = 'giams.server'

/** The office server running on this same computer. */
export const DEFAULT_SERVER = 'http://127.0.0.1:8787'

/** True inside the installed desktop app rather than a browser. */
export function inDesktopApp(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window
}

/** The server's address, without a trailing slash. Empty in a browser. */
export function serverUrl(): string {
  if (!inDesktopApp()) return ''
  try {
    return localStorage.getItem(KEY) ?? DEFAULT_SERVER
  } catch {
    return DEFAULT_SERVER
  }
}

/**
 * Turns what someone typed into an address: "192.168.1.25" becomes
 * "http://192.168.1.25:8787". Returns null for something that is not one.
 */
export function normaliseServer(input: string): string | null {
  let text = input.trim()
  if (!text) return null
  if (!/^https?:\/\//i.test(text)) text = `http://${text}`
  try {
    const url = new URL(text)
    if (!url.port && url.protocol === 'http:') url.port = '8787'
    return url.origin
  } catch {
    return null
  }
}

export function setServerUrl(url: string): void {
  try {
    localStorage.setItem(KEY, url)
  } catch {
    /* the default stays in use */
  }
}
