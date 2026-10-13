import { useState } from 'react'
import { BookCheck, ChevronDown, ChevronRight, Clock, Inbox, PencilLine } from 'lucide-react'
import { EmptyList, ListPage, Row, type ListTab } from '@/components/ListPage'
import { ErrorNotice } from '@/components/ErrorNotice'
import { useDebounced } from '@/components/SearchField'
import { formatAmount } from '@/lib/money'
import {
  useAdvanceVoucher, usePostVoucher, useVoucherDeductions, useVouchers,
  type Voucher, type VoucherStep,
} from '@/queries/vouchers'
import { useAuthStore } from '@/stores/authStore'
import { usePeriodRange } from '@/stores/periodStore'

/** The step each status is waiting for, and who takes it. */
const NEXT_STEP: Record<string, { step: VoucherStep; label: string; permission: string }> = {
  draft:     { step: 'submit',  label: 'Submit',  permission: 'dv.draft' },
  submitted: { step: 'certify', label: 'Certify', permission: 'dv.approve' },
  certified: { step: 'approve', label: 'Approve', permission: 'dv.approve' },
}

/**
 * The voucher register.
 *
 * Reads as a work queue rather than a table: what a clerk wants on opening it
 * is *which vouchers are waiting on me*, so the action a voucher needs sits on
 * its row and the amounts stay right-aligned and scannable.
 *
 * Expanding a row shows the deductions the server computed, each with the
 * issuance it came from — the answer to "why 2%?" without leaving the screen.
 */
export function Vouchers() {
  const [tab, setTab] = useState('all')
  const [expanded, setExpanded] = useState<string | null>(null)
  const period = usePeriodRange()
  const [search, setSearch] = useState('')
  const term = useDebounced(search.trim())

  const filter = {
    fiscal_year: period.fiscalYear,
    ...(tab === 'all' ? {} : { status: tab }),
    ...(term ? { search: term } : {}),
  }
  const { data: vouchers, error, isPending, isFetching, refetch } = useVouchers(filter)

  const advance = useAdvanceVoucher()
  const post = usePostVoucher()

  // Counts come from the unfiltered list so a tab does not report on itself.
  const { data: all } = useVouchers({ fiscal_year: period.fiscalYear })
  const count = (status: string) => all?.filter((v) => v.status === status).length ?? 0

  const tabs: ListTab[] = [
    { key: 'all',       label: 'All',           icon: Inbox },
    { key: 'draft',     label: 'Drafts',        icon: PencilLine, count: count('draft') },
    { key: 'submitted', label: 'For certifying', icon: Clock, count: count('submitted'), attention: true },
    { key: 'certified', label: 'For approval',  icon: Clock,      count: count('certified'), attention: true },
    { key: 'approved',  label: 'For posting',   icon: BookCheck,  count: count('approved'), attention: true },
  ]

  return (
    <ListPage
      tabs={tabs}
      activeTab={tab}
      onTabChange={setTab}
      total={vouchers?.length}
      onRefresh={() => void refetch()}
      refreshing={isFetching}
      search={{
        value: search,
        onChange: (v) => { setSearch(v); setExpanded(null) },
        placeholder: 'Search DV number, payee or particulars',
      }}
    >
      {error && <div className="p-4"><ErrorNotice error={error} /></div>}
      {advance.error && <div className="p-4"><ErrorNotice error={advance.error} /></div>}
      {post.error && <div className="p-4"><ErrorNotice error={post.error} /></div>}

      {isPending && <p className="px-4 py-6 text-ink-muted">Loading…</p>}

      {vouchers?.length === 0 && (
        term ? (
          <EmptyList
            title={`No voucher matches "${term}".`}
            body="Search by DV number, payee name or a word from the particulars."
          />
        ) : (
          <EmptyList
            title="Nothing here."
            body="A voucher is raised against an obligation request once the Budget Officer and the Accountant have both certified it."
          />
        )
      )}

      {vouchers?.map((v) => (
        <VoucherRow
          key={v.id}
          voucher={v}
          expanded={expanded === v.id}
          onToggle={() => setExpanded(expanded === v.id ? null : v.id)}
          busy={advance.isPending || post.isPending}
          onAdvance={(step) => advance.mutate({ id: v.id, step })}
          onPost={() => post.mutate(v.id)}
        />
      ))}
    </ListPage>
  )
}

