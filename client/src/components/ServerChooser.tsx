import { useState } from 'react'
import { Loader2, Radar, Server } from 'lucide-react'
import { discoverNodes, type DiscoveredNode } from '@/lib/drafts'
import { inDesktopApp, normaliseServer, serverUrl, setServerUrl } from '@/lib/server'

/**
 * Which accounting server the desktop app talks to.
 *
 * Only in the desktop app — in a browser the page already came from the
 * server. Usually left alone: the default is this same computer. A
 * workstation elsewhere in the office picks the server here once, either from
 * the ones found on the network or by typing its address.
 */
export function ServerChooser() {
  const [open, setOpen] = useState(false)
  const [address, setAddress] = useState(() => serverUrl().replace(/^https?:\/\//, ''))
  const [invalid, setInvalid] = useState(false)
  const [searching, setSearching] = useState(false)
  const [found, setFound] = useState<DiscoveredNode[] | null>(null)

  if (!inDesktopApp()) return null

  // a new server means new sessions and data, so start clean
  function use(url: string) {
    setServerUrl(url)
    window.location.reload()
  }

  function save() {
    const url = normaliseServer(address)
    if (!url) {
      setInvalid(true)
      return
    }
    use(url)
  }

  async function search() {
    setSearching(true)
    try {
      setFound(await discoverNodes())
    } catch {
      setFound([])
    } finally {
      setSearching(false)
    }
  }

  if (!open) {
    return (
      <p className="mt-3 flex items-center justify-center gap-1.5 text-sm text-ink-muted">
        <Server className="size-3.5" aria-hidden />
        Server: {serverUrl().replace(/^https?:\/\//, '')}
        <button type="button" onClick={() => setOpen(true)} className="ml-1 text-accent-solid hover:underline">
          Change
        </button>
      </p>
    )
  }

  return (
    <div className="mt-4 rounded-lg bg-surface p-4 text-sm">
      <p className="mb-2 font-medium">Accounting office server</p>

      <div className="flex gap-2">
        <input
          value={address}
          onChange={(e) => { setAddress(e.target.value); setInvalid(false) }}
          onKeyDown={(e) => { if (e.key === 'Enter') save() }}
          placeholder="192.168.1.25:8787"
          aria-label="Server address"
          className={`min-w-0 flex-1 rounded-md bg-sunken px-3 py-2 tabular ${invalid ? 'ring-1 ring-danger-ink' : ''}`}
        />
        <button
          type="button"
          onClick={save}
          className="rounded-pill bg-accent-fill px-4 py-2 font-medium text-accent-ink hover:brightness-[0.97]"
        >
          Use
        </button>
      </div>
      {invalid && <p className="mt-1.5 text-danger-ink">Enter an address like 192.168.1.25 or 192.168.1.25:8787.</p>}

      <button
        type="button"
        onClick={() => void search()}
        disabled={searching}
        className="mt-3 flex items-center gap-2 rounded-pill px-3 py-1.5 text-ink-secondary hover:bg-hover disabled:opacity-60"
      >
        {searching
          ? <Loader2 className="size-4 animate-spin" aria-hidden />
          : <Radar className="size-4" aria-hidden />}
        {searching ? 'Looking on the office network…' : 'Find on the office network'}
      </button>

      {found && found.length === 0 && (
        <p className="mt-2 text-ink-secondary">
          No accounting server answered. Check that it is switched on, or type its address above.
        </p>
      )}

      {found && found.length > 0 && (
        <ul className="mt-2">
          {found.map((node) => (
            <li key={node.url}>
              <button
                type="button"
                onClick={() => use(node.url)}
                className="flex w-full items-center justify-between gap-3 rounded-md px-3 py-2 text-left hover:bg-hover"
              >
                <span>
                  <span className="block font-medium">{node.municipality}</span>
                  <span className="text-xs text-ink-muted">{node.node_code}</span>
                </span>
                <span className="tabular text-ink-secondary">{node.address}:{node.port}</span>
              </button>
            </li>
          ))}
        </ul>
      )}

      <button
        type="button"
        onClick={() => setOpen(false)}
        className="mt-3 text-ink-muted hover:underline"
      >
        Cancel
      </button>
    </div>
  )
}
