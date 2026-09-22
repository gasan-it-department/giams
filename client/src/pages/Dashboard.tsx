import { useNavigate } from 'react-router-dom'
import { AlertTriangle, ChevronRight, RefreshCw } from 'lucide-react'
import { ErrorNotice } from '@/components/ErrorNotice'
import { formatAmount } from '@/lib/money'
import type { Amount } from '@/lib/money'
import { useDashboard, type Attention, type BudgetUse, type Figures } from '@/queries/dashboard'
import { usePeriodRange } from '@/stores/periodStore'

/**
 * Where things stand, for the period selected.
 *
 * Three questions, in the order the office asks them: how much cash is there,
 * what moved, and what is waiting on somebody. The third is the reason this
 * screen exists — a voucher certified in March and never approved is invisible
 * everywhere else in the system, because everywhere else is organised by
 * document rather than by what is stuck.
 *
 * Nothing here is a chart for its own sake. Every figure is a number an
 * accountant would write down, and every row of the attention list goes
 * somewhere.
 */

const EXPENSE_CLASS: Record<string, string> = {
  PS:   'Personal Services',
  MOOE: 'Maintenance and Other Operating Expenses',
  CO:   'Capital Outlay',
  FE:   'Financial Expenses',
}

export function Dashboard() {
  const period = usePeriodRange()
  const query = useDashboard(period.from, period.to)

  return (
    <div className="rounded-lg border border-line bg-surface">
      <header className="flex items-center justify-between border-b border-line px-6 py-3">
        <div>
          <h1 className="text-lg">Where things stand</h1>
          <p className="text-sm text-ink-muted">{period.label}</p>
        </div>
        <button
          type="button"
          onClick={() => void query.refetch()}
          className="flex items-center gap-2 rounded-pill px-3 py-1 text-sm hover:bg-hover"
        >
          <RefreshCw className={`size-4 ${query.isFetching ? 'animate-spin' : ''}`} aria-hidden />
          Refresh
        </button>
      </header>

      {query.error && <div className="p-4"><ErrorNotice error={query.error} /></div>}
      {query.isPending && <p className="px-6 py-10 text-ink-muted">Loading…</p>}

      {query.data && (
        <>
          <FigureRow figures={query.data.figures} asOf={query.data.as_of} />
          <AttentionList items={query.data.attention} />
          <BudgetPanel lines={query.data.budget} />
        </>
      )}
    </div>
  )
}

// ── The four figures ─────────────────────────────────────────────────────────

function FigureRow({ figures, asOf }: { figures: Figures; asOf: string }) {
  return (
    <div className="grid grid-cols-2 divide-x divide-line border-b border-line md:grid-cols-4">
      <Figure
        label="Cash and cash equivalents"
        note={`as at ${asOf}`}
        amount={figures.cash}
      />
      <Figure label="Received in the period" amount={figures.received} />
      <Figure label="Paid out in the period" amount={figures.paid} />
      <Figure
        label="Surplus for the period"
        note="income less expenses, before closing"
        amount={figures.surplus}
      />
    </div>
  )
}

function Figure({
  label, amount, note,
}: {
  label: string
  amount: Amount
  note?: string
}) {
  // A deficit is stated in words as well as by its sign. A leading minus on a
  // wide screen is easy to read past, and reading past it is expensive.
  const deficit = amount.startsWith('-')

  return (
    <div className="px-6 py-4">
      <p className="text-sm text-ink-secondary">{label}</p>
      <p className={`mt-1 text-xl tabular ${deficit ? 'text-danger-ink' : ''}`}>
        {formatAmount(amount)}
      </p>
      <p className="mt-0.5 text-xs text-ink-muted">
        {deficit ? 'a deficit' : note ?? ' '}
      </p>
    </div>
  )
}

// ── What is waiting on somebody ──────────────────────────────────────────────

function AttentionList({ items }: { items: Attention[] }) {
  const navigate = useNavigate()

  return (
    <section className="border-b border-line">
      <h2 className="px-6 pb-2 pt-4 text-sm text-ink-secondary">Waiting on someone</h2>

      {items.length === 0 ? (
        <p className="px-6 pb-4 text-ink-muted">
          Nothing is waiting. Every voucher, obligation and entry has been dealt with.
        </p>
      ) : (
        <ul>
          {items.map((item) => (
            <li key={item.key}>
              <button
                type="button"
                onClick={() => navigate(item.route)}
                className={`flex w-full items-center gap-3 border-t border-line px-6 py-2 text-left hover:bg-hover ${
                  item.urgent ? 'bg-danger-fill text-danger-ink' : ''
                }`}
              >
                {item.urgent && <AlertTriangle className="size-4 shrink-0" aria-hidden />}
                <span className="w-10 shrink-0 text-right tabular font-medium">{item.count}</span>
                <span className="grow">{item.label}</span>
                <ChevronRight className="size-4 shrink-0 text-ink-muted" aria-hidden />
              </button>
            </li>
          ))}
        </ul>
      )}
    </section>
  )
}

// ── How much of the budget has been used ─────────────────────────────────────

function BudgetPanel({ lines }: { lines: BudgetUse[] }) {
  return (
    <section>
      <h2 className="px-6 pb-2 pt-4 text-sm text-ink-secondary">Budget used, by class</h2>

      {lines.length === 0 ? (
        <p className="px-6 pb-5 text-ink-muted">
          No appropriations have been recorded for this year yet.
        </p>
      ) : (
        <ul className="pb-2">
          {lines.map((line) => (
            <BudgetRow key={line.expense_class} line={line} />
          ))}
        </ul>
      )}
    </section>
  )
}

function BudgetRow({ line }: { line: BudgetUse }) {
  // The bar is a reading aid for the figure beside it, not the figure itself,
  // so it is clamped rather than allowed to run off the row when a class is
  // over-obligated — a state the ledger refuses, but the display should not
  // break if it ever arrives from older data.
  const used = Math.min(100, Math.max(0, Number(line.utilisation_rate)))
  const over = Number(line.utilisation_rate) > 100

  return (
    <li className="border-t border-line px-6 py-3">
      <div className="flex items-baseline justify-between gap-4">
        <span>{EXPENSE_CLASS[line.expense_class] ?? line.expense_class}</span>
        <span className="text-sm text-ink-secondary tabular">
          {formatAmount(line.obligated)} obligated of {formatAmount(line.budget)}
        </span>
      </div>

      <div className="mt-2 flex items-center gap-3">
        <div className="h-1.5 grow overflow-hidden rounded-pill bg-sunken">
          <div
            className={`h-full rounded-pill ${over ? 'bg-danger-ink' : 'bg-accent-solid'}`}
            style={{ width: `${used}%` }}
          />
        </div>
        <span className="w-14 shrink-0 text-right text-sm tabular text-ink-secondary">
          {line.utilisation_rate}%
        </span>
      </div>

      <p className="mt-1 text-xs text-ink-muted tabular">
        {formatAmount(line.disbursed)} of that has actually been paid
      </p>
    </li>
  )
}
