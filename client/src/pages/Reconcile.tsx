import { useEffect, useState } from 'react'
import { Check, Link2, Link2Off, RefreshCw, Sparkles } from 'lucide-react'
import { ErrorNotice } from '@/components/ErrorNotice'
import { formatAmount } from '@/lib/money'
import {
  useBankAccounts, useMatchLine, useRematch, useStatementLines, useStatements,
  useSuggestions, useUnmatchLine,
  type StatementLine, type Suggestion,
} from '@/queries/reconciliation'

/**
 * The matching workspace.
 *
 * Two panes, because reconciling is a two-sided job: the bank's version on the
 * left, and what the books say on the right. Selecting an unmatched line asks
 * the server for candidates; nothing is ever matched without somebody choosing.
 *
 * The tier is shown on every match. An accountant reviewing a statement should
 * be able to skim the confident ones and spend attention on the rest, and an
 * auditor asking why two records were paired gets an answer.
 */
export function Reconcile() {
  const [bankId, setBankId] = useState<string | null>(null)
  const [statementId, setStatementId] = useState<string | null>(null)
  const [selectedLine, setSelectedLine] = useState<StatementLine | null>(null)

  const { data: banks } = useBankAccounts()
  const { data: statements, error: statementsError } = useStatements(bankId ?? undefined)
  const { data: lines, error: linesError, isPending } = useStatementLines(statementId)

  const rematch = useRematch()
  const unmatch = useUnmatchLine()

  // Settle on a statement once one exists, so the screen is useful on arrival
  // rather than asking for two choices before showing anything.
  useEffect(() => {
    if (statementId === null && statements && statements.length > 0) {
      setStatementId(statements[0]?.id ?? null)
    }
  }, [statements, statementId])

  const statement = statements?.find((s) => s.id === statementId)

  return (
    <>
      <div className="flex items-baseline justify-between gap-4">
        <div>
          <h1 className="text-xl font-medium">Bank reconciliation</h1>
          <p className="mt-1 text-ink-secondary">
            Matching the bank statement against the payments and collections in the books.
          </p>
        </div>
        {statement && (
          <span className="shrink-0 text-sm text-ink-muted tabular">
            {statement.unmatched_count} of {statement.line_count} unmatched
          </span>
        )}
      </div>

      <div className="mt-5 flex flex-wrap items-center gap-2">
        <select
          value={bankId ?? ''}
          onChange={(e) => {
            setBankId(e.target.value || null)
            setStatementId(null)
            setSelectedLine(null)
          }}
          aria-label="Bank account"
          className="rounded-md bg-sunken px-3 py-2 text-sm"
        >
          <option value="">All bank accounts</option>
          {banks?.map((b) => (
            <option key={b.id} value={b.id}>
              {b.bank_name} · {b.account_no}
            </option>
          ))}
        </select>

        <select
          value={statementId ?? ''}
          onChange={(e) => {
            setStatementId(e.target.value || null)
            setSelectedLine(null)
          }}
          aria-label="Statement"
          className="rounded-md bg-sunken px-3 py-2 text-sm"
        >
          <option value="">Choose a statement</option>
          {statements?.map((s) => (
            <option key={s.id} value={s.id}>
              {s.period_start} to {s.period_end} · {s.bank_name}
            </option>
          ))}
        </select>

        {statementId && (
          <button
            type="button"
            onClick={() => rematch.mutate(statementId)}
            disabled={rematch.isPending}
            className="flex items-center gap-2 rounded-pill bg-accent-fill px-3.5 py-2 text-sm font-medium text-accent-ink hover:brightness-[0.97] disabled:opacity-60"
          >
            <RefreshCw className={`size-4 ${rematch.isPending ? 'animate-spin' : ''}`} aria-hidden />
            Match again
          </button>
        )}
      </div>

      {rematch.data && (
        <p className="mt-4 rounded-md bg-info-fill px-4 py-3 text-info-ink">
          {rematch.data.message}
        </p>
      )}

      {statementsError && <div className="mt-5"><ErrorNotice error={statementsError} /></div>}
      {linesError && <div className="mt-5"><ErrorNotice error={linesError} /></div>}
      {unmatch.error && <div className="mt-5"><ErrorNotice error={unmatch.error} /></div>}

      {statements?.length === 0 && (
        <div className="mt-8 rounded-md bg-sunken p-8 text-center">
          <p className="font-medium">No bank statements have been imported yet.</p>
          <p className="mt-1 text-ink-secondary">
            Import the statement file from the bank to begin reconciling.
          </p>
        </div>
      )}

      {isPending && statementId && <p className="mt-6 text-ink-muted">Loading the statement…</p>}

      {lines && lines.length > 0 && (
        <div className="mt-5 flex gap-4">
          <div className="min-w-0 flex-1">
            <h2 className="mb-2 ml-1 text-xs text-ink-muted">From the bank</h2>
            <div className="row-list">
            {lines.map((line) => (
              <StatementRow
                key={line.id}
                line={line}
                selected={selectedLine?.id === line.id}
                onSelect={() => setSelectedLine(line.matched_id ? null : line)}
                onUnmatch={() => unmatch.mutate(line.id)}
                busy={unmatch.isPending}
              />
            ))}
            </div>
          </div>

          <div className="w-[340px] shrink-0">
            <h2 className="mb-2 ml-1 text-xs text-ink-muted">From the books</h2>
            <SuggestionPane line={selectedLine} onMatched={() => setSelectedLine(null)} />
          </div>
        </div>
      )}
    </>
  )
}

