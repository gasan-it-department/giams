import type ExcelJS from 'exceljs'
import type { BookLine, BookPage, TrialBalancePage } from '@/queries/books'
import { isPositive, loadTemplate, money, save } from '@/lib/excelForms'
import type { Amount } from '@/lib/money'

/**
 * The books of accounts on the office's own sheets: GL (Annex 1, the general
 * journal), CRJ, CKDJ, CDJ and TB from `Accounting forms.xlsx`, filled the way
 * the JEV is — values into their cells, their borders and fonts untouched.
 *
 * The journals are columnar. Cash goes in the sheet's own cash column; every
 * other account goes under Sundry with its code. The blank columns between are
 * the ones an office titles for its frequent accounts, and stay blank.
 *
 * A month with more lines than a sheet is ruled for gets extra ruled rows, so
 * nothing is ever cut off.
 */

type Book = 'GJ' | 'CRJ' | 'CKDJ' | 'CDJ'

const SHEET: Record<Book | 'TB', string> = { GJ: 'GL', CRJ: 'CRJ', CKDJ: 'CKDJ', CDJ: 'CDJ', TB: 'TB' }

/** First and last ruled entry row of each journal, read from the template. */
const BODY: Record<Book, { first: number; last: number }> = {
  GJ:   { first: 11, last: 44 },
  CRJ:  { first: 11, last: 33 },
  CKDJ: { first: 12, last: 21 },
  CDJ:  { first: 11, last: 23 },
}

/** The column headings, repeated at the top of every printed page of a long book. */
const HEADING_ROWS: Record<Book | 'TB', string> = { GJ: '9:10', CRJ: '7:10', CKDJ: '9:11', CDJ: '8:10', TB: '6:7' }

const CASH_ON_HAND = '1-01-01-010' // Cash - Local Treasury
const isCash = (code: string) => code.startsWith('1-01-')
const isCashInBank = (code: string) => code.startsWith('1-01-02-') || code.startsWith('1-01-03-')

export async function downloadBookExcel(book: Book, page: BookPage, period: { from: string; to: string }): Promise<void> {
  const workbook = await loadTemplate()
  const ws = sheet(workbook, SHEET[book])
  keepOnly(workbook, ws)

  const month = monthLabel(period.from, period.to)
  const entries = byEntry(page.lines)

  // the heading lines are the template's own "LGU" / "Month: ____" cells
  set(ws, 'B4', page.municipality)
  if (book === 'GJ') {
    set(ws, 'B5', `Month: ${month}`)
    set(ws, 'B7', `Fund: ${page.fund_name}`)
  } else if (book === 'CRJ') {
    // this sheet has no Fund line of its own; it rides on the Month line
    set(ws, 'B5', `Month: ${month}      Fund: ${page.fund_name}`)
  } else if (book === 'CKDJ') {
    set(ws, 'B5', `Month: ${month}`)
    set(ws, 'B7', `Fund: ${page.fund_name}`)
  } else {
    set(ws, 'B5', `Month: ${month}`)
    set(ws, 'B6', `Fund: ${page.fund_name}`)
  }

  const rows: Row[] = book === 'GJ' ? generalJournal(entries)
    : book === 'CRJ' ? cashReceipts(entries)
    : book === 'CKDJ' ? checkDisbursements(entries)
    : cashDisbursements(entries)

  const { first } = BODY[book]
  makeRoom(ws, BODY[book].first, BODY[book].last, rows.length)

  rows.forEach((row, i) => {
    const r = first + i
    const free = row._free === 1
    for (const [col, value] of Object.entries(row)) {
      if (col.startsWith('_') || value === undefined || value === null || value === '') continue
      const cell = ws.getCell(`${col}${r}`)
      // the template's cells share style objects; this cell gets its own, or
      // a change here would also change every cell sharing it
      cell.style = { ...cell.style }
      if (typeof value === 'object') money(cell, value.amount)
      else cell.value = value
      // the paper form's columns are narrow; a long title or figure gets
      // smaller type instead of "#####" or a cut-off end. An explanation line
      // runs on across the empty columns instead, as it would on paper.
      cell.alignment = { ...cell.style.alignment, shrinkToFit: !free, wrapText: false }
    }
    // pairs of ruled cells under one heading read as one column: the general
    // journal's Debit and Credit, the receipts' collecting officer
    if (book === 'GJ') {
      mergeIfFilled(ws, `G${r}`, `G${r}:H${r}`)
      mergeIfFilled(ws, `I${r}`, `I${r}:J${r}`)
    }
    if (book === 'CRJ') mergeIfFilled(ws, `E${r}`, `E${r}:F${r}`)
  })

  fitToPage(ws, book === 'GJ' ? 'portrait' : 'landscape', HEADING_ROWS[book])
  await save(workbook, `${SHEET[book]} ${month}.xlsx`)
}

