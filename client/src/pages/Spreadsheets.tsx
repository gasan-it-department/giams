import { useEffect, useRef, useState, type DragEvent } from 'react'
import { FilePlus2, FileSpreadsheet, FolderOpen, Loader2, Save } from 'lucide-react'
import { Workbook, type WorkbookInstance } from '@fortune-sheet/react'
import type { Sheet } from '@fortune-sheet/core'
import { transformExcelToFortune, transformFortuneToExcel } from '@corbe30/fortune-excel'
import '@fortune-sheet/react/dist/index.css'
import { EditorBoundary } from '@/components/EditorBoundary'
import { ErrorNotice } from '@/components/ErrorNotice'
import { saveFile, XLSX_TYPE } from '@/lib/saveFile'
import { polishExport } from '@/lib/spreadsheetFile'

/**
 * A spreadsheet editor for the office's own Excel files.
 *
 * Open an .xlsx or .csv, merge cells, change fonts and fills, drag row and
 * column edges, and save a copy. It is a file editor and nothing more: a
 * number changed here changes the file, never the books. Saving always writes
 * a new file so the original can't be damaged.
 *
 * Loaded on its own (lazy route): the editor and its Excel bridge are most of
 * a megabyte and nobody else needs them.
 */

// the editing the office asked for, without the tools a round trip to .xlsx
// would lose (screenshots, images, comments, conditional formats)
const TOOLBAR = [
  'undo', 'redo', 'format-painter', 'clear-format', '|',
  'currency-format', 'percentage-format', 'number-decrease', 'number-increase', 'format', '|',
  'font', '|', 'font-size', '|',
  'bold', 'italic', 'strike-through', 'underline', '|',
  'font-color', 'background', 'border', 'merge-cell', '|',
  'horizontal-align', 'vertical-align', 'text-wrap', '|',
  'freeze', 'filter', 'quick-formula', 'search',
]

const BLANK: Sheet[] = [{ name: 'Sheet1', celldata: [], order: 0, status: 1 }]

const ACCEPT = '.xlsx,.csv'

function copyName(name: string): string {
  const base = name.replace(/\.(xlsx|csv)$/i, '')
  return `${base} (edited).xlsx`
}

