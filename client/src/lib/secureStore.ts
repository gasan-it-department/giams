/**
 * Where the refresh token lives.
 *
 * The access token is never stored anywhere — it stays in memory in the auth
 * store and dies with the tab. The refresh token has to outlive a reload, so
 * it needs somewhere, and the choice differs by shell:
 *
 *   * **Desktop (Tauri)** — the Windows Credential Manager, via DPAPI. Encrypted
 *     at rest against the user account, and unreadable by another process
 *     running as another user.
 *   * **Browser (LAN dashboards)** — `sessionStorage`. Not as good, and
 *     deliberately not `localStorage`: session storage is scoped to the tab
 *     and cleared when it closes, so a shared workstation does not leave a
 *     usable token behind for whoever sits down next.
 *
 * The browser path is only used for the read-only dashboards the node serves.
 * Anyone recording entries is on the desktop client.
 */

const KEY = 'giams.refresh'

interface SecureStore {
  get(): Promise<string | null>
  set(value: string): Promise<void>
  clear(): Promise<void>
}

/** True when running inside the Tauri shell rather than a plain browser. */
function inDesktopShell(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window
}

const browserStore: SecureStore = {
  async get() {
    try {
      return sessionStorage.getItem(KEY)
    } catch {
      // Storage can be unavailable under strict privacy settings. Treat it as
      // "not signed in" rather than crashing the application.
      return null
    }
  },
  async set(value) {
    try {
      sessionStorage.setItem(KEY, value)
    } catch {
      // Nothing to do. The session simply will not survive a reload.
    }
  },
  async clear() {
    try {
      sessionStorage.removeItem(KEY)
    } catch {
      /* already gone */
    }
  },
}

/**
 * Credential Manager backing, used once the Tauri shell is in place.
 *
 * Falls back to the browser store if the plugin is unavailable, so a
 * misconfigured build degrades to a working-but-weaker state rather than
 * locking everyone out of the system.
 */
const desktopStore: SecureStore = {
  async get() {
    try {
      const { invoke } = await import('@tauri-apps/api/core')
      return await invoke<string | null>('read_refresh_token')
    } catch {
      return browserStore.get()
    }
  },
  async set(value) {
    try {
      const { invoke } = await import('@tauri-apps/api/core')
      await invoke('store_refresh_token', { token: value })
    } catch {
      await browserStore.set(value)
    }
  },
  async clear() {
    try {
      const { invoke } = await import('@tauri-apps/api/core')
      await invoke('clear_refresh_token')
    } catch {
      await browserStore.clear()
    }
  },
}

export const secureStore: SecureStore = inDesktopShell() ? desktopStore : browserStore
export { inDesktopShell }