export async function downloadTrialBalanceExcel(page: TrialBalancePage): Promise<void> {
  const workbook = await loadTemplate()
  const ws = sheet(workbook, SHEET.TB)
  keepOnly(workbook, ws)

  set(ws, 'A1', page.municipality)
  set(ws, 'A3', page.fund_name.toUpperCase())
  set(ws, 'A4', `As of ${longDate(page.as_of)}`)

  // the sheet lists the office's accounts in rows 8-271, code in column B
  const FIRST = 8
  let last = 271
  const rowOf = new Map<string, number>()
  for (let r = FIRST; r <= last; r++) {
    const code = String(ws.getCell(`B${r}`).value ?? '').trim()
    if (code) rowOf.set(code, r)
    // the hidden C/D columns summed another sheet that isn't sent along
    ws.getCell(`C${r}`).value = null
    ws.getCell(`D${r}`).value = null
  }

  // accounts with a balance that the office's list doesn't have go at the end
  const missing = page.lines.filter((l) => !rowOf.has(l.account_code))
  if (missing.length) {
    makeRoom(ws, FIRST, last, last - FIRST + 1 + missing.length)
    missing.forEach((l, i) => {
      const r = last + 1 + i
      ws.getCell(`A${r}`).value = l.account_name
      ws.getCell(`B${r}`).value = l.account_code
      rowOf.set(l.account_code, r)
    })
    last += missing.length
  }

  for (const line of page.lines) {
    const r = rowOf.get(line.account_code)
    if (!r) continue
    if (isPositive(line.debit_balance)) money(ws.getCell(`E${r}`), line.debit_balance)
    if (isPositive(line.credit_balance)) money(ws.getCell(`F${r}`), line.credit_balance)
  }

  // the total row sits right under the list; its sums cover the list as it is now
  const total = last + 1
  ws.getCell(`C${total}`).value = null
  ws.getCell(`D${total}`).value = null
  ws.getCell(`E${total}`).value = { formula: `SUM(E${FIRST}:E${last})`, result: Number(page.total_debit) } // check-money: allow — Excel's own total of the column
  ws.getCell(`F${total}`).value = { formula: `SUM(F${FIRST}:F${last})`, result: Number(page.total_credit) } // check-money: allow — Excel's own total of the column
  ws.getCell(`D${total + 1}`).value = null
  ws.getCell(`F${total + 1}`).value = { formula: `E${total}-F${total}`, result: 0 }

  fitToPage(ws, 'portrait', HEADING_ROWS.TB)
  await save(workbook, `Trial Balance ${page.as_of}.xlsx`)
}

// ── the journals, line by line ──────────────────────────────────────────────

type Cellish = string | number | { amount: Amount } | undefined
type Row = Record<string, Cellish>

interface Entry {
  jevNo: string
  date: string
  particulars: string
  reference: string | null
  counterparty: string | null
  lines: BookLine[]
}

function byEntry(lines: BookLine[]): Entry[] {
  const out: Entry[] = []
  for (const line of lines) {
    let e = out[out.length - 1]
    if (!e || e.jevNo !== line.jev_no) {
      e = { jevNo: line.jev_no, date: line.entry_date, particulars: line.particulars,
            reference: line.reference, counterparty: line.counterparty, lines: [] }
      out.push(e)
    }
    e.lines.push(line)
  }
  return out
}

const amt = (a: Amount) => (isPositive(a) ? { amount: a } : undefined)

