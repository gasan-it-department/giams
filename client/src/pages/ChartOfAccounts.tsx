import { useMemo, useState } from 'react'
import { Coins, Landmark, Receipt, Scale, Wallet } from 'lucide-react'
import { EmptyList, ListPage, Row, type ListTab } from '@/components/ListPage'
import { ErrorNotice } from '@/components/ErrorNotice'
import { PAGE_SIZE, useAccounts } from '@/queries/accounts'
import {
  ACCOUNT_TYPE_LABELS, type Account, type AccountType,
} from '@/schemas/accounts'

/**
 * The chart of accounts.
 *
 * A register of 263 accounts, so density is the whole game: the question a
 * clerk arrives with is "what is the code for X", and the answer should be
 * one glance and one keystroke away rather than a scroll.
 *
 * Account groups are tabs rather than headings, which keeps every row at the
 * same indentation and lets the list run unbroken.
 */

const TABS: ListTab[] = [
  { key: 'all',       label: 'All',         icon: Landmark },
  { key: 'asset',     label: 'Assets',      icon: Wallet },
  { key: 'liability', label: 'Liabilities', icon: Scale },
  { key: 'income',    label: 'Income',      icon: Coins },
  { key: 'expense',   label: 'Expenses',    icon: Receipt },
]

export function ChartOfAccounts() {
  const [tab, setTab] = useState('all')
  const [page, setPage] = useState(0)

  const filter = useMemo(
    () => ({
      ...(tab === 'all' ? {} : { account_type: tab as AccountType }),
      ...(page > 0 ? { page } : {}),
    }),
    [tab, page],
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
      onTabChange={(key) => { setTab(key); setPage(0) }}
      total={data?.total}
      page={page}
      pageSize={PAGE_SIZE}
      onPageChange={setPage}
      onRefresh={() => void refetch()}
      refreshing={isFetching}
    >
      {error && <div className="p-4"><ErrorNotice error={error} /></div>}

      {isPending && <p className="px-4 py-6 text-ink-muted">Loading…</p>}

      {accounts?.length === 0 && (
        <EmptyList
          title="No accounts in this group."
          body="Try another group, or search by code or name from the bar at the top."
        />
      )}

      {accounts?.map((account) => (
        <AccountRow key={account.id} account={account} />
      ))}
    </ListPage>
  )
}

function AccountRow({ account }: { account: Account }) {
  // Summary accounts roll up and cannot be posted to, so they read as headings
  // — bold, on the tinted ground — while postable accounts stay plain.
  const summary = !account.is_postable

  return (
    <Row needsAction={summary} done={summary}>
      <code className="w-[108px] shrink-0 tabular text-sm">{account.code}</code>

      <span className="min-w-0 flex-1 truncate">{account.name}</span>

      {/* Only facts that change what a clerk can do with the account. Contra
          and cash change how it behaves; a "summary" chip would repeat what
          the row's own weight already says. */}
      {account.is_contra && <Note>contra</Note>}
      {account.is_cash && <Note>cash</Note>}
      {account.requires_subsidiary && <Note>subsidiary</Note>}
      {account.expense_class && <Note>{account.expense_class}</Note>}

      <span className="w-[24px] shrink-0 text-right text-xs text-ink-muted">
        {account.normal_balance === 'debit' ? 'Dr' : 'Cr'}
      </span>
    </Row>
  )
}

/** A quiet aside on a row. Lower-case and unboxed, so it never competes. */
function Note({ children }: { children: React.ReactNode }) {
  return <span className="shrink-0 text-xs text-ink-muted">{children}</span>
}
