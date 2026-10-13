import { useMemo, useState } from 'react'
import { Coins, Landmark, PiggyBank, Receipt, Scale, Wallet } from 'lucide-react'
import { EmptyList, ListPage, Row, type ListTab } from '@/components/ListPage'
import { ErrorNotice } from '@/components/ErrorNotice'
import { useDebounced } from '@/components/SearchField'
import { PAGE_SIZE, useAccounts } from '@/queries/accounts'
import {
  ACCOUNT_TYPE_LABELS, type Account, type AccountType,
} from '@/schemas/accounts'

/**
 * The chart of accounts.
 *
 * The full Annex B chart, close to six hundred accounts, so density is the
 * whole game: the question a clerk arrives with is "what is the code for X",
 * and the answer should be one glance and one keystroke away rather than a
 * scroll. Clicking a row opens what the account is for.
 *
 * Account groups are tabs rather than headings, which keeps every row at the
 * same indentation and lets the list run unbroken.
 */

const TABS: ListTab[] = [
  { key: 'all',       label: 'All',         icon: Landmark },
  { key: 'asset',     label: 'Assets',      icon: Wallet },
  { key: 'liability', label: 'Liabilities', icon: Scale },
  { key: 'equity',    label: 'Equity',      icon: PiggyBank },
  { key: 'income',    label: 'Income',      icon: Coins },
  { key: 'expense',   label: 'Expenses',    icon: Receipt },
]

export function ChartOfAccounts() {
  const [tab, setTab] = useState('all')
  const [page, setPage] = useState(0)
  const [openId, setOpenId] = useState<string | null>(null)
  const [search, setSearch] = useState('')
  const term = useDebounced(search.trim())

  const filter = useMemo(
    () => ({
      ...(tab === 'all' ? {} : { account_type: tab as AccountType }),
      ...(term ? { search: term } : {}),
      ...(page > 0 ? { page } : {}),
    }),
    [tab, term, page],
  )

  const { data, error, isPending, isFetching, refetch } = useAccounts(filter)
  const accounts = data?.items

  const tabs = TABS.map((t) => ({
    ...t,
    label: t.key === 'all' ? 'All' : ACCOUNT_TYPE_LABELS[t.key as AccountType],
  }))

  return (
    <ListPage
      tabs={tabs}
      activeTab={tab}
      onTabChange={(key) => { setTab(key); setPage(0); setOpenId(null) }}
      total={data?.total}
      page={page}
      pageSize={PAGE_SIZE}
      onPageChange={(p) => { setPage(p); setOpenId(null) }}
      onRefresh={() => void refetch()}
      refreshing={isFetching}
      search={{
        value: search,
        onChange: (v) => { setSearch(v); setPage(0); setOpenId(null) },
        placeholder: 'Search account number or title',
      }}
    >
      {error && <div className="p-4"><ErrorNotice error={error} /></div>}

      {isPending && <p className="px-4 py-6 text-ink-muted">Loading…</p>}

      {accounts?.length === 0 && (
        term ? (
          <EmptyList
            title={`No account matches "${term}".`}
            body="Try part of the title, like “depreciation”, or the first digits of the number, like 1-07."
          />
        ) : (
          <EmptyList
            title="No accounts in this group."
            body="Try another group, or search by number or title."
          />
        )
      )}

      {accounts?.map((account) => (
        <AccountRow
          key={account.id}
          account={account}
          open={openId === account.id}
          onToggle={() => setOpenId(openId === account.id ? null : account.id)}
        />
      ))}
    </ListPage>
  )
}

function AccountRow({
  account, open, onToggle,
}: {
  account:  Account
  open:     boolean
  onToggle: () => void
}) {
  // Summary accounts roll up and cannot be posted to, so they read as headings
  // — bold, on the tinted ground — while postable accounts stay plain.
  const summary = !account.is_postable
  const side    = account.normal_balance === 'debit' ? 'Debit' : 'Credit'

  return (
    <>
    <Row needsAction={summary} done={summary} selected={open} onClick={summary ? undefined : onToggle}>
      <code className="w-[108px] shrink-0 tabular text-sm">{account.code}</code>

      <span className="min-w-0 flex-1 truncate">{account.name}</span>

      {/* Only facts that change what a clerk can do with the account. Contra
          and cash change how it behaves; a "summary" chip would repeat what
          the row's own weight already says. */}
      {account.is_contra && <Note>contra</Note>}
      {account.is_cash && <Note>cash</Note>}
      {account.requires_subsidiary && <Note>subsidiary</Note>}
      {account.expense_class && <Note>{account.expense_class}</Note>}

      <span className="w-[24px] shrink-0 text-right text-xs text-ink-muted" title={`Normal balance: ${side}`}>
        {account.normal_balance === 'debit' ? 'Dr' : 'Cr'}
      </span>
    </Row>

    {open && (
      <div className="border-b border-line bg-surface px-4 py-3 pl-[124px]">
        <dl className="grid max-w-[78ch] grid-cols-[max-content_1fr] gap-x-4 gap-y-1.5 text-sm">
          <dt className="text-ink-muted">Account title</dt>
          <dd>{account.name}</dd>
          <dt className="text-ink-muted">Account number</dt>
          <dd className="tabular">{account.code}</dd>
          <dt className="text-ink-muted">Normal balance</dt>
          <dd>{side}</dd>
          <dt className="text-ink-muted">Description</dt>
          <dd className="whitespace-pre-line text-ink-secondary">
            {account.description ?? 'No description for this account.'}
          </dd>
        </dl>
      </div>
    )}
    </>
  )
}

/** A quiet aside on a row. Lower-case and unboxed, so it never competes. */
function Note({ children }: { children: React.ReactNode }) {
  return <span className="shrink-0 text-xs text-ink-muted">{children}</span>
}
