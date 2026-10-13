import { useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { BookOpen, Coins, FilePlus2, FileSpreadsheet, Printer, Receipt, Wallet } from 'lucide-react'
import { ListPage, type ListTab } from '@/components/ListPage'
import { ErrorNotice } from '@/components/ErrorNotice'
import { ExportButton } from '@/components/ExportButton'
import { FormHeading } from '@/components/FormHeading'
import { Td, Th } from '@/components/FormTable'
import { formatAmount } from '@/lib/money'
import {
  useBook, useTrialBalance,
  type BookLine, type TrialBalanceLine,
} from '@/queries/books'
import { useAuthStore } from '@/stores/authStore'
import { usePeriodRange } from '@/stores/periodStore'
import { printForm, usePageSetup } from '@/lib/print'
import { CertifiedCorrect } from '@/components/Signatures'

/**
 * The books of accounts, laid out as the office's own forms.
 *
 * These are read-and-print rather than data-entry, so unlike the registers
 * they keep the form's heading block — Annex number, LGU, fund, period. That
 * heading is not decoration: it is what makes a printed page identifiable
 * once it leaves the screen and goes into a folder, which is where these end
 * up.
 *
 * The column rules are the paper form's, down to the debit and credit columns
 * being the rightmost pair, because an accountant checks a book by reading
 * down those two.
 */

const TABS: ListTab[] = [
  { key: 'GJ',   label: 'General Journal',      icon: BookOpen },
  { key: 'CRJ',  label: 'Cash Receipts',        icon: Coins },
  { key: 'CKDJ', label: 'Check Disbursements',  icon: Receipt },
  { key: 'CDJ',  label: 'Cash Disbursements',   icon: Wallet },
  { key: 'TB',   label: 'Trial Balance',        icon: FileSpreadsheet },
]

export function Books() {
  const [tab, setTab] = useState('GJ')
  const [search, setSearch] = useState('')
  const period = usePeriodRange()
  const navigate = useNavigate()
  const can = useAuthStore((s) => s.can)

  const book = useBook(tab, period.from, period.to)
  const trialBalance = useTrialBalance(period.to, undefined, tab === 'TB')

  // The journals are ruled wide — six columns including particulars — and
  // lose their debit and credit columns off the edge of a portrait page.
  usePageSetup(tab === 'TB' ? 'portrait' : 'landscape')

  const active = tab === 'TB' ? trialBalance : book
  const rowCount = tab === 'TB' ? trialBalance.data?.lines.length : book.data?.lines.length

  return (
    <ListPage
      tabs={TABS}
      activeTab={tab}
      onTabChange={(key) => { setTab(key); setSearch('') }}
      search={{
        value: search,
        onChange: setSearch,
        placeholder: tab === 'TB' ? 'Search account number or title' : 'Search JEV number, particulars or account',
      }}
      total={rowCount}
      pageSize={rowCount ?? 0}
      onRefresh={() => void active.refetch()}
      refreshing={active.isFetching}
      toolbar={
        <>
        {can('jev.draft') && (
          <button
            type="button"
            onClick={() => navigate('/books/new')}
            className="ml-2 flex shrink-0 items-center gap-2 whitespace-nowrap rounded-pill bg-accent-fill px-3 py-1 text-sm font-medium text-accent-ink hover:brightness-[0.97]"
          >
            <FilePlus2 className="size-4" aria-hidden />
            New entry
          </button>
        )}
        <ExportButton
          path={
            tab === 'TB'
              ? `/api/trial-balance/export.xlsx?as_of=${period.to}`
              : `/api/books/${tab}/export.xlsx?from=${period.from}&to=${period.to}`
          }
          filename={tab === 'TB' ? 'trial-balance.xlsx' : `${tab}.xlsx`}
        />
        <button
          type="button"
          onClick={printForm}
          className="ml-2 flex shrink-0 items-center gap-2 whitespace-nowrap rounded-pill px-3 py-1 text-sm hover:bg-hover"
        >
          <Printer className="size-4" aria-hidden />
          Print
        </button>
        </>
      }
    >
      {active.error && <div className="p-4"><ErrorNotice error={active.error} /></div>}
      {active.isPending && <p className="px-4 py-6 text-ink-muted">Loading…</p>}

      {tab === 'TB'
        ? trialBalance.data && <TrialBalanceForm page={trialBalance.data} term={search} />
        : book.data && <JournalForm page={book.data} term={search} />}
    </ListPage>
  )
}

function JournalForm({ page, term }: { page: import('@/queries/books').BookPage; term: string }) {
  // Annex 1 is dated; Annexes 2, 3 and 4 label the signature rule.
  const isGeneralJournal = page.annex === 'Annex 1'
  const matches = (line: BookLine) => matchesSearch(term, [
    line.jev_no, line.particulars, line.reference, line.counterparty, line.account_code, line.account_name,
  ])

  return (
    <div className="relative">
      <FormHeading
        annex={page.annex}
        title={page.title}
        municipality={page.municipality}
        lines={[`Fund: ${page.fund_name}`, page.period_label]}
      />

      <SearchNote term={term} shown={page.lines.filter(matches).length} total={page.lines.length} />

      {page.lines.length === 0 ? (
        <p className="px-6 py-12 text-center text-ink-secondary">
          Nothing was posted to this book in the period selected.
        </p>
      ) : (
        <table className="w-full text-sm">
          <thead>
            <tr className="border-b border-line-strong text-xs text-ink-muted">
              <Th className="w-[92px] text-left">Date</Th>
              <Th className="w-[150px] text-left">JEV No.</Th>
              <Th className="text-left">Particulars</Th>
              <Th className="w-[112px] text-left">Account Code</Th>
              <Th className="w-[124px] text-right">Debit</Th>
              <Th className="w-[124px] text-right">Credit</Th>
            </tr>
          </thead>
          <tbody>
            {page.lines.map((line, i) => (
              <JournalRow key={`${line.jev_no}-${i}`} line={line} hidden={!matches(line)} />
            ))}
          </tbody>
          <tfoot>
            <tr className="border-t-2 border-line-strong font-medium">
              <Td colSpan={4} className="text-right">Total</Td>
              <Td className="text-right tabular">{formatAmount(page.total_debit)}</Td>
              <Td className="text-right tabular">{formatAmount(page.total_credit)}</Td>
            </tr>
          </tfoot>
        </table>
      )}

      <CertifiedCorrect dated={isGeneralJournal} signatureLabel={!isGeneralJournal} />
    </div>
  )
}

function JournalRow({ line, hidden }: { line: BookLine; hidden: boolean }) {
  return (
    <tr className={`border-b border-line hover:bg-hover ${hidden ? 'hidden print:table-row' : ''}`}>
      <Td className="tabular text-ink-secondary">{line.entry_date}</Td>
      <Td className="tabular">{line.jev_no}</Td>
      <Td className="max-w-0 truncate">
        {line.particulars}
        {/* The reference a clerk looks the line up by: cheque number on the
            CkDJ, report number on the CRJ. */}
        {line.reference && <span className="text-ink-muted"> · {line.reference}</span>}
        {line.counterparty && <span className="text-ink-muted"> · {line.counterparty}</span>}
      </Td>
      <Td className="tabular text-ink-secondary">{line.account_code}</Td>
      <Td className="text-right tabular">
        {line.debit === '0.00' ? '' : formatAmount(line.debit)}
      </Td>
      <Td className="text-right tabular">
        {line.credit === '0.00' ? '' : formatAmount(line.credit)}
      </Td>
    </tr>
  )
}

function TrialBalanceForm({ page, term }: { page: import('@/queries/books').TrialBalancePage; term: string }) {
  const balanced = page.difference === '0.00'
  const matches = (line: TrialBalanceLine) => matchesSearch(term, [line.account_code, line.account_name])

  return (
    <div>
      <FormHeading
        title="TRIAL BALANCE"
        municipality={page.municipality}
        lines={[page.fund_name, `As of ${page.as_of}`]}
      />

      {/* Whether the ledger balances is the first thing to know, so it sits
          above the figures rather than at the foot of a long page. */}
      {!balanced && (
        <p className="border-b border-line bg-danger-fill px-6 py-2 text-center text-danger-ink">
          The debits and credits differ by {formatAmount(page.difference)}. This
          trial balance does not balance and must not be filed.
        </p>
      )}

      <SearchNote term={term} shown={page.lines.filter(matches).length} total={page.lines.length} />

      {page.lines.length === 0 ? (
        <p className="px-6 py-12 text-center text-ink-secondary">
          Nothing has been posted as at this date.
        </p>
      ) : (
        <table className="w-full text-sm">
          <thead>
            <tr className="border-b border-line-strong text-xs text-ink-muted">
              <Th className="text-left">Account Title</Th>
              <Th className="w-[124px] text-left">Account No.</Th>
              <Th className="w-[140px] text-right">Debit Balance</Th>
              <Th className="w-[140px] text-right">Credit Balance</Th>
            </tr>
          </thead>
          <tbody>
            {page.lines.map((line) => (
              <TrialBalanceRow key={line.account_code} line={line} hidden={!matches(line)} />
            ))}
          </tbody>
          <tfoot>
            <tr className="border-t-2 border-line-strong font-medium">
              <Td colSpan={2} className="text-right">Total</Td>
              <Td className="text-right tabular">{formatAmount(page.total_debit)}</Td>
              <Td className="text-right tabular">{formatAmount(page.total_credit)}</Td>
            </tr>
          </tfoot>
        </table>
      )}

      <CertifiedCorrect />
    </div>
  )
}

function TrialBalanceRow({ line, hidden }: { line: TrialBalanceLine; hidden: boolean }) {
  return (
    <tr className={`border-b border-line hover:bg-hover ${hidden ? 'hidden print:table-row' : ''}`}>
      <Td className="max-w-0 truncate">{line.account_name}</Td>
      <Td className="tabular text-ink-secondary">{line.account_code}</Td>
      <Td className="text-right tabular">
        {line.debit_balance === '0.00' ? '' : formatAmount(line.debit_balance)}
      </Td>
      <Td className="text-right tabular">
        {line.credit_balance === '0.00' ? '' : formatAmount(line.credit_balance)}
      </Td>
    </tr>
  )
}

// ── Search ───────────────────────────────────────────────────────────────────
// A book is a form that gets filed, so search only narrows the screen: rows
// that do not match are hidden here and still print, and the totals stay the
// whole book's.

function matchesSearch(term: string, fields: (string | null)[]): boolean {
  const t = term.trim().toLowerCase()
  return !t || fields.some((f) => f?.toLowerCase().includes(t))
}

function SearchNote({ term, shown, total }: { term: string; shown: number; total: number }) {
  if (!term.trim() || total === 0) return null

  return (
    <p className="no-print border-b border-line bg-info-fill px-6 py-1.5 text-sm text-info-ink">
      {shown === 0
        ? `No line matches "${term.trim()}".`
        : `Showing ${shown} of ${total} lines matching "${term.trim()}".`}
      {' '}Printing still prints the whole book, and the totals are the whole book's.
    </p>
  )
}
