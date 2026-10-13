import { useEffect, useLayoutEffect, useMemo, useRef, useState, type KeyboardEvent } from 'react'
import { createPortal } from 'react-dom'
import { ChevronDown, Search } from 'lucide-react'
import { useAccountOptions, type AccountOption } from '@/queries/accounts'
import type { AccountType } from '@/schemas/accounts'

/**
 * Chooses an account from the chart.
 *
 * A dropdown that opens on the whole chart, grouped by type, with a search box
 * at the top that narrows it as you type. The picker it replaces showed
 * nothing until two letters were typed, so a clerk who did not already know
 * an account's exact name had nothing to look through.
 *
 * Search matches the name, the code with or without its dashes (`50203010`
 * finds 5-02-03-010), and the summary account above it — typing "cash" also
 * lists everything filed under Cash.
 *
 * The list is rendered in a portal at a fixed position, so it is never cut
 * off by the scrolling table it sits in, and it opens upward when there is no
 * room below.
 */

const TYPE_LABELS: Record<AccountType, string> = {
  asset:     'Assets',
  liability: 'Liabilities',
  equity:    'Equity',
  income:    'Income',
  expense:   'Expenses',
}

const PANEL_WIDTH = 420
const PANEL_MAX_LIST = 320

interface Placement {
  top:       number
  left:      number
  width:     number
  listMax:   number
  openAbove: boolean
}

