import { useMemo, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { FileSpreadsheet, Plus, Printer, Trash2, X } from 'lucide-react'
import { downloadJevExcel } from '@/lib/excelForms'
import { ErrorNotice } from '@/components/ErrorNotice'
import { compareAmounts, formatAmount, isZero, sumAmounts, type Amount } from '@/lib/money'
import { AccountSelect } from '@/components/AccountSelect'
import { useFunds } from '@/queries/accounts'
import { useCreateJournalEntry, usePostJournalEntry } from '@/queries/journalEntries'
import { useAuthStore } from '@/stores/authStore'
import { usePeriodRange } from '@/stores/periodStore'
import { printForm, usePageSetup } from '@/lib/print'
import { JevPrintForm, type JevPrintLine } from '@/components/print/JevPrintForm'

/**
 * Journal Entry Voucher — Annex 23.
 *
 * The one form in the system that is written rather than read, so it follows
 * the paper closely: the four transaction-type boxes across the top, the
 * responsibility centre, and the ruled accounting-entries table underneath.
 *
 * The running debit and credit totals are the point of the layout. An entry
 * that does not balance is refused by the database at COMMIT, and finding that
 * out on submit — after typing twenty lines — is miserable. The totals are
 * computed in exact integer centavos (see `lib/money.ts`), never in floating
 * point, and are a guide only: the database remains the authority.
 */

interface DraftLine {
  key: number
  accountId: string
  accountLabel: string
  /** Kept from the moment the account is chosen, so the code column and the
      printed voucher never need to look it up again. */
  accountCode: string
  debit: string
  credit: string
  particulars: string
}

/** The four boxes across the top of the form, and the book each writes to. */
const TRANSACTION_TYPES = [
  { key: 'CRJ',  label: 'Collection' },
  { key: 'CKDJ', label: 'Check Disbursement' },
  { key: 'CDJ',  label: 'Cash Disbursement' },
  { key: 'GJ',   label: 'Others' },
] as const

let nextKey = 1
function blankLine(): DraftLine {
  return { key: nextKey++, accountId: '', accountLabel: '', accountCode: '', debit: '', credit: '', particulars: '' }
}

export function NewJournalEntry() {
  const navigate = useNavigate()
  const can = useAuthStore((s) => s.can)
  const period = usePeriodRange()

  const { data: funds } = useFunds()
  const [fundId, setFundId] = useState('')
  const [journalType, setJournalType] = useState<string>('GJ')
  const [entryDate, setEntryDate] = useState(period.to)
  const [particulars, setParticulars] = useState('')
  const [lines, setLines] = useState<DraftLine[]>([blankLine(), blankLine()])

  const create = useCreateJournalEntry()
  const post = usePostJournalEntry()

  // Exact, in integer centavos. Never a float.
  const totalDebit = useMemo(() => sumAmounts(lines.map((l) => l.debit)), [lines])
  const totalCredit = useMemo(() => sumAmounts(lines.map((l) => l.credit)), [lines])
  const balanced = compareAmounts(totalDebit, totalCredit) === 0
  const hasAmounts = !isZero(totalDebit) || !isZero(totalCredit)

  const defaultFund = fundId || funds?.[0]?.id || ''

  function update(key: number, patch: Partial<DraftLine>) {
    setLines((current) => current.map((l) => (l.key === key ? { ...l, ...patch } : l)))
  }

  async function save(thenPost: boolean) {
    const usable = lines.filter((l) => l.accountId && (l.debit || l.credit))

    const saved = await create.mutateAsync({
      fund_id: defaultFund,
      entry_date: entryDate,
      journal_type: journalType,
      particulars,
      lines: usable.map((l) => ({
        account_id: l.accountId,
        debit: (l.debit || '0.00') as Amount,
        credit: (l.credit || '0.00') as Amount,
        ...(l.particulars ? { particulars: l.particulars } : {}),
      })),
    })

    if (thenPost) await post.mutateAsync(saved.id)
    navigate('/books')
  }

  const busy = create.isPending || post.isPending

  usePageSetup('portrait')

  // What the printed voucher shows: only the lines actually filled in, with
  // the account code resolved. The editor's search boxes and blank starter
  // rows do not belong on the paper form.
  const printLines: JevPrintLine[] = lines
    .filter((l) => l.accountId && (l.debit || l.credit))
    .map((l) => ({
      code: l.accountCode,
      account: l.accountLabel,
      particulars: l.particulars,
      debit: (l.debit || '0.00') as Amount,
      credit: (l.credit || '0.00') as Amount,
    }))

  return (
    <>
    <div className="no-print min-h-0 flex-1 overflow-auto">
      <div className="mx-auto max-w-[900px] px-6 py-5">
        <div className="mb-4 flex items-start justify-between">
          <p className="text-xs text-ink-muted">Annex 23</p>

          <div className="no-print flex items-center gap-1">
            <button
              type="button"
              onClick={() =>
                void downloadJevExcel({
                  jevNo: null,
                  date: entryDate,
                  lines: printLines,
                  totalDebit,
                  totalCredit,
                })
              }
              title="Download this voucher as the office's Excel form, ready to print"
              className="flex items-center gap-2 rounded-pill bg-accent-fill px-3 py-1 text-sm font-medium text-accent-ink hover:brightness-[0.97]"
            >
              <FileSpreadsheet className="size-4" aria-hidden />
              Excel form
            </button>
            <button
              type="button"
              onClick={printForm}
              className="flex items-center gap-2 rounded-pill px-3 py-1 text-sm hover:bg-hover"
            >
              <Printer className="size-4" aria-hidden />
              Print
            </button>
            <button
              type="button"
              onClick={() => navigate('/books')}
              aria-label="Close"
              className="rounded-pill p-1.5 text-ink-secondary hover:bg-hover"
            >
              <X className="size-[18px]" aria-hidden />
            </button>
          </div>
        </div>

        <div className="text-center">
          <p className="font-medium tracking-wide">JOURNAL ENTRY VOUCHER</p>
        </div>

        {/* Heading block: LGU on the left, number and date on the right, as
            the paper form is laid out. The number is assigned on save, so it
            reads "to be assigned" until then rather than showing a guess. */}
        <div className="mt-4 flex items-end justify-between gap-6 border-b border-line pb-3">
          <div className="min-w-0 flex-1">
            <Label>LGU</Label>
            <p>Municipality of Gasan</p>
          </div>
          <div className="w-[170px]">
            <Label>No.</Label>
            <p className="text-ink-muted">assigned on saving</p>
          </div>
          <div className="w-[150px]">
            <Label>Date</Label>
            <input
              type="date"
              value={entryDate}
              onChange={(e) => setEntryDate(e.target.value)}
              className="w-full rounded-sm bg-sunken px-2 py-1 tabular"
            />
          </div>
        </div>

        <div className="mt-4 flex flex-wrap items-center gap-x-6 gap-y-2 border-b border-line pb-3">
          {TRANSACTION_TYPES.map((t) => (
            <label key={t.key} className="flex cursor-pointer items-center gap-2 text-sm">
              <input
                type="radio"
                name="journal_type"
                checked={journalType === t.key}
                onChange={() => setJournalType(t.key)}
              />
              {t.label}
            </label>
          ))}

          <span className="ml-auto flex items-center gap-2 text-sm">
            <Label>Fund</Label>
            <select
              value={defaultFund}
              onChange={(e) => setFundId(e.target.value)}
              className="rounded-sm bg-sunken px-2 py-1"
            >
              {funds?.map((f) => (
                <option key={f.id} value={f.id}>{f.name}</option>
              ))}
            </select>
          </span>
        </div>

        <div className="mt-4">
          <Label>Explanation</Label>
          <input
            value={particulars}
            onChange={(e) => setParticulars(e.target.value)}
            placeholder="What this entry is for"
            className="w-full rounded-sm bg-sunken px-3 py-2"
          />
        </div>

        <p className="mt-5 text-center text-sm font-medium">ACCOUNTING ENTRIES</p>

        <table className="mt-2 w-full text-sm">
          <thead>
            <tr className="border-b border-line-strong text-xs text-ink-muted">
              <Th className="text-left">Accounts and Explanation</Th>
              <Th className="w-[124px] text-left">Account Code</Th>
              <Th className="w-[70px] text-left">Ref.</Th>
              <Th className="w-[132px] text-right">Debit</Th>
              <Th className="w-[132px] text-right">Credit</Th>
              <Th className="w-[36px]" />
            </tr>
          </thead>
          <tbody>
            {lines.map((line) => (
              <EntryLineRow
                key={line.key}
                line={line}
                onChange={(patch) => update(line.key, patch)}
                onRemove={() => setLines((c) => (c.length > 2 ? c.filter((l) => l.key !== line.key) : c))}
              />
            ))}
          </tbody>
          <tfoot>
            <tr className="border-t-2 border-line-strong font-medium">
              <td colSpan={3} className="px-3 py-2 text-right">Total</td>
              <td className="px-3 py-2 text-right tabular">{formatAmount(totalDebit)}</td>
              <td className="px-3 py-2 text-right tabular">{formatAmount(totalCredit)}</td>
              <td />
            </tr>
          </tfoot>
        </table>

        <button
          type="button"
          onClick={() => setLines((c) => [...c, blankLine()])}
          className="mt-2 flex items-center gap-1.5 rounded-pill px-3 py-1 text-sm text-ink-secondary hover:bg-hover"
        >
          <Plus className="size-4" aria-hidden />
          Add a line
        </button>

        {/* Says whether it balances while there is still something to fix. */}
        {hasAmounts && (
          <p
            className={`mt-4 rounded-md px-4 py-2 text-sm ${
              balanced ? 'bg-ok-fill text-ok-ink' : 'bg-warn-fill text-warn-ink'
            }`}
          >
            {balanced
              ? 'Debits and credits agree.'
              : `Out by ${formatAmount(difference(totalDebit, totalCredit))}. This cannot be posted until they agree.`}
          </p>
        )}

        {create.error && <div className="mt-4"><ErrorNotice error={create.error} /></div>}
        {post.error && <div className="mt-4"><ErrorNotice error={post.error} /></div>}

        <div className="mt-5 flex items-center gap-2 border-t border-line pt-4">
          <button
            type="button"
            onClick={() => void save(false)}
            disabled={busy}
            className="rounded-pill bg-sunken px-4 py-2 text-sm font-medium hover:bg-hover disabled:opacity-60"
          >
            Save as draft
          </button>

          {can('jev.post') && (
            <button
              type="button"
              onClick={() => void save(true)}
              disabled={busy || !balanced || !hasAmounts}
              title={!balanced ? 'The debits and credits must agree first' : undefined}
              className="rounded-pill bg-accent-fill px-4 py-2 text-sm font-medium text-accent-ink hover:brightness-[0.97] disabled:opacity-50"
            >
              Save and post
            </button>
          )}

          <span className="ml-auto text-xs text-ink-muted">
            A draft can be finished later. Posting writes it to the general ledger.
          </span>
        </div>
      </div>
    </div>

    <JevPrintForm
      jevNo={null}
      date={entryDate}
      journalType={journalType}
      lines={printLines}
      totalDebit={totalDebit}
      totalCredit={totalCredit}
    />
    </>
  )
}

/** The gap between two totals, exactly. */
function difference(debit: Amount, credit: Amount): Amount {
  return sumAmounts([debit, credit.startsWith('-') ? credit.slice(1) : `-${credit}`])
}

function EntryLineRow({
  line, onChange, onRemove,
}: {
  line: DraftLine
  onChange: (patch: Partial<DraftLine>) => void
  onRemove: () => void
}) {
  return (
    <tr className="border-b border-line align-top">
      <td className="px-3 py-1.5">
        <AccountSelect
          value={line.accountId}
          onChange={(a) => onChange({ accountId: a.id, accountLabel: a.name, accountCode: a.code })}
        />

        <input
          value={line.particulars}
          onChange={(e) => onChange({ particulars: e.target.value })}
          placeholder="Explanation for this line"
          className="mt-1 w-full rounded-sm bg-transparent px-2 py-0.5 text-xs text-ink-secondary placeholder:text-ink-muted"
        />
      </td>

      <td className="px-3 py-1.5 tabular text-ink-secondary">{line.accountCode}</td>

      {/* Annex 23 rules a reference column here. It is filled in by hand
          against the source document, so it is printed empty rather than
          invented — an entry's source is recorded on the voucher itself. */}
      <td className="px-3 py-1.5" />

      <AmountCell
        value={line.debit}
        // A line is one or the other. Typing in one clears the other rather
        // than letting the clerk create a line the database will refuse.
        onChange={(v) => onChange({ debit: v, credit: v ? '' : line.credit })}
      />
      <AmountCell
        value={line.credit}
        onChange={(v) => onChange({ credit: v, debit: v ? '' : line.debit })}
      />

      <td className="px-1 py-1.5">
        <button
          type="button"
          onClick={onRemove}
          aria-label="Remove this line"
          className="rounded-pill p-1 text-ink-muted hover:bg-hover hover:text-danger-ink"
        >
          <Trash2 className="size-4" aria-hidden />
        </button>
      </td>
    </tr>
  )
}

function AmountCell({ value, onChange }: { value: string; onChange: (v: string) => void }) {
  return (
    <td className="px-3 py-1.5">
      <input
        value={value}
        onChange={(e) => {
          // Digits and one decimal point. Keeping the field clean is what lets
          // the running total parse it exactly on every keystroke.
          const cleaned = e.target.value.replace(/[^\d.]/g, '')
          if ((cleaned.match(/\./g) ?? []).length <= 1) onChange(cleaned)
        }}
        inputMode="decimal"
        placeholder="0.00"
        className="w-full rounded-sm bg-sunken px-2 py-1 text-right tabular"
      />
    </td>
  )
}

function Label({ children }: { children: React.ReactNode }) {
  return <p className="text-xs text-ink-muted">{children}</p>
}

function Th({ children, className = '' }: { children?: React.ReactNode; className?: string }) {
  return <th className={`px-3 py-2 font-normal ${className}`}>{children}</th>
}
