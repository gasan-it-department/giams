import { create } from 'zustand'
import { api, ApiError } from '@/lib/api'
import {
  countDrafts, draftsAvailable, listDrafts, recordDraftFailure, removeDraft, saveDraft,
  type Draft,
} from '@/lib/drafts'

/**
 * Work waiting for the office node.
 *
 * Deliberately not persisted by Zustand — the drafts themselves live in the
 * encrypted local store, and this holds only the count and the flush state that
 * drives the status indicator.
 */

interface DraftState {
  pending: number
  flushing: boolean
  /** Drafts the node refused, as opposed to could not be reached. */
  rejected: Draft[]

  refresh: () => Promise<void>
  /** Sends a request, keeping it as a draft if the node cannot be reached. */
  submit: <T>(endpoint: string, body: unknown, describe: DraftDescription) => Promise<T | null>
  flush: () => Promise<void>
}

export interface DraftDescription {
  kind: string
  summary: string
}

export const useDraftStore = create<DraftState>((set, get) => ({
  pending: 0,
  flushing: false,
  rejected: [],

  async refresh() {
    set({ pending: await countDrafts() })
  },

  /**
   * The offline fallback.
   *
   * Only `NODE_UNREACHABLE` becomes a draft. A rejected posting — an
   * unbalanced entry, an obligation over its allotment, a closed period — is a
   * decision the node has already made, and would be rejected identically on
   * every retry. Keeping it as a draft would present a permanent failure as
   * pending work.
   *
   * Returns `null` when the work was kept locally, so the caller can tell the
   * person their entry is saved rather than sent.
   */
  async submit<T>(endpoint: string, body: unknown, describe: DraftDescription): Promise<T | null> {
    try {
      return await api.post<T>(endpoint, body)
    } catch (e) {
      const unreachable = e instanceof ApiError && e.code === 'NODE_UNREACHABLE'

      if (!unreachable || !draftsAvailable()) throw e

      await saveDraft({ ...describe, endpoint, body })
      await get().refresh()
      return null
    }
  },

  /**
   * Sends everything waiting, oldest first.
   *
   * Order matters: a voucher raised before another may depend on it, and
   * sending them out of order can fail in ways that sending them in order
   * would not.
   *
   * A draft is removed **only** once the node confirms it. If the connection
   * drops mid-flush the draft stays, and re-sending it is an upsert on its
   * existing id rather than a duplicate.
   */
  async flush() {
    if (get().flushing || !draftsAvailable()) return

    set({ flushing: true })
    const rejected: Draft[] = []

    try {
      const drafts = [...(await listDrafts())].sort((a, b) =>
        a.created_at.localeCompare(b.created_at),
      )

      for (const draft of drafts) {
        let parsed: { endpoint: string; body: unknown }
        try {
          parsed = JSON.parse(draft.payload) as { endpoint: string; body: unknown }
        } catch {
          await recordDraftFailure(draft.id, 'This draft could not be read.')
          rejected.push(draft)
          continue
        }

        try {
          await api.post(parsed.endpoint, parsed.body)
          await removeDraft(draft.id)
        } catch (e) {
          if (e instanceof ApiError && e.code === 'NODE_UNREACHABLE') {
            // The connection went again. Stop rather than marking every
            // remaining draft as failed for the same reason.
            break
          }

          // The node considered it and said no. Keep it, record why, and show
          // it — retrying forever would hide a problem only a person can fix.
          const reason = e instanceof ApiError ? e.message : 'This could not be sent.'
          await recordDraftFailure(draft.id, reason)
          rejected.push({ ...draft, last_error: reason })
        }
      }
    } finally {
      set({ flushing: false, rejected })
      await get().refresh()
    }
  },
}))
