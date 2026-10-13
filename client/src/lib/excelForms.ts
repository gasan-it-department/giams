import type ExcelJS from 'exceljs'
import type { Amount } from '@/lib/money'

/**
 * Fills the office's own Excel forms with an entry's data and hands back the
 * file to print.
 *
 * This is deliberately not a re-creation of the forms. The workbook the office
 * submitted (`Accounting forms.xlsx`) is bundled unchanged and loaded at run
 * time; we write values into the exact cells and write nothing else, so every
 * border, font, merge and column width stays theirs. What comes out is their
 * form — because it is their form — with the entry typed in.
 *
 * ExcelJS keeps a cell's existing style when only its `.value` is set, which is
 * the whole reason this works: setting `H13` to a number leaves `H13`'s borders
 * and number format exactly as the template drew them.
 */

const TEMPLATE_URL = '/templates/accounting-forms.xlsx'

export interface JevExcelLine {
  code: string
  account: string
  particulars: string
  debit: Amount
  credit: Amount
}

export interface JevExcelData {
  jevNo: string | null
  date: string
  lines: JevExcelLine[]
  totalDebit: Amount
  totalCredit: Amount
}

/** JEV sheet layout, read from the template: data rows 13–28, totals on 29. */
const JEV = {
  sheet: 'JEV',
  no: 'J5',
  date: 'J6',
  firstRow: 13,
  lastRow: 28,
  totalDebit: 'H29',
  totalCredit: 'K29',
  cell: (col: string, row: number) => `${col}${row}`,
} as const

/**
 * Builds the filled JEV workbook and prompts the browser to save it.
 *
 * Amounts are written as numbers so the template's own number format renders
 * them (12,500.00) and Excel treats the column as figures. A single peso amount
 * to two places is exact as a number; the float rule this system enforces is
 * about *arithmetic*, and no arithmetic happens here — the totals are computed
 * upstream in exact centavos and passed in already summed.
 */
export async function downloadJevExcel(data: JevExcelData): Promise<void> {
  const workbook = await loadTemplate()
  const sheet = workbook.getWorksheet(JEV.sheet)
  if (!sheet) throw new Error('The JEV sheet is missing from the form template.')

  sheet.getCell(JEV.no).value = data.jevNo ?? ''
  sheet.getCell(JEV.date).value = data.date

  const rows = JEV.lastRow - JEV.firstRow + 1
  data.lines.slice(0, rows).forEach((line, i) => {
    const row = JEV.firstRow + i
    sheet.getCell(JEV.cell('C', row)).value = line.account
    sheet.getCell(JEV.cell('F', row)).value = line.code
    if (isPositive(line.debit)) money(sheet.getCell(JEV.cell('H', row)), line.debit)
    if (isPositive(line.credit)) money(sheet.getCell(JEV.cell('K', row)), line.credit)
  })

  money(sheet.getCell(JEV.totalDebit), data.totalDebit)
  money(sheet.getCell(JEV.totalCredit), data.totalCredit)

  // The office's blank template runs wider than one portrait page, so the
  // Credit column spills onto a second sheet. Fit the columns to one page
  // width so a voucher prints on a single sheet, as it is meant to.
  //
  // Margins are pulled in tight and the form centred, so with fit-to-width the
  // voucher fills the sheet rather than sitting small inside wide default
  // margins. 0.2" clears the unprintable edge on ordinary office printers.
  sheet.pageSetup.fitToPage = true
  sheet.pageSetup.fitToWidth = 1
  sheet.pageSetup.fitToHeight = 1
  sheet.pageSetup.horizontalCentered = true
  sheet.pageSetup.verticalCentered = true
  sheet.pageSetup.margins = {
    left: 0.2, right: 0.2, top: 0.2, bottom: 0.2, header: 0, footer: 0,
  }

  // Only the JEV goes to the printer, so the other forms in the workbook are
  // dropped. Done by name to leave the JEV — and its ruled arrow — untouched.
  for (const ws of [...workbook.worksheets]) {
    if (ws.name !== JEV.sheet) workbook.removeWorksheet(ws.id)
  }

  const name = data.jevNo ? `JEV-${data.jevNo}` : `JEV-${data.date}`
  await save(workbook, `${name}.xlsx`)
}

async function loadTemplate(): Promise<ExcelJS.Workbook> {
  const response = await fetch(TEMPLATE_URL)
  if (!response.ok) {
    throw new Error('The form template could not be loaded. Reload the page and try again.')
  }
  const buffer = await response.arrayBuffer()
  // loaded on first export rather than at startup: it is most of the bundle
  const { default: Excel } = await import('exceljs')
  const workbook = new Excel.Workbook()
  await workbook.xlsx.load(buffer)
  return workbook
}

async function save(workbook: ExcelJS.Workbook, filename: string): Promise<void> {
  const buffer = await workbook.xlsx.writeBuffer()
  const blob = new Blob([buffer], {
    type: 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
  })
  const url = URL.createObjectURL(blob)
  const anchor = document.createElement('a')
  anchor.href = url
  anchor.download = filename
  document.body.append(anchor)
  anchor.click()
  anchor.remove()
  // Revoked on the next tick so the click has taken the download.
  setTimeout(() => URL.revokeObjectURL(url), 1000)
}

/**
 * Writes an amount as a number the office's way: grouped, two decimals.
 *
 * The one place an amount becomes a JavaScript number, deliberately. An .xlsx
 * stores every number as a double whatever writes it, so a string would only
 * turn the column into text Excel cannot total. Nothing is added or multiplied
 * here — the totals arrive already summed in exact centavos — and a peso
 * figure of up to fifteen digits survives the conversion unchanged.
 */
function money(cell: ExcelJS.Cell, amount: Amount): void {
  cell.value = Number(amount) // check-money: allow — written once to a cell Excel keeps as a double
  cell.numFmt = '#,##0.00'
}

function isPositive(amount: Amount): boolean {
  return !!amount && amount !== '0.00' && !amount.startsWith('-')
}
