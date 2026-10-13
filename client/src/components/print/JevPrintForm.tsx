import { formatAmount, type Amount } from '@/lib/money'

/**
 * The Journal Entry Voucher (Annex 23), printed to match the office's own
 * workbook cell for cell — checked against the exported PDF of the JEV sheet,
 * not a guess at it.
 *
 * The screen form is an editor and can never be the paper form, so the two are
 * split: the editor is `no-print`, this is `print-only`. The details that make
 * it the office's form rather than a lookalike:
 *
 *   * a serif face and a heavy outer rule, as the printed template has;
 *   * the title over a ruled "LGU" line, with No. and Date boxed off to the
 *     right by a vertical rule;
 *   * the transaction-type strip with squared checkboxes;
 *   * "Responsibility Center" down the left, then Accounts and Explanation,
 *     Account Code, Ref., and the letter-spaced "A m o u n t" split into
 *     Debit and Credit;
 *   * a fixed sixteen ruled rows, then "T O T A L" with its arrow;
 *   * "Prepared by: / Accounting Staff" and "Approved by: / Municipal
 *     Accountant" boxed beneath.
 *
 * The columns the office fills by hand — Responsibility Center and Ref. — are
 * ruled but blank, exactly as on the paper form.
 */

export interface JevPrintLine {
  code: string
  account: string
  particulars: string
  debit: Amount
  credit: Amount
}

const TYPE_LABELS: Record<string, string> = {
  CRJ: 'Collection',
  CKDJ: 'Check Disbursement',
  CDJ: 'Cash Disbursement',
  GJ: 'Others',
}

const TYPE_ORDER = ['CRJ', 'CKDJ', 'CDJ', 'GJ'] as const

/** The blank form carries sixteen ruled rows; a short entry still fills it. */
const FORM_ROWS = 16