/** Annex 1: account titles under each entry (credits indented), then its explanation. */
function generalJournal(entries: Entry[]): Row[] {
  const rows: Row[] = []
  for (const e of entries) {
    const ordered = [...e.lines.filter((l) => isPositive(l.debit)), ...e.lines.filter((l) => !isPositive(l.debit))]
    ordered.forEach((l, i) => rows.push({
      B: i === 0 ? shortDate(e.date) : undefined,
      C: i === 0 ? shortJev(e.jevNo) : undefined,
      D: isPositive(l.debit) ? l.account_name : `      ${l.account_name}`,
      E: l.account_code,
      G: amt(l.debit),
      I: amt(l.credit),
    }))
    if (e.particulars) rows.push({ D: `(${e.particulars})`, _free: 1 })
  }
  return rows
}

/**
 * Annex 2. A collection debits Cash - Local Treasury (column G) and credits
 * income under Sundry (M/O/P); a deposit debits the bank under the deposits'
 * Sundry (U/W) and credits Cash - Local Treasury (column X).
 */
function cashReceipts(entries: Entry[]): Row[] {
  const rows: Row[] = []
  for (const e of entries) {
    const deposit = e.lines.some((l) => l.account_code === CASH_ON_HAND && isPositive(l.credit))
    // cash on the entry's first line; the other accounts down the two Sundry
    // sections, side by side
    const cash: Row = {}
    const collection: Row[] = []
    const deposits: Row[] = []
    for (const l of e.lines) {
      if (l.account_code === CASH_ON_HAND) {
        if (isPositive(l.debit)) cash.G = amt(l.debit)
        else cash.X = amt(l.credit)
      } else if (deposit && isPositive(l.debit)) {
        deposits.push({ U: l.account_code, W: amt(l.debit) })
      } else {
        collection.push({ M: l.account_code, O: amt(l.debit), P: amt(l.credit) })
      }
    }
    rows.push(...entryRows(e, cash, collection, deposits))
  }
  return rows
}

/** Annex 3: the bank credited in column G, everything else under Sundry (N/P/Q). */
function checkDisbursements(entries: Entry[]): Row[] {
  const rows: Row[] = []
  for (const e of entries) {
    const cash: Row = {}
    const sundry: Row[] = []
    for (const l of e.lines) {
      if (isCashInBank(l.account_code) && isPositive(l.credit) && !cash.G) cash.G = amt(l.credit)
      else sundry.push({ N: l.account_code, P: amt(l.debit), Q: amt(l.credit) })
    }
    rows.push(...entryRows(e, cash, sundry))
  }
  return rows
}

/** Annex 4: cash credited in column G, everything else under Sundry (O/Q/R). */
function cashDisbursements(entries: Entry[]): Row[] {
  const rows: Row[] = []
  for (const e of entries) {
    const cash: Row = {}
    const sundry: Row[] = []
    for (const l of e.lines) {
      if (isCash(l.account_code) && isPositive(l.credit) && !cash.G) cash.G = amt(l.credit)
      else sundry.push({ O: l.account_code, Q: amt(l.debit), R: amt(l.credit) })
    }
    rows.push(...entryRows(e, cash, sundry))
  }
  return rows
}

/**
 * One entry's lines: date, reference, JEV number, name and the cash amount on
 * the first; the sundry lists laid down the rows beside each other.
 */
function entryRows(e: Entry, cash: Row, ...lists: Row[][]): Row[] {
  const height = Math.max(1, ...lists.map((l) => l.length))
  const out: Row[] = []
  for (let i = 0; i < height; i++) {
    const row: Row = i === 0
      ? { B: shortDate(e.date), C: e.reference ?? undefined, D: shortJev(e.jevNo), E: e.counterparty ?? undefined, ...cash }
      : {}
    for (const list of lists) Object.assign(row, list[i] ?? {})
    out.push(row)
  }
  return out
}

// ── the sheet ───────────────────────────────────────────────────────────────

function sheet(workbook: ExcelJS.Workbook, name: string): ExcelJS.Worksheet {
  const ws = workbook.getWorksheet(name)
  if (!ws) throw new Error(`The ${name} sheet is missing from the form template.`)
  return ws
}