export function AccountSelect({
  value,
  onChange,
  accountType,
  placeholder = 'Choose an account',
  invalid = false,
}: {
  value: string
  onChange: (account: AccountOption) => void
  /** Limits the chart to one type, e.g. `expense` for an appropriation. */
  accountType?: AccountType
  placeholder?: string
  invalid?: boolean
}) {
  const { data: options, isPending, error } = useAccountOptions(accountType)

  const [open, setOpen] = useState(false)
  const [query, setQuery] = useState('')
  const [active, setActive] = useState(0)
  const [placement, setPlacement] = useState<Placement | null>(null)

  const triggerRef = useRef<HTMLButtonElement>(null)
  const panelRef = useRef<HTMLDivElement>(null)
  const listRef = useRef<HTMLDivElement>(null)

  const selected = options?.find((o) => o.id === value)

  const matches = useMemo(() => filterAccounts(options ?? [], query), [options, query])

  // ── Placement ──────────────────────────────────────────────────────────────
  useLayoutEffect(() => {
    if (!open) return

    function place() {
      const r = triggerRef.current?.getBoundingClientRect()
      if (!r) return
      const below = window.innerHeight - r.bottom
      const openAbove = below < 280 && r.top > below
      const room = (openAbove ? r.top : below) - 72
      const width = Math.max(r.width, PANEL_WIDTH)

      setPlacement({
        top:       openAbove ? r.top - 4 : r.bottom + 4,
        left:      Math.max(8, Math.min(r.left, window.innerWidth - width - 8)),
        width,
        listMax:   Math.max(160, Math.min(PANEL_MAX_LIST, room)),
        openAbove,
      })
    }

    place()
    // Capture, so scrolling the table the picker sits in keeps the list with it.
    window.addEventListener('scroll', place, true)
    window.addEventListener('resize', place)
    return () => {
      window.removeEventListener('scroll', place, true)
      window.removeEventListener('resize', place)
    }
  }, [open])

  // ── Opening and closing ────────────────────────────────────────────────────
  useEffect(() => {
    if (!open) return

    function onPointerDown(e: PointerEvent) {
      const target = e.target as Node
      if (panelRef.current?.contains(target) || triggerRef.current?.contains(target)) return
      setOpen(false)
    }
    document.addEventListener('pointerdown', onPointerDown)
    return () => document.removeEventListener('pointerdown', onPointerDown)
  }, [open])

  // Keeps the highlighted row in view while arrowing through the list.
  useEffect(() => {
    if (!open) return
    listRef.current
      ?.querySelector(`[data-index="${active}"]`)
      ?.scrollIntoView({ block: 'nearest' })
  }, [active, open])

  function openWith(initialQuery = '') {
    setQuery(initialQuery)
    const start = initialQuery ? 0 : Math.max(0, (options ?? []).findIndex((o) => o.id === value))
    setActive(start)
    setOpen(true)
  }

  function close(returnFocus: boolean) {
    setOpen(false)
    if (returnFocus) triggerRef.current?.focus()
  }

  function choose(account: AccountOption) {
    onChange(account)
    close(true)
  }

  function onSearchKey(e: KeyboardEvent<HTMLInputElement>) {
    if (e.key === 'ArrowDown') {
      e.preventDefault()
      setActive((i) => Math.min(i + 1, matches.length - 1))
    } else if (e.key === 'ArrowUp') {
      e.preventDefault()
      setActive((i) => Math.max(i - 1, 0))
    } else if (e.key === 'Enter') {
      e.preventDefault()
      const pick = matches[active]
      if (pick) choose(pick)
    } else if (e.key === 'Escape') {
      e.preventDefault()
      close(true)
    } else if (e.key === 'Tab') {
      close(false)
    }
  }

  // Typing on the closed picker opens it already searching for that letter,
  // the way a native select jumps to what is typed.
  function onTriggerKey(e: KeyboardEvent<HTMLButtonElement>) {
    if (e.key === 'ArrowDown') {
      e.preventDefault()
      openWith()
    } else if (e.key.length === 1 && !e.ctrlKey && !e.metaKey && !e.altKey && e.key !== ' ') {
      e.preventDefault()
      openWith(e.key)
    }
  }

  return (
    <>
      <button
        ref={triggerRef}
        type="button"
        onClick={() => (open ? close(false) : openWith())}
        onKeyDown={onTriggerKey}
        aria-haspopup="listbox"
        aria-expanded={open}
        className={`flex w-full items-center gap-2 rounded-sm border bg-canvas px-2 py-1 text-left hover:bg-hover ${
          invalid ? 'border-danger-ink' : open ? 'border-accent-solid' : 'border-line'
        }`}
      >
        {selected ? (
          <span className="flex min-w-0 flex-1 items-baseline gap-2">
            <code className="shrink-0 tabular text-xs text-ink-secondary">{selected.code}</code>
            <span className="truncate">{selected.name}</span>
          </span>
        ) : (
          <span className="flex-1 truncate text-ink-muted">{placeholder}</span>
        )}
        <ChevronDown
          className={`size-4 shrink-0 text-ink-muted transition-transform ${open ? 'rotate-180' : ''}`}
          aria-hidden
        />
      </button>

      {open && placement && createPortal(
        <div
          ref={panelRef}
          role="dialog"
          aria-label="Choose an account"
          style={{
            position: 'fixed',
            left:     placement.left,
            width:    placement.width,
            ...(placement.openAbove
              ? { bottom: window.innerHeight - placement.top }
              : { top: placement.top }),
          }}
          className="z-50 flex flex-col rounded-md border border-line-strong bg-surface"
        >
          <div className="flex items-center gap-2 border-b border-line px-3 py-2">
            <Search className="size-4 shrink-0 text-ink-muted" aria-hidden />
            {/* Focused as it mounts. The panel appears a render after `open`
                (it waits to be measured), so focusing from an effect on
                `open` found no input to focus. */}
            <input
              autoFocus
              value={query}
              onChange={(e) => { setQuery(e.target.value); setActive(0) }}
              onKeyDown={onSearchKey}
              placeholder="Search by name or code"
              aria-label="Search accounts"
              aria-controls="account-options"
              className="min-w-0 flex-1 bg-transparent text-sm outline-none placeholder:text-ink-muted"
            />
          </div>

          <div
            ref={listRef}
            id="account-options"
            role="listbox"
            style={{ maxHeight: placement.listMax }}
            className="overflow-y-auto py-1"
          >
            {isPending && <p className="px-3 py-2 text-sm text-ink-muted">Loading the chart of accounts…</p>}
            {error && <p className="px-3 py-2 text-sm text-danger-ink">The chart of accounts could not be loaded.</p>}

            {options && matches.length === 0 && (
              <p className="px-3 py-3 text-sm text-ink-secondary">
                No account matches “{query.trim()}”.
                <span className="mt-0.5 block text-xs text-ink-muted">
                  Summary accounts cannot be posted to, so they are not listed.
                </span>
              </p>
            )}

            {groupByType(matches).map((group) => (
              <section key={group.type}>
                {/* Sticks while its own accounts scroll past, so the reader
                    always knows which part of the chart they are in. */}
                <p className="sticky top-0 bg-surface px-3 pb-1 pt-2 text-xs font-medium text-ink-muted">
                  {TYPE_LABELS[group.type] ?? group.type}
                </p>
                {group.rows.map(({ account, index }) => (
                  <button
                    key={account.id}
                    type="button"
                    role="option"
                    aria-selected={account.id === value}
                    data-index={index}
                    onMouseEnter={() => setActive(index)}
                    onClick={() => choose(account)}
                    className={`flex w-full items-baseline gap-3 px-3 py-1.5 text-left text-sm ${
                      index === active ? 'bg-accent-soft' : ''
                    } ${account.id === value ? 'font-medium' : ''}`}
                  >
                    <code className="w-[92px] shrink-0 tabular text-xs text-ink-secondary">{account.code}</code>
                    <span className="min-w-0 flex-1">
                      <span className="block truncate">{account.name}</span>
                      {/* The group above it, unless that is just the heading
                          this row already sits under. */}
                      {account.parent_name
                        && account.parent_name.toLowerCase() !== TYPE_LABELS[group.type]?.toLowerCase() && (
                        <span className="block truncate text-xs text-ink-muted">{account.parent_name}</span>
                      )}
                    </span>
                  </button>
                ))}
              </section>
            ))}
          </div>

          {options && (
            <p className="border-t border-line px-3 py-1.5 text-xs text-ink-muted">
              {query.trim()
                ? `${matches.length} of ${options.length} accounts`
                : `${options.length} accounts · ↑ ↓ to move, Enter to choose`}
            </p>
          )}
        </div>,
        document.body,
      )}
    </>
  )
}

/**
 * Splits the list into runs of one account type, keeping each row's position
 * in the flat list — the arrow keys move through that, across groups.
 * The chart arrives ordered by code, and the first digit of a code is its
 * type, so each type is already one contiguous run.
 */
function groupByType(accounts: AccountOption[]) {
  const groups: { type: AccountType; rows: { account: AccountOption; index: number }[] }[] = []
  accounts.forEach((account, index) => {
    const last = groups.at(-1)
    if (last && last.type === account.account_type) last.rows.push({ account, index })
    else groups.push({ type: account.account_type, rows: [{ account, index }] })
  })
  return groups
}

/** Narrows the chart to what was typed. Empty text keeps everything. */
export function filterAccounts(options: AccountOption[], query: string): AccountOption[] {
  const q = query.trim().toLowerCase()
  if (!q) return options

  const digits = q.replace(/\D/g, '')

  return options.filter((o) =>
    o.name.toLowerCase().includes(q)
    || o.code.includes(q)
    || (digits.length >= 2 && o.code.replace(/-/g, '').includes(digits))
    || (o.parent_name?.toLowerCase().includes(q) ?? false),
  )
}
