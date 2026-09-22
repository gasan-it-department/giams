import { useState } from 'react'
import { Landmark, Printer, Scale, TrendingUp } from 'lucide-react'
import { ListPage, type ListTab } from '@/components/ListPage'
import { ErrorNotice } from '@/components/ErrorNotice'
import { ExportButton } from '@/components/ExportButton'
import { FormHeading } from '@/components/FormHeading'
import { Td, Th } from '@/components/FormTable'
import { formatAmount, isZero } from '@/lib/money'
import {
  useBudgetActual, useFinancialPerformance, useFinancialPosition,
  type BudgetActual, type BudgetActualLine,
  type FinancialPerformance, type FinancialPosition,
  type PerformanceLine, type StatementLine,
} from '@/queries/statements'
import { usePeriodRange } from '@/stores/periodStore'
import { printForm, usePageSetup } from '@/lib/print'
import { Signatures } from '@/components/Signatures'

/**
 * The general purpose financial statements.
 *
 * Three of the six GPFS are produced here. The Statement of Cash Flows and the
 * Statement of Changes in Net Assets/Equity need prior-period opening balances
 * to mean anything, and those arrive with the legacy data migration — so they
 * are named as pending rather than shown empty, which would read as "we have
 * no cash flows".
 *
 * Every figure comes from the database, computed from posted entries only.
 * Nothing on this screen is added up in the browser.
 */

const TABS: ListTab[] = [
  { key: 'SFP',   label: 'Financial Position',    icon: Landmark },
  { key: 'SFPer', label: 'Financial Performance', icon: TrendingUp },
  { key: 'SCBAA', label: 'Budget vs Actual',      icon: Scale },
]

export function Statements() {
  const [tab, setTab] = useState('SFP')
  const period = usePeriodRange()
  const fiscalYear = Number(period.from.slice(0, 4))

  const position    = useFinancialPosition(period.to, undefined, tab === 'SFP')
  const performance = useFinancialPerformance(period.from, period.to, undefined, tab === 'SFPer')
  const budget      = useBudgetActual(fiscalYear, undefined, tab === 'SCBAA')

  const active =
    tab === 'SFP'   ? position :
    tab === 'SFPer' ? performance :
                      budget

  // Ten columns on the budget comparison; nothing else needs the width.
  usePageSetup(tab === 'SCBAA' ? 'landscape' : 'portrait')

  const rowCount = active.data?.lines.length

  const [exportPath, exportName] =
    tab === 'SFP'
      ? [`/api/statements/financial-position/export.xlsx?as_of=${period.to}`,
         'financial-position.xlsx']
      : tab === 'SFPer'
        ? [`/api/statements/financial-performance/export.xlsx?from=${period.from}&to=${period.to}`,
           'financial-performance.xlsx']
        : [`/api/statements/budget-vs-actual/export.xlsx?fiscal_year=${fiscalYear}`,
           'budget-and-actual.xlsx']

  return (
    <ListPage
      tabs={TABS}
      activeTab={tab}
      onTabChange={setTab}
      total={rowCount}
      pageSize={rowCount ?? 0}
      onRefresh={() => void active.refetch()}
      refreshing={active.isFetching}
      toolbar={
        <>
          <ExportButton path={exportPath} filename={exportName} />
          <button
            type="button"
            onClick={printForm}
            className="ml-2 flex items-center gap-2 rounded-pill px-3 py-1 text-sm hover:bg-hover"
          >
            <Printer className="size-4" aria-hidden />
            Print
          </button>
        </>
      }
    >
      {active.error && <div className="p-4"><ErrorNotice error={active.error} /></div>}
      {active.isPending && <p className="px-4 py-6 text-ink-muted">Loading…</p>}

      {tab === 'SFP'   && position.data    && <PositionForm    page={position.data} />}
      {tab === 'SFPer' && performance.data && <PerformanceForm page={performance.data} />}
      {tab === 'SCBAA' && budget.data      && <BudgetActualForm page={budget.data} />}
    </ListPage>
  )
}

// ── Statement of Financial Position ──────────────────────────────────────────

/** The order a balance sheet is read in, which is not the order rows arrive. */
const POSITION_SECTIONS = ['Assets', 'Liabilities', 'Net assets and equity']

