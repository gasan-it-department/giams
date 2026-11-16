import type ExcelJS from 'exceljs'
import { XLSX_TYPE } from '@/lib/saveFile'

/**
 * Corrects what the editor's Excel export gets wrong, using the file that
 * was opened as the reference.
 *
 *   * colours come out as six hex digits ("c00000"); Excel reads ARGB, eight
 *   * page setup (paper size, orientation, fit to page, margins, print titles)
 *     and header/footer are dropped; they're copied back per sheet by name
 *   * a hidden column loses its width, so unhiding it later shows a default
 *     width; it gets the original back
 *
 * Without an original (a new blank sheet) only the colours are fixed.
 */
export async function polishExport(exported: Blob, original: ArrayBuffer | null): Promise<Blob> {
  const { default: Excel } = await import('exceljs')

  const out = new Excel.Workbook()
  await out.xlsx.load(await exported.arrayBuffer())

  let source: ExcelJS.Workbook | null = null
  if (original) {
    source = new Excel.Workbook()
    await source.xlsx.load(original)
  }

  for (const ws of out.worksheets) {
    fixColours(ws)

    const was = source?.getWorksheet(ws.name)
    if (!was) continue

    ws.pageSetup = { ...ws.pageSetup, ...was.pageSetup }
    ws.headerFooter = { ...ws.headerFooter, ...was.headerFooter }

    for (let i = 1; i <= ws.columnCount; i++) {
      const now = ws.getColumn(i)
      const then = was.getColumn(i)
      if (now.hidden && then.hidden && then.width) now.width = then.width
    }
  }

  const buffer = await out.xlsx.writeBuffer()
  return new Blob([buffer], { type: XLSX_TYPE })
}

// ── colours ────────────────────────────────────────────────────────────────
// "c00000" → "FFC00000"; anything already eight digits is left alone

function argb(color: Partial<ExcelJS.Color> | undefined): void {
  if (!color?.argb) return
  const hex = color.argb.replace(/^#/, '')
  if (/^[0-9a-f]{6}$/i.test(hex)) color.argb = `FF${hex.toUpperCase()}`
}

function fixColours(ws: ExcelJS.Worksheet): void {
  ws.eachRow({ includeEmpty: true }, (row) => {
    row.eachCell({ includeEmpty: true }, (cell) => {
      argb(cell.font?.color)
      const fill = cell.fill
      if (fill?.type === 'pattern') {
        argb(fill.fgColor)
        argb(fill.bgColor)
      }
      const border = cell.border
      if (border) {
        for (const side of ['top', 'left', 'bottom', 'right', 'diagonal'] as const) {
          argb(border[side]?.color)
        }
      }
    })
  })
}
