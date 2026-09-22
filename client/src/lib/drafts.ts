/**
 * Local drafts.
 *
 * Wraps the Tauri commands backed by the encrypted store in
 * `desktop/src/drafts.rs`. Per docs/ARCHITECTURE.md §2 a workstation may hold
 * drafts but never creates a ledger fact, so everything here is unfinished
 * work waiting for the office node.
 *
 * In a plain browser there is no local store — the browser build serves the
 * read-only dashboards, and anyone recording entries is on the desktop client.
 * The functions degrade to "not available" rather than throwing, so a browser
 * user sees the ordinary offline message instead of a broken screen.
 */

import { inDesktopShell } from '@/lib/secureStore'

export interface Draft {
  id: string
  /** `voucher`, `obligation`, `journal_entry`. */
  kind: string
  /** One line for the drafts list, so it reads without opening the payload. */
  summary: string
  /** The form contents, exactly as the screen would have submitted them. */
  payload: string
  created_at: string
  updated_at: string
  attempts: number
  last_error: string | null
}

/** What a draft needs to be created; the rest is filled in here. */
export interface NewDraft {
  kind: string
  summary: string
  /** The request body that failed to send. */
  body: unknown
  /** Where it should be sent when the node returns. */
  endpoint: string
}

async function invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke: tauriInvoke } = await import('@tauri-apps/api/core')
  return tauriInvoke<T>(command, args)
}

/** Whether this build can hold drafts at all. */
export function draftsAvailable(): boolean {
  return inDesktopShell()
}

/**
 * A time-ordered id, assigned here so the draft keeps its identity all the way
 * to the node.
 *
 * That is what makes a re-send an upsert on a known id rather than a second
 * document — the first of the six duplication defences in ARCHITECTURE.md §3.
 * Uses the same UUIDv7 layout as the server: 48 bits of millisecond timestamp,
 * then random.
 */
export function newDraftId(): string {
  const now = BigInt(Date.now())
  const timeHex = now.toString(16).padStart(12, '0')

  const random = crypto.getRandomValues(new Uint8Array(10))
  // Version 7 in the high nibble of byte 6, variant 0b10 in byte 8.
  random[0] = ((random[0] ?? 0) & 0x0f) | 0x70
  random[2] = ((random[2] ?? 0) & 0x3f) | 0x80

  const randomHex = [...random].map((b) => b.toString(16).padStart(2, '0')).join('')

  return [
    timeHex.slice(0, 8),
    timeHex.slice(8, 12),
    randomHex.slice(0, 4),
    randomHex.slice(4, 8),
    randomHex.slice(8, 20),
  ].join('-')
}

export async function saveDraft(draft: NewDraft): Promise<Draft | null> {
  if (!draftsAvailable()) return null

  const now = new Date().toISOString()
  const record: Draft = {
    id: newDraftId(),
    kind: draft.kind,
    summary: draft.summary,
    payload: JSON.stringify({ endpoint: draft.endpoint, body: draft.body }),
    created_at: now,
    updated_at: now,
    attempts: 0,
    last_error: null,
  }

  await invoke('draft_save', { draft: record })
  return record
}

export async function listDrafts(kind?: string): Promise<Draft[]> {
  if (!draftsAvailable()) return []
  return invoke<Draft[]>('draft_list', { kind: kind ?? null })
}

export async function removeDraft(id: string): Promise<void> {
  if (!draftsAvailable()) return
  await invoke('draft_remove', { id })
}

export async function recordDraftFailure(id: string, error: string): Promise<void> {
  if (!draftsAvailable()) return
  await invoke('draft_record_failure', { id, error })
}

export async function countDrafts(): Promise<number> {
  if (!draftsAvailable()) return 0
  try {
    return await invoke<number>('draft_count')
  } catch {
    // A count is decoration on a status bar; failing to read it must not
    // interrupt whatever the person is doing.
    return 0
  }
}

export interface DiscoveredNode {
  url: string
  address: string
  port: number
  node_code: string
  municipality: string
  tenant: string
  version: string
}

/** Searches the office network for accounting servers. */
export async function discoverNodes(): Promise<DiscoveredNode[]> {
  if (!draftsAvailable()) return []
  return invoke<DiscoveredNode[]>('discover_nodes')
}
