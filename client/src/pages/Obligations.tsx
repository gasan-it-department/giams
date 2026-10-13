import { useState } from 'react'
import { Check, FileText, Inbox, PenLine } from 'lucide-react'
import { EmptyList, ListPage, Row, type ListTab } from '@/components/ListPage'
import { ErrorNotice } from '@/components/ErrorNotice'
import { useDebounced } from '@/components/SearchField'
import { formatAmount } from '@/lib/money'
import {
  useCertifyObligation, useObligations, useRaod,
  type CertificationBox, type Obligation, type RaodEntry,
} from '@/queries/budget'
import { useAuthStore } from '@/stores/authStore'
import { usePeriodRange } from '@/stores/periodStore'

/**
 * Obligation requests.
 *
 * Organised around the question a Budget Officer or Accountant opens it with:
 * *what is waiting for my signature?* The tabs are the certification stages,
 * and a request waiting on the reader reads bold the way unread mail does.
 */
export function Obligations() {
  const [tab, setTab] = useState('all')
  const can = useAuthStore((s) => s.can)
  const period = usePeriodRange()
  const [search, setSearch] = useState('')
  const term = useDebounced(search.trim())

  const filter = {
    fiscal_year: period.fiscalYear,
    ...(tab === 'all' ? {} : { status: tab }),
    ...(term ? { search: term } : {}),
  }

  const { data: obligations, error, isPending, isFetching, refetch } = useObligations(filter)
  const { data: registry } = useRaod(period.fiscalYear)
  const { data: all } = useObligations({ fiscal_year: period.fiscalYear })

  const certify = useCertifyObligation()

  const count = (status: string) => all?.filter((o) => o.status === status).length ?? 0

  const tabs: ListTab[] = [
    { key: 'all',       label: 'All',            icon: Inbox },
    { key: 'draft',     label: 'Drafts',         icon: PenLine,  count: count('draft') },
    { key: 'submitted', label: 'For certifying', icon: FileText, count: count('submitted'), attention: true },
    { key: 'certified', label: 'Certified',      icon: Check,    count: count('certified') },
  ]

  return (
    <ListPage
      tabs={tabs}
      activeTab={tab}
      onTabChange={setTab}
      total={obligations?.length}
      onRefresh={() => void refetch()}
      refreshing={isFetching}
      toolbar={registry && registry.length > 0 ? <RegistryStrip entries={registry} /> : undefined}
      search={{
        value: search,
        onChange: setSearch,
        placeholder: 'Search ORS number, payee or particulars',
      }}
    >
      {error && <div className="p-4"><ErrorNotice error={error} /></div>}
      {certify.error && <div className="p-4"><ErrorNotice error={certify.error} /></div>}

      {isPending && <p className="px-4 py-6 text-ink-muted">Loading…</p>}

      {obligations?.length === 0 && (
        term ? (
          <EmptyList
            title={`No obligation request matches "${term}".`}
            body="Search by ORS number, payee name or a word from the particulars."
          />
        ) : (
          <EmptyList
            title="Nothing here."
            body="Obligation requests appear once an office commits against a released allotment."
          />
        )
      )}

      {obligations?.map((ob) => (
        <ObligationRow
          key={ob.id}
          obligation={ob}
          canCertifyBudget={can('obligation.certify_budget')}
          canCertifyAccounting={can('obligation.certify_accounting')}
          busy={certify.isPending}
          onCertify={(box) => certify.mutate({ id: ob.id, box })}
        />
      ))}
    </ListPage>
  )
}