/** Only the one form goes out; the rest of the workbook is dropped. */
function keepOnly(workbook: ExcelJS.Workbook, keep: ExcelJS.Worksheet): void {
  for (const ws of [...workbook.worksheets]) if (ws.id !== keep.id) workbook.removeWorksheet(ws.id)
}

function set(ws: ExcelJS.Worksheet, address: string, text: string): void {
  ws.getCell(address).value = text
}

/**
 * Adds ruled rows (copies of the last one) when there are more lines than the
 * sheet has, and moves the signature block's merged cells down with them;
 * ExcelJS shifts the cells but not the merges.
 */
function makeRoom(ws: ExcelJS.Worksheet, first: number, last: number, needed: number): void {
  const extra = needed - (last - first + 1)
  if (extra <= 0) return

  const below = (ws.model.merges ?? []).filter((m) => topRow(m) > last)
  below.forEach((m) => ws.unMergeCells(m))
  ws.duplicateRow(last, extra, true)
  below.forEach((m) => ws.mergeCells(shiftRows(m, extra)))
}

function mergeIfFilled(ws: ExcelJS.Worksheet, cell: string, range: string): void {
  if (ws.getCell(cell).value === null || ws.getCell(cell).value === undefined) return
  ws.mergeCells(range)
}

function columnLetter(n: number): string {
  let s = ''
  for (let c = n; c > 0; c = Math.floor((c - 1) / 26)) s = String.fromCharCode(65 + ((c - 1) % 26)) + s
  return s
}

function topRow(range: string): number {
  return Number(range.match(/\d+/)?.[0] ?? 0)
}

function shiftRows(range: string, by: number): string {
  return range.replace(/([A-Z]+)(\d+)/g, (_, col: string, row: string) => `${col}${Number(row) + by}`)
}

/**
 * One page wide and as many pages tall as the book needs, with the column
 * headings at the top of each; a short month still fits its one sheet.
 */
function fitToPage(ws: ExcelJS.Worksheet, orientation: 'portrait' | 'landscape', headings: string): void {
  ws.pageSetup.orientation = orientation
  ws.pageSetup.fitToPage = true
  ws.pageSetup.fitToWidth = 1
  ws.pageSetup.fitToHeight = 0
  ws.pageSetup.printTitlesRow = headings
  // without a named range Excel's automatic height dropped the last column
  // (the receipts' CREDIT); this pins the whole form, every added row included
  ws.pageSetup.printArea = `A1:${columnLetter(ws.columnCount)}${ws.rowCount}`
  ws.pageSetup.horizontalCentered = true
  ws.pageSetup.margins = { left: 0.3, right: 0.3, top: 0.4, bottom: 0.4, header: 0.2, footer: 0.2 }
}
/**
 * A JEV number as a book writes it: "JEV-2026-11-00001" -> "11-00001". The
 * book's heading already says the year, and month-and-sequence stays unique,
 * so the narrow JEV column holds it at a readable size.
 */
function shortJev(jevNo: string): string {
  const m = /^JEV-\d{4}-(\d{2}-\d+)$/.exec(jevNo)
  return m?.[1] ?? jevNo.replace(/^JEV-/, '')
}

// ── dates ───────────────────────────────────────────────────────────────────

const MONTHS = ['January', 'February', 'March', 'April', 'May', 'June', 'July',
  'August', 'September', 'October', 'November', 'December']

// an ISO date as its numbers; GIAMS always sends YYYY-MM-DD
function parts(iso: string): { y: number; m: number; d: number } {
  const [y = 0, m = 1, d = 1] = iso.slice(0, 10).split('-').map(Number)
  return { y, m, d }
}

function monthLabel(from: string, to: string): string {
  const a = parts(from)
  const b = parts(to)
  if (a.y === b.y && a.m === b.m) return `${MONTHS[a.m - 1]} ${a.y}`
  return `${longDate(from)} to ${longDate(to)}`
}

function longDate(iso: string): string {
  const { y, m, d } = parts(iso)
  return `${MONTHS[m - 1]} ${d}, ${y}`
}

function shortDate(iso: string): string {
  const { y, m, d } = parts(iso)
  return `${String(m).padStart(2, '0')}/${String(d).padStart(2, '0')}/${String(y).slice(2)}`
}