function VoucherRow({
  voucher, expanded, onToggle, busy, onAdvance, onPost,
}: {
  voucher: Voucher
  expanded: boolean
  onToggle: () => void
  busy: boolean
  onAdvance: (step: VoucherStep) => void
  onPost: () => void
}) {
  const can = useAuthStore((s) => s.can)
  const next = NEXT_STEP[voucher.status]

  // A voucher waiting on this user reads louder, the way unread mail does.
  const needsAction = next !== undefined && can(next.permission)

  return (
    <>
      <Row needsAction={needsAction} done={voucher.posted}>
        <button
          type="button"
          onClick={onToggle}
          aria-expanded={expanded}
          aria-label={expanded ? 'Hide deductions' : 'Show deductions'}
          className="shrink-0 text-ink-muted hover:text-ink"
        >
          {expanded
            ? <ChevronDown className="size-4" aria-hidden />
            : <ChevronRight className="size-4" aria-hidden />}
        </button>

        <span className="w-[168px] shrink-0 truncate">{voucher.payee_name}</span>

        <span className="min-w-0 flex-1 truncate">
          <code className="tabular text-sm">{voucher.dv_no}</code>
          <span className="font-normal text-ink-secondary"> — {voucher.particulars}</span>
        </span>

        {needsAction && next && (
          <button
            type="button"
            onClick={(e) => { e.stopPropagation(); onAdvance(next.step) }}
            disabled={busy}
            className="shrink-0 rounded-pill bg-accent-fill px-3 py-0.5 text-xs font-medium text-accent-ink hover:brightness-[0.97] disabled:opacity-60"
          >
            {next.label}
          </button>
        )}

        {voucher.status === 'approved' && !voucher.posted && can('jev.post') && (
          <button
            type="button"
            onClick={(e) => { e.stopPropagation(); onPost() }}
            disabled={busy}
            className="shrink-0 rounded-pill bg-accent-fill px-3 py-0.5 text-xs font-medium text-accent-ink hover:brightness-[0.97] disabled:opacity-60"
          >
            Post
          </button>
        )}

        <span className="w-[100px] shrink-0 text-right tabular">
          {formatAmount(voucher.net_amount)}
        </span>

        <span className="w-[46px] shrink-0 text-right text-xs text-ink-muted tabular">
          {voucher.dv_date.slice(5)}
        </span>
      </Row>

      {expanded && (
        <div className="border-b border-line bg-sunken px-4 py-3 pl-12">
          <Deductions voucherId={voucher.id} voucher={voucher} />
        </div>
      )}
    </>
  )
}

/**
 * The deduction lines, with the issuance each rate came from.
 *
 * Showing the authority is the point: an auditor asking "why 2%?" gets the
 * circular from the voucher itself, and it is the rate that was in force on
 * the voucher date rather than whatever is current today.
 */
function Deductions({ voucherId, voucher }: { voucherId: string; voucher: Voucher }) {
  const { data, error, isPending } = useVoucherDeductions(voucherId)

  return (
    <div className="ml-7">
      {isPending && <p className="text-sm text-ink-muted">Loading deductions…</p>}
      {error && <ErrorNotice error={error} />}

      {data?.length === 0 && (
        <p className="text-sm text-ink-muted">No deductions on this voucher.</p>
      )}

      {data && data.length > 0 && (
        <>
          <div className="flex items-baseline gap-3 text-sm">
            <span className="w-[92px] shrink-0 text-ink-muted">Gross</span>
            <span className="flex-1" />
            <span className="w-[104px] shrink-0 text-right tabular">
              {formatAmount(voucher.gross_amount)}
            </span>
          </div>

          {data.map((d) => (
            <div key={d.code} className="mt-1.5 flex items-baseline gap-3 text-sm">
              <span className="w-[92px] shrink-0 text-ink-muted">{d.code}</span>
              <span className="min-w-0 flex-1 truncate text-ink-secondary">
                {d.name}
                {d.authority && <span className="text-ink-muted"> · {d.authority}</span>}
              </span>
              <span className="w-[104px] shrink-0 text-right tabular text-ink-secondary">
                ({formatAmount(d.amount_deducted)})
              </span>
            </div>
          ))}

          <div className="mt-2 flex items-baseline gap-3 border-t border-line pt-2 text-sm">
            <span className="w-[92px] shrink-0 font-medium">Net</span>
            <span className="flex-1 text-ink-muted">
              paid to {voucher.payee_name}
            </span>
            <span className="w-[104px] shrink-0 text-right tabular font-medium">
              {formatAmount(voucher.net_amount)}
            </span>
          </div>
        </>
      )}
    </div>
  )
}
