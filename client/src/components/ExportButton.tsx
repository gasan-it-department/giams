import { useState } from 'react'
import { Download, Loader2 } from 'lucide-react'
import { api } from '@/lib/api'
import { ErrorNotice } from '@/components/ErrorNotice'

/**
 * Saves the page currently on screen as a spreadsheet.
 *
 * The export is built by the server from the same query the screen ran, not
 * from what is rendered — so a filtered page and its export always agree, and
 * the figures land in Excel as numbers the office can sum.
 *
 * A failure is reported in place, under the button, rather than as a dialog:
 * an export that quietly does nothing is the worst outcome, because the person
 * goes looking in their Downloads folder for a file that was never written.
 */
export function ExportButton({ path, filename }: { path: string; filename: string }) {
  const [busy, setBusy] = useState(false)
  const [problem, setProblem] = useState<unknown>(null)

  async function save() {
    setBusy(true)
    setProblem(null)
    try {
      await api.download(path, filename)
    } catch (error) {
      setProblem(error)
    } finally {
      setBusy(false)
    }
  }

  return (
    <span className="relative">
      <button
        type="button"
        onClick={() => void save()}
        disabled={busy}
        className="ml-2 flex items-center gap-2 rounded-pill px-3 py-1 text-sm hover:bg-hover disabled:opacity-60"
      >
        {busy
          ? <Loader2 className="size-4 animate-spin" aria-hidden />
          : <Download className="size-4" aria-hidden />}
        {busy ? 'Preparing…' : 'Export'}
      </button>

      {problem != null && (
        <span className="absolute right-0 top-full z-10 mt-1 block w-[40ch]">
          <ErrorNotice error={problem} fallback="The spreadsheet could not be prepared." />
        </span>
      )}
    </span>
  )
}