function StatementRow({
  line, selected, onSelect, onUnmatch, busy,
}: {
  line: StatementLine
  selected: boolean
  onSelect: () => void
  onUnmatch: () => void
  busy: boolean
}) {
  const isMatched = line.matched_id !== null
  const amount = line.withdrawal !== '0.00' ? line.withdrawal : line.deposit
  const outgoing = line.withdrawal !== '0.00'

  return (
    <div
      className={`border-b border-line px-3 py-2.5 last:border-b-0 ${
        selected ? 'bg-accent-soft' : isMatched ? 'bg-sunken' : 'hover:bg-hover'
      }`}
    >
      <div className="flex items-baseline gap-3">
        <button
          type="button"
          onClick={onSelect}
          disabled={isMatched}
          className="min-w-0 flex-1 text-left disabled:cursor-default"
        >
          <span className="truncate">{line.description}</span>
          <span className="ml-2 text-sm text-ink-muted tabular">
            {line.txn_date}
            {line.reference && ` · ${line.reference}`}
          </span>
        </button>

        <span className={`shrink-0 tabular ${outgoing ? '' : 'text-ok-ink'}`}>
          {outgoing ? '' : '+'}{formatAmount(amount)}
        </span>
      </div>

      <div className="mt-1.5 flex items-center gap-2 text-sm">
        {isMatched ? (
          <>
            <MatchTier tier={line.match_tier} />
            <span className="min-w-0 truncate text-ink-secondary">
              {line.matched_reference && <code className="tabular">{line.matched_reference}</code>}
              {line.matched_counterparty && ` ${line.matched_counterparty}`}
            </span>
            <button
              type="button"
              onClick={onUnmatch}
              disabled={busy}
              title="Remove this match"
              className="ml-auto flex shrink-0 items-center gap-1 rounded-pill px-2 py-0.5 text-xs text-ink-muted hover:bg-hover disabled:opacity-60"
            >
              <Link2Off className="size-3.5" aria-hidden />
              Unmatch
            </button>
          </>
        ) : (
          <span className="rounded-pill bg-warn-fill px-2 py-0.5 text-xs text-warn-ink">
            Not matched
          </span>
        )}
      </div>
    </div>
  )
}

/**
 * How a match was arrived at.
 *
 * `exact` came from a reference number, `amount` from an unambiguous amount,
 * `manual` from a person. Showing it lets a reviewer skim the confident ones.
 */
function MatchTier({ tier }: { tier: string | null }) {
  const label =
    tier === 'exact'  ? 'Matched on reference'
    : tier === 'amount' ? 'Matched on amount'
    : tier === 'manual' ? 'Matched by hand'
    : 'Matched'

  const tone = tier === 'amount' ? 'bg-info-fill text-info-ink' : 'bg-ok-fill text-ok-ink'

  return (
    <span className={`flex shrink-0 items-center gap-1 rounded-pill px-2 py-0.5 text-xs ${tone}`}>
      <Check className="size-3" aria-hidden />
      {label}
    </span>
  )
}

function SuggestionPane({
  line, onMatched,
}: {
  line: StatementLine | null
  onMatched: () => void
}) {
  const { data, error, isPending } = useSuggestions(line?.id ?? null)
  const match = useMatchLine()

  if (!line) {
    return (
      <div className="rounded-md bg-sunken p-6 text-center text-sm text-ink-secondary">
        Choose a line on the left that has not been matched, and the closest
        records from the books will be shown here.
      </div>
    )
  }

  return (
    <div className="rounded-md bg-sunken p-3">
      <p className="mb-3 text-sm text-ink-secondary">
        Closest records to{' '}
        <span className="tabular">
          {formatAmount(line.withdrawal !== '0.00' ? line.withdrawal : line.deposit)}
        </span>{' '}
        on {line.txn_date}.
      </p>

      {isPending && <p className="text-sm text-ink-muted">Looking…</p>}
      {error && <ErrorNotice error={error} />}
      {match.error && <div className="mb-3"><ErrorNotice error={match.error} /></div>}

      {data?.length === 0 && (
        <p className="rounded-md bg-surface px-3 py-4 text-center text-sm text-ink-secondary">
          Nothing in the books resembles this line. It may be a bank charge or
          interest that still needs recording.
        </p>
      )}

      {data?.map((s) => (
        <SuggestionCard
          key={s.record_id}
          suggestion={s}
          busy={match.isPending}
          onAccept={() =>
            match.mutate(
              { lineId: line.id, kind: s.kind, recordId: s.record_id },
              { onSuccess: onMatched },
            )
          }
        />
      ))}
    </div>
  )
}

function SuggestionCard({
  suggestion, busy, onAccept,
}: {
  suggestion: Suggestion
  busy: boolean
  onAccept: () => void
}) {
  return (
    <div className="mb-2 rounded-md bg-surface p-3">
      <div className="flex items-baseline gap-2">
        {suggestion.record_ref && (
          <code className="shrink-0 tabular text-sm">{suggestion.record_ref}</code>
        )}
        <span className="min-w-0 flex-1 truncate text-sm">{suggestion.counterparty}</span>
        <span className="shrink-0 tabular text-sm">{formatAmount(suggestion.record_amount)}</span>
      </div>

      <div className="mt-1 flex items-center gap-2 text-xs text-ink-muted">
        <span className="tabular">{suggestion.record_date}</span>
        {suggestion.why && (
          <span className="flex min-w-0 items-center gap-1 truncate">
            <Sparkles className="size-3 shrink-0" aria-hidden />
            {suggestion.why}
          </span>
        )}
      </div>

      <button
        type="button"
        onClick={onAccept}
        disabled={busy}
        className="mt-2.5 flex w-full items-center justify-center gap-1.5 rounded-pill bg-accent-fill px-3 py-1.5 text-xs font-medium text-accent-ink hover:brightness-[0.97] disabled:opacity-60"
      >
        <Link2 className="size-3.5" aria-hidden />
        This is the one
      </button>
    </div>
  )
}