function PositionForm({ page }: { page: FinancialPosition }) {
  const balances = isZero(page.check)

  return (
    <div>
      <FormHeading
        title="STATEMENT OF FINANCIAL POSITION"
        municipality={page.municipality}
        lines={[page.fund_name, `As at ${page.as_of}`]}
      />

      {/* Whether it balances is the first thing to know about a balance sheet,
          so it sits above the figures rather than at the foot of a long page. */}
      {!balances && (
        <p className="border-b border-line bg-danger-fill px-6 py-2 text-center text-danger-ink">
          Assets exceed liabilities and equity by {formatAmount(page.check)}. This
          statement does not balance and must not be issued.
        </p>
      )}

      {page.lines.length === 0 ? (
        <Nothing>Nothing has been posted as at this date.</Nothing>
      ) : (
        <table className="w-full text-sm">
          <thead>
            <tr className="border-b border-line-strong text-xs text-ink-muted">
              <Th className="text-left">Account Title</Th>
              <Th className="w-[124px] text-left">Account No.</Th>
              <Th className="w-[160px] text-right">Amount</Th>
            </tr>
          </thead>
          {POSITION_SECTIONS.map((section) => {
            const lines = page.lines.filter((l) => l.section === section)
            if (lines.length === 0) return null

            return (
              <tbody key={section}>
                <tr className="border-b border-line bg-sunken">
                  <Td colSpan={3} className="text-xs uppercase tracking-wide text-ink-secondary">
                    {section}
                  </Td>
                </tr>
                {lines.map((line) => (
                  <PositionRow key={`${section}-${line.account_code}-${line.account_name}`} line={line} />
                ))}
              </tbody>
            )
          })}
        </table>
      )}

      <Signatures
        signatories={[
          { act: 'Certified correct:', role: 'Municipal Accountant' },
          { act: 'Noted:', role: 'Municipal Mayor' },
        ]}
      />
    </div>
  )
}

function PositionRow({ line }: { line: StatementLine }) {
  return (
    <tr
      className={
        line.is_total
          ? 'border-b border-line border-t-2 border-t-line-strong font-medium'
          : 'border-b border-line hover:bg-hover'
      }
    >
      <Td className={line.is_total ? '' : 'max-w-0 truncate pl-6'}>{line.account_name}</Td>
      <Td className="tabular text-ink-secondary">
        {/* TOTAL and RESULT are markers, not account numbers. */}
        {line.account_code === 'TOTAL' || line.account_code === 'RESULT' ? '' : line.account_code}
      </Td>
      <Td className="text-right tabular">{formatAmount(line.amount)}</Td>
    </tr>
  )
}

// ── Statement of Financial Performance ───────────────────────────────────────

const PERFORMANCE_SECTIONS = ['Income', 'Expenses', 'Result']

function PerformanceForm({ page }: { page: FinancialPerformance }) {
  return (
    <div>
      <FormHeading
        title="STATEMENT OF FINANCIAL PERFORMANCE"
        municipality={page.municipality}
        lines={[page.fund_name, `For the period ${page.period_label}`]}
      />

      {page.lines.length === 0 ? (
        <Nothing>Nothing was posted to income or expense in the period selected.</Nothing>
      ) : (
        <table className="w-full text-sm">
          <thead>
            <tr className="border-b border-line-strong text-xs text-ink-muted">
              <Th className="text-left">Account Title</Th>
              <Th className="w-[124px] text-left">Account No.</Th>
              <Th className="w-[168px] text-left">Class</Th>
              <Th className="w-[160px] text-right">Amount</Th>
            </tr>
          </thead>
          {PERFORMANCE_SECTIONS.map((section) => {
            const lines = page.lines.filter((l) => l.section === section)
            if (lines.length === 0) return null

            return (
              <tbody key={section}>
                {section !== 'Result' && (
                  <tr className="border-b border-line bg-sunken">
                    <Td colSpan={4} className="text-xs uppercase tracking-wide text-ink-secondary">
                      {section}
                    </Td>
                  </tr>
                )}
                {lines.map((line) => (
                  <PerformanceRow key={`${section}-${line.account_code}-${line.account_name}`} line={line} />
                ))}
              </tbody>
            )
          })}
        </table>
      )}

      <Signatures
        signatories={[
          { act: 'Certified correct:', role: 'Municipal Accountant' },
          { act: 'Noted:', role: 'Municipal Mayor' },
        ]}
      />
    </div>
  )
}