function ObligationRow({
  obligation, canCertifyBudget, canCertifyAccounting, busy, onCertify,
}: {
  obligation: Obligation
  canCertifyBudget: boolean
  canCertifyAccounting: boolean
  busy: boolean
  onCertify: (box: CertificationBox) => void
}) {
  const { certified_budget_name: boxB, certified_accounting_name: boxC } = obligation

  // Waiting on this reader if a box they can sign is still empty.
  const needsAction =
    (boxB === null && canCertifyBudget) ||
    (boxB !== null && boxC === null && canCertifyAccounting)

  return (
    <Row needsAction={needsAction} done={boxB !== null && boxC !== null}>
      <span className="w-[168px] shrink-0 truncate">
        {obligation.payee_name ?? obligation.office_name}
      </span>

      <span className="min-w-0 flex-1 truncate">
        <code className="tabular text-sm">{obligation.ors_no}</code>
        <span className="font-normal text-ink-secondary"> — {obligation.particulars}</span>
      </span>

      <span className="w-[44px] shrink-0 text-xs text-ink-muted">{obligation.expense_class}</span>

      <SignatureBox
        label="Box B" who={boxB} waiting="Budget Officer"
        canSign={canCertifyBudget} busy={busy} onSign={() => onCertify('budget')}
      />
      <SignatureBox
        label="Box C" who={boxC} waiting="Accountant"
        canSign={canCertifyAccounting && boxB !== null} busy={busy}
        onSign={() => onCertify('accounting')}
      />

      <span className="w-[100px] shrink-0 text-right tabular">
        {formatAmount(obligation.amount)}
      </span>

      <span className="w-[46px] shrink-0 text-right text-xs text-ink-muted tabular">
        {obligation.obligation_date.slice(5)}
      </span>
    </Row>
  )
}

/**
 * One certification box, compressed to fit a row.
 *
 * Signed shows a tick and the name on hover; unsigned and actionable is a
 * button; unsigned and not yours is a quiet dash. The row then explains itself
 * without anyone needing the ORS form by heart.
 */
function SignatureBox({
  label, who, waiting, canSign, busy, onSign,
}: {
  label: string
  who: string | null
  waiting: string
  canSign: boolean
  busy: boolean
  onSign: () => void
}) {
  if (who) {
    return (
      <span
        title={`${label} certified by ${who}`}
        className="flex w-[76px] shrink-0 items-center gap-1 text-xs text-ok-ink"
      >
        <Check className="size-3.5 shrink-0" aria-hidden />
        <span className="truncate">{who.split(' ')[0]}</span>
      </span>
    )
  }

  if (!canSign) {
    return (
      <span
        title={`Waiting for the ${waiting}`}
        className="w-[76px] shrink-0 text-xs text-ink-muted"
      >
        {label} —
      </span>
    )
  }

  return (
    <button
      type="button"
      onClick={(e) => { e.stopPropagation(); onSign() }}
      disabled={busy}
      className="w-[76px] shrink-0 rounded-pill bg-accent-fill px-2 py-0.5 text-xs font-medium text-accent-ink hover:brightness-[0.97] disabled:opacity-60"
    >
      Sign {label.replace('Box ', '')}
    </button>
  )
}

/**
 * The RAOD, compressed into the toolbar.
 *
 * A Budget Officer checks the remaining balance before releasing anything, so
 * it belongs where it is always visible rather than in a panel that scrolls
 * away.
 *
 * It shows the *largest* remaining balance and the count, not a total. Summing
 * the balances would mean adding peso figures in JavaScript, which this client
 * does not do — see `lib/money.ts`. A cross-allotment total is a real thing to
 * want, and when it is wanted the RAOD endpoint should return it, computed in
 * exact decimal alongside the rows it summarises.
 */
function RegistryStrip({ entries }: { entries: RaodEntry[] }) {
  // Comparison, not arithmetic: amounts are fixed-width decimal strings, so
  // the widest one sorts correctly as text once padded.
  const largest = entries.reduce((max, e) =>
    e.unobligated_balance.padStart(20, '0') > max.unobligated_balance.padStart(20, '0') ? e : max,
  )

  return (
    <span className="ml-3 flex items-center gap-2 text-xs text-ink-secondary">
      <span className="text-ink-muted">Largest unobligated</span>
      <span className="tabular font-medium text-ink">
        {formatAmount(largest.unobligated_balance)}
      </span>
      <span className="text-ink-muted">
        of {entries.length} allotment{entries.length === 1 ? '' : 's'}
      </span>
    </span>
  )
}