export function JevPrintForm({
  jevNo,
  date,
  journalType,
  lines,
  totalDebit,
  totalCredit,
}: {
  jevNo: string | null
  date: string
  journalType: string
  lines: JevPrintLine[]
  totalDebit: Amount
  totalCredit: Amount
}) {
  const rowCount = Math.max(FORM_ROWS, lines.length)
  const blanks = rowCount - lines.length

  return (
    <div
      className="print-only jev-print mx-auto text-black"
      style={{ fontFamily: "'Times New Roman', Georgia, serif", maxWidth: '190mm' }}
    >
      <p className="mb-1 text-right text-[9pt] font-bold italic">Annex 23</p>

      <div className="border-2 border-black">
        {/* ── Title, LGU line, and the boxed No./Date ─────────────────────── */}
        <div className="flex border-b-2 border-black">
          <div className="flex flex-1 flex-col items-center justify-center px-4 py-3">
            <p className="text-[13pt] font-bold tracking-wide">JOURNAL ENTRY VOUCHER</p>
            <p className="mt-3 min-w-[240px] border-b border-black text-center text-[10pt]">
              Municipality of Gasan
            </p>
            <p className="text-center text-[8.5pt] italic">LGU</p>
          </div>
          <div className="w-[150px] shrink-0 border-l-2 border-black text-[10pt]">
            <div className="flex h-1/2 items-center gap-1 border-b border-black px-2">
              <span>No.</span>
              <span className="tabular">{jevNo ?? ''}</span>
            </div>
            <div className="flex h-1/2 items-center gap-1 px-2">
              <span>Date:</span>
              <span className="tabular">{date}</span>
            </div>
          </div>
        </div>

        {/* ── Transaction-type strip ──────────────────────────────────────── */}
        <div className="flex items-center gap-x-8 border-b-2 border-black px-4 py-1.5 text-[10pt]">
          {TYPE_ORDER.map((key) => (
            <span key={key} className="flex items-center gap-1.5">
              <span
                className="relative inline-block size-[12px] border border-black"
                aria-hidden
              >
                {journalType === key && (
                  <span className="absolute inset-0 flex items-center justify-center text-[10pt] font-bold leading-none">
                    ✓
                  </span>
                )}
              </span>
              {TYPE_LABELS[key]}
            </span>
          ))}
        </div>

        {/* ── Accounting entries ──────────────────────────────────────────── */}
        <table className="w-full border-collapse text-[10pt]">
          <colgroup>
            <col style={{ width: '12%' }} />
            <col style={{ width: '34%' }} />
            <col style={{ width: '14%' }} />
            <col style={{ width: '8%' }} />
            <col style={{ width: '16%' }} />
            <col style={{ width: '16%' }} />
          </colgroup>
          <thead>
            <tr>
              <th
                rowSpan={3}
                className="border border-black px-1 py-1 text-center align-middle font-bold leading-tight"
              >
                Responsibility
                <br />
                Center
              </th>
              <th colSpan={5} className="border border-black px-1 py-0.5 text-center font-bold">
                ACCOUNTING ENTRIES
              </th>
            </tr>
            <tr>
              <th rowSpan={2} className="border border-black px-1 py-0.5 text-center font-bold">
                Accounts and Explanation
              </th>
              <th rowSpan={2} className="border border-black px-1 py-0.5 text-center font-bold">
                Account Code
              </th>
              <th rowSpan={2} className="border border-black px-1 py-0.5 text-center font-bold">
                Ref.
              </th>
              <th colSpan={2} className="border border-black px-1 py-0.5 text-center font-bold tracking-[0.35em]">
                Amount
              </th>
            </tr>
            <tr>
              <th className="border border-black px-1 py-0.5 text-center font-bold">Debit</th>
              <th className="border border-black px-1 py-0.5 text-center font-bold">Credit</th>
            </tr>
          </thead>
          <tbody>
            {lines.map((line, i) => (
              <tr key={i} className="align-top">
                <td className="h-[20px] border border-black px-1">&nbsp;</td>
                <td className="border border-black px-1">
                  {line.account}
                  {line.particulars && (
                    <span className="block pl-4 text-[9pt] italic">{line.particulars}</span>
                  )}
                </td>
                <td className="border border-black px-1 text-center tabular">{line.code}</td>
                <td className="border border-black px-1">&nbsp;</td>
                <td className="border border-black px-1 text-right tabular">
                  {isPositive(line.debit) ? formatAmount(line.debit) : ''}
                </td>
                <td className="border border-black px-1 text-right tabular">
                  {isPositive(line.credit) ? formatAmount(line.credit) : ''}
                </td>
              </tr>
            ))}
            {Array.from({ length: blanks }).map((_, i) => (
              <tr key={`blank-${i}`}>
                <td className="h-[20px] border border-black px-1">&nbsp;</td>
                <td className="border border-black px-1">&nbsp;</td>
                <td className="border border-black px-1">&nbsp;</td>
                <td className="border border-black px-1">&nbsp;</td>
                <td className="border border-black px-1">&nbsp;</td>
                <td className="border border-black px-1">&nbsp;</td>
              </tr>
            ))}
            <tr>
              <td
                colSpan={4}
                className="border border-black px-2 py-0.5 text-center text-[11pt] font-bold tracking-[0.3em]"
              >
                T O T A L&nbsp;&nbsp;➡
              </td>
              <td className="border border-black px-1 text-right font-bold tabular">
                {formatAmount(totalDebit)}
              </td>
              <td className="border border-black px-1 text-right font-bold tabular">
                {formatAmount(totalCredit)}
              </td>
            </tr>
          </tbody>
        </table>

        {/* ── Certification ───────────────────────────────────────────────── */}
        <div className="flex border-t-2 border-black text-[10pt]">
          <div className="flex-1 px-4 pb-3 pt-2">
            <p>Prepared by:</p>
            <p className="mx-auto mt-8 w-4/5 border-t border-black pt-0.5 text-center">
              Accounting Staff
            </p>
          </div>
          <div className="flex-1 border-l border-black px-4 pb-3 pt-2">
            <p>Approved by:</p>
            <p className="mx-auto mt-8 w-4/5 border-t border-black pt-0.5 text-center">
              Municipal Accountant
            </p>
          </div>
        </div>
      </div>
    </div>
  )
}

/** An amount string is positive when it is neither empty, zero, nor negative. */
function isPositive(amount: Amount): boolean {
  return !!amount && amount !== '0.00' && !amount.startsWith('-')
}