export function Spreadsheets() {
  const sheetRef = useRef<WorkbookInstance>(null)
  const inputRef = useRef<HTMLInputElement>(null)

  const [sheets, setSheets] = useState<Sheet[] | null>(null)
  // a new key remounts the editor, which is how a freshly opened file replaces the old one
  const [key, setKey] = useState(0)
  const [fileName, setFileName] = useState('')
  const [dirty, setDirty] = useState(false)
  const [busy, setBusy] = useState<'opening' | 'saving' | null>(null)
  const [failure, setFailure] = useState<unknown>(null)
  const [pending, setPending] = useState<File | 'blank' | null>(null)
  // sheets whose column widths and row heights still need applying
  const layoutFor = useRef<Sheet[] | null>(null)
  // the editor reports changes while it lays a file out; those aren't edits
  const settled = useRef(false)
  // the file as opened, the reference for fixing up the saved copy
  const original = useRef<ArrayBuffer | null>(null)

  // once the editor for the new file has mounted, size its columns and rows,
  // but only for sheets it actually holds
  useEffect(() => {
    settled.current = false
    const loaded = layoutFor.current
    let settle: ReturnType<typeof setTimeout> | undefined
    const timer = setTimeout(() => {
      const editor = sheetRef.current
      if (loaded && editor) {
        layoutFor.current = null
        const held = new Set(editor.getAllSheets().map((s) => s.id))
        for (const sheet of loaded) {
          if (!sheet.id || !held.has(sheet.id)) continue
          editor.setColumnWidth(sheet.config?.columnlen ?? {}, { id: sheet.id })
          editor.setRowHeight(sheet.config?.rowlen ?? {}, { id: sheet.id })
        }
      }
      settle = setTimeout(() => { settled.current = true; setDirty(false) }, 300)
    }, 50)
    return () => { clearTimeout(timer); clearTimeout(settle) }
  }, [key])

  // dev only: lets a test script drive the editor
  useEffect(() => {
    if (import.meta.env.DEV) (window as unknown as { __sheet?: typeof sheetRef }).__sheet = sheetRef
  }, [])

  async function open(file: File) {
    if (!/\.(xlsx|csv)$/i.test(file.name)) {
      setFailure(new Error(`${file.name} is not a spreadsheet this editor can open. Open an .xlsx or .csv file.`))
      return
    }
    setFailure(null)
    setBusy('opening')
    try {
      // the add-on sizes columns 1 ms after loading, before the editor has the
      // sheets, and that throws while drawing. it gets no editor to call;
      // the layout is applied below once the editor has them.
      await transformExcelToFortune(file, (loaded: Sheet[]) => {
        layoutFor.current = loaded
        setSheets(loaded)
      }, setKey, { current: null })
      // kept to restore what the export drops (page setup, hidden widths)
      original.current = /\.xlsx$/i.test(file.name) ? await file.arrayBuffer() : null
      setFileName(file.name)
      setDirty(false)
    } catch {
      setFailure(new Error(`${file.name} could not be opened. It may be damaged, password-protected, or an older .xls file; save it as .xlsx in Excel and try again.`))
    } finally {
      setBusy(null)
    }
  }

  function startBlank() {
    setFailure(null)
    setSheets(BLANK.map((s) => ({ ...s, celldata: [] })))
    original.current = null
    setKey((k) => k + 1)
    setFileName('New spreadsheet.xlsx')
    setDirty(false)
  }

  // with unsaved edits, ask in the page before throwing them away
  function request(next: File | 'blank') {
    if (dirty) {
      setPending(next)
      return
    }
    if (next === 'blank') startBlank()
    else void open(next)
  }

  function confirmDiscard() {
    const next = pending
    setPending(null)
    setDirty(false)
    if (next === 'blank') startBlank()
    else if (next) void open(next)
  }

  // true once the copy is handed over, so "save, then continue" never discards on a failure
  async function saveCopy(): Promise<boolean> {
    setFailure(null)
    setBusy('saving')
    try {
      // the add-on's IFileType enum isn't exported; its xlsx member is this string
      // exporting stirs the editor's change events; they aren't edits
      settled.current = false
      const exported = await transformFortuneToExcel(sheetRef, 'xlsx' as Parameters<typeof transformFortuneToExcel>[1], false)
      const blob = await polishExport(new Blob([exported], { type: XLSX_TYPE }), original.current)
      saveFile(blob, copyName(fileName))
      setDirty(false)
      setTimeout(() => { settled.current = true }, 500)
      return true
    } catch {
      setFailure(new Error('The spreadsheet could not be saved. Your edits are still here; try Save a copy again.'))
      return false
    } finally {
      setBusy(null)
    }
  }

  function onDrop(e: DragEvent) {
    e.preventDefault()
    const file = e.dataTransfer.files[0]
    if (file) request(file)
  }

  const picker = (
    <input
      ref={inputRef}
      type="file"
      accept={ACCEPT}
      className="hidden"
      onChange={(e) => {
        const file = e.target.files?.[0]
        e.target.value = ''
        if (file) request(file)
      }}
    />
  )

  // ── nothing open yet ────────────────────────────────────────────────────
  if (!sheets) {
    return (
      <div
        className="flex min-h-0 flex-1 items-center justify-center p-6"
        onDragOver={(e) => e.preventDefault()}
        onDrop={onDrop}
      >
        {picker}
        <div className="w-full max-w-[460px] text-center">
          <div className="mx-auto mb-4 flex size-14 items-center justify-center rounded-lg bg-accent-fill text-accent-ink">
            <FileSpreadsheet className="size-7" aria-hidden />
          </div>
          <h1 className="text-lg font-medium">Spreadsheets</h1>
          <p className="mt-1 text-ink-secondary">
            Open an Excel file to view and edit it: merge cells, change fonts and fills, resize rows and columns.
            Drop a file here, or choose one.
          </p>

          {failure != null && <div className="mt-4 text-left"><ErrorNotice error={failure} /></div>}

          <div className="mt-6 flex flex-wrap justify-center gap-2">
            <button
              type="button"
              onClick={() => inputRef.current?.click()}
              disabled={busy !== null}
              className="flex items-center gap-2 rounded-pill bg-accent-fill px-4 py-2 font-medium text-accent-ink hover:brightness-[0.97] disabled:opacity-60"
            >
              {busy === 'opening' ? <Loader2 className="size-4 animate-spin" aria-hidden /> : <FolderOpen className="size-4" aria-hidden />}
              {busy === 'opening' ? 'Opening…' : 'Open a file'}
            </button>
            <button
              type="button"
              onClick={startBlank}
              className="flex items-center gap-2 rounded-pill px-4 py-2 text-ink-secondary hover:bg-hover"
            >
              <FilePlus2 className="size-4" aria-hidden />
              New blank sheet
            </button>
          </div>

          <p className="mt-6 text-sm text-ink-muted">
            .xlsx and .csv. Saving makes a new copy; the file you opened is never changed.
          </p>
        </div>
      </div>
    )
  }

  // ── editing ─────────────────────────────────────────────────────────────
  return (
    <div className="flex min-h-0 flex-1 flex-col" onDragOver={(e) => e.preventDefault()} onDrop={onDrop}>
      {picker}

      <div className="no-print flex flex-wrap items-center gap-2 border-b border-line px-3 py-1.5">
        <FileSpreadsheet className="size-[18px] shrink-0 text-ok-ink" aria-hidden />
        <span className="min-w-0 truncate font-medium" title={fileName}>{fileName}</span>
        {dirty && <span className="shrink-0 text-xs text-ink-muted">Edited</span>}

        <div className="ml-auto flex shrink-0 items-center gap-1">
          <button
            type="button"
            onClick={() => inputRef.current?.click()}
            className="flex items-center gap-2 whitespace-nowrap rounded-pill px-3 py-1 text-sm hover:bg-hover"
          >
            <FolderOpen className="size-4" aria-hidden />
            Open
          </button>
          <button
            type="button"
            onClick={() => request('blank')}
            className="flex items-center gap-2 whitespace-nowrap rounded-pill px-3 py-1 text-sm hover:bg-hover"
          >
            <FilePlus2 className="size-4" aria-hidden />
            New
          </button>
          <button
            type="button"
            onClick={() => void saveCopy()}
            disabled={busy !== null}
            className="ml-1 flex items-center gap-2 whitespace-nowrap rounded-pill bg-accent-fill px-3 py-1 text-sm font-medium text-accent-ink hover:brightness-[0.97] disabled:opacity-60"
          >
            {busy === 'saving' ? <Loader2 className="size-4 animate-spin" aria-hidden /> : <Save className="size-4" aria-hidden />}
            Save a copy
          </button>
        </div>
      </div>

      {pending && (
        <div className="no-print flex flex-wrap items-center gap-3 border-b border-line bg-warn-fill px-4 py-2 text-sm text-warn-ink">
          <span className="flex-1">Your edits to {fileName} haven't been saved. Save a copy first, or discard them.</span>
          <button type="button" onClick={() => void saveCopy().then((saved) => { if (saved) confirmDiscard() })} className="rounded-pill bg-surface px-3 py-1 font-medium hover:bg-hover">
            Save a copy, then continue
          </button>
          <button type="button" onClick={confirmDiscard} className="rounded-pill px-3 py-1 hover:bg-hover">
            Discard edits
          </button>
          <button type="button" onClick={() => setPending(null)} className="rounded-pill px-3 py-1 hover:bg-hover">
            Cancel
          </button>
        </div>
      )}

      <p className="no-print border-b border-line bg-info-fill px-4 py-1.5 text-xs text-info-ink">
        Editing a spreadsheet changes this file only. It never changes the books; corrections to the ledger are made with a JEV.
      </p>

      {failure != null && <div className="p-3"><ErrorNotice error={failure} /></div>}

      {/* the editor sizes itself to its parent, so the parent needs a real height */}
      <div className="relative min-h-0 flex-1">
        <div className="absolute inset-0">
          <EditorBoundary key={key} onReset={() => { setSheets(null); setDirty(false) }}>
            <Workbook
              key={key}
              ref={sheetRef}
              data={sheets}
              lang="en"
              currency="₱"
              toolbarItems={TOOLBAR}
              onChange={() => { if (settled.current) setDirty(true) }}
            />
          </EditorBoundary>
        </div>
      </div>
    </div>
  )
}