function PerformanceRow({ line }: { line: PerformanceLine }) {
  return (
    <tr
      className={
        line.is_total
          ? 'border-b border-line border-t-2 border-t-line-strong font-medium'
          : 'border-b border-line hover:bg-hover'
      }
    >
      <Td className={line.is_total ? '' : 'max-w-0 truncate pl-6'}>{line.account_name}</Td>
      <Td className="tabular text-ink-secondary">
        {line.account_code === 'TOTAL' || line.account_code === 'RESULT' ? '' : line.account_code}
      </Td>
      <Td className="text-ink-secondary">{EXPENSE_CLASS[line.expense_class ?? ''] ?? ''}</Td>
      <Td className="text-right tabular">{formatAmount(line.amount)}</Td>
    </tr>
  )
}

/** The budget's own vocabulary, spelled out rather than abbreviated. */
const EXPENSE_CLASS: Record<string, string> = {
  PS:     'Personal Services',
  MOOE:   'Maintenance and Other Operating Expenses',
  CO:     'Capital Outlay',
  FE:     'Financial Expenses',
}

// ── Comparison of Budget and Actual Amounts ──────────────────────────────────

function BudgetActualForm({ page }: { page: BudgetActual }) {
  return (
    <div>
      <FormHeading
        title="STATEMENT OF COMPARISON OF BUDGET AND ACTUAL AMOUNTS"
        municipality={page.municipality}
        lines={[page.fund_name, `For the year ended 31 December ${page.fiscal_year}`]}
      />

      {page.lines.length === 0 ? (
        <Nothing>
          No appropriations have been recorded for {page.fiscal_year}. This statement
          compares the appropriation ordinance against what was obligated and paid,
          so it stays empty until the year&rsquo;s budget is entered.
        </Nothing>
      ) : (
        <div className="overflow-x-auto">
          <table className="w-full min-w-[1080px] text-sm">
            <thead>
              <tr className="border-b border-line-strong text-xs text-ink-muted">
                <Th className="w-[80px] text-left">Class</Th>
                <Th className="w-[180px] text-left">Office</Th>
                <Th className="text-left">Account Title</Th>
                <Th className="w-[112px] text-left">Account No.</Th>
                <Th className="w-[132px] text-right">Original Budget</Th>
                <Th className="w-[132px] text-right">Final Budget</Th>
                <Th className="w-[132px] text-right">Obligated</Th>
                <Th className="w-[132px] text-right">Disbursed</Th>
                <Th className="w-[132px] text-right">Unused</Th>
                <Th className="w-[88px] text-right">Used</Th>
              </tr>
            </thead>
            <tbody>
              {page.lines.map((line) => (
                <BudgetActualRow
                  key={`${line.expense_class}-${line.office_name}-${line.account_code}`}
                  line={line}
                />
              ))}
            </tbody>
          </table>
        </div>
      )}

      <Signatures
        signatories={[
          { act: 'Certified correct:', role: 'Municipal Accountant' },
          { act: 'Noted:', role: 'Municipal Mayor' },
        ]}
      />
    </div>
  )
}

function BudgetActualRow({ line }: { line: BudgetActualLine }) {
  return (
    <tr className="border-b border-line hover:bg-hover">
      <Td className="text-ink-secondary">{line.expense_class}</Td>
      <Td className="max-w-0 truncate">{line.office_name}</Td>
      <Td className="max-w-0 truncate">{line.account_name}</Td>
      <Td className="tabular text-ink-secondary">{line.account_code}</Td>
      <Td className="text-right tabular">{formatAmount(line.original_budget)}</Td>
      <Td className="text-right tabular">{formatAmount(line.final_budget)}</Td>
      <Td className="text-right tabular">{formatAmount(line.obligated)}</Td>
      <Td className="text-right tabular">{formatAmount(line.disbursed)}</Td>
      <Td className="text-right tabular">{formatAmount(line.unused_budget)}</Td>
      <Td className="text-right tabular text-ink-secondary">{line.utilisation_rate}%</Td>
    </tr>
  )
}

function Nothing({ children }: { children: React.ReactNode }) {
  return (
    <p className="mx-auto max-w-[60ch] px-6 py-12 text-center text-ink-secondary">{children}</p>
  )
}
