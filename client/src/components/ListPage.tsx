import type { ReactNode } from 'react'
import { ChevronLeft, ChevronRight, MoreVertical, RefreshCw, Square } from 'lucide-react'
import { SearchField } from '@/components/SearchField'

/**
 * The chrome every register sits in.
 *
 * Modelled directly on Gmail's list, because that is the layout this office
 * already knows. What that means in practice, and what earlier versions of
 * these screens got wrong:
 *
 *   * **No page title and no description paragraph.** Gmail never tells you
 *     you are looking at your inbox. The navigation on the left already said
 *     so, and a heading plus a sentence of explanation pushes the first row
 *     a hundred pixels down the screen for nothing.
 *   * **Search sits in the toolbar, per register.** A single box in the top
 *     bar promised to search everything and searched nothing; each register
 *     now searches what it lists.
 *   * **The count sits on the right of the toolbar**, as "1–50 of 263", not as
 *     a sentence.
 *   * **Tabs carry the filters**, with an underline on the active one.
 *
 * Density is the other half of it. Gmail fits roughly twice as many rows on a
 * screen as a comfortable admin table does, and on a 1366×768 municipal
 * monitor that is the difference between seeing a day's work and scrolling.
 */

export interface ListTab {
  key: string
  label: string
  /** Shown as a count beside the label; a tinted pill when `attention`. */
  count?: number
  /** Marks work waiting on someone — the count reads as amber. */
  attention?: boolean
  icon?: typeof Square
}

interface ListPageProps {
  tabs?: ListTab[]
  activeTab?: string
  onTabChange?: (key: string) => void
  /** Total behind the list, rendered as "1–20 of 263". */
  total?: number
  /** Zero-based page, and how many fit on one. */
  page?: number
  pageSize?: number
  onPageChange?: (page: number) => void
  onRefresh?: () => void
  refreshing?: boolean
  /** Actions that belong beside the toolbar, e.g. a filter chip row. */
  toolbar?: ReactNode
  /** This register's search box. */
  search?: { value: string; onChange: (value: string) => void; placeholder: string }
  children: ReactNode
}

export function ListPage({
  tabs, activeTab, onTabChange, total, page = 0, pageSize = 20, onPageChange,
  onRefresh, refreshing, toolbar, search, children,
}: ListPageProps) {
  const first = total === 0 ? 0 : page * pageSize + 1
  const last = Math.min((page + 1) * pageSize, total ?? 0)
  const hasPrevious = page > 0
  const hasNext = total !== undefined && last < total
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="no-print flex items-center gap-1 px-2 py-1.5 text-ink-secondary">
        <button
          type="button"
          aria-label="Select all"
          className="rounded-pill p-2 hover:bg-hover"
        >
          <Square className="size-[18px]" aria-hidden />
        </button>

        <button
          type="button"
          onClick={onRefresh}
          aria-label="Refresh"
          className="rounded-pill p-2 hover:bg-hover disabled:opacity-50"
          disabled={refreshing}
        >
          <RefreshCw className={`size-[18px] ${refreshing ? 'animate-spin' : ''}`} aria-hidden />
        </button>

        <button type="button" aria-label="More" className="rounded-pill p-2 hover:bg-hover">
          <MoreVertical className="size-[18px]" aria-hidden />
        </button>

        {search && (
          <SearchField
            value={search.value}
            onChange={search.onChange}
            placeholder={search.placeholder}
            className="ml-1 w-[320px] min-w-[180px] shrink"
          />
        )}

        {toolbar}

        {total !== undefined && (
          <div className="ml-auto flex shrink-0 items-center gap-1">
            <span className="whitespace-nowrap text-xs tabular">
              {total === 0 ? 'None' : `${first}–${last} of ${total}`}
            </span>
            <button
              type="button"
              aria-label="Previous page"
              onClick={() => onPageChange?.(page - 1)}
              disabled={!hasPrevious}
              className="rounded-pill p-1.5 hover:bg-hover disabled:opacity-30 disabled:hover:bg-transparent"
            >
              <ChevronLeft className="size-[18px]" aria-hidden />
            </button>
            <button
              type="button"
              aria-label="Next page"
              onClick={() => onPageChange?.(page + 1)}
              disabled={!hasNext}
              className="rounded-pill p-1.5 hover:bg-hover disabled:opacity-30 disabled:hover:bg-transparent"
            >
              <ChevronRight className="size-[18px]" aria-hidden />
            </button>
          </div>
        )}
      </div>

      {tabs && tabs.length > 0 && (
        <div role="tablist" className="no-print flex px-2">
          {tabs.map((tab) => {
            const active = tab.key === activeTab
            const Icon = tab.icon

            return (
              <button
                key={tab.key}
                role="tab"
                aria-selected={active}
                onClick={() => onTabChange?.(tab.key)}
                className={`flex min-w-0 flex-1 items-center gap-2.5 border-b-[3px] px-4 py-2.5 text-left ${
                  active
                    ? 'border-accent-solid font-medium text-accent-solid'
                    : 'border-transparent text-ink-secondary hover:bg-hover'
                }`}
              >
                {Icon && <Icon className="size-[18px] shrink-0" aria-hidden />}
                <span className="truncate">{tab.label}</span>
                {tab.count !== undefined && tab.count > 0 && (
                  <span
                    className={`shrink-0 rounded-pill px-1.5 text-xs tabular ${
                      tab.attention ? 'bg-warn-fill text-warn-ink' : 'text-ink-muted'
                    }`}
                  >
                    {tab.count}
                  </span>
                )}
              </button>
            )
          })}
        </div>
      )}

      <div className="min-h-0 flex-1 overflow-auto">{children}</div>
    </div>
  )
}

/**
 * A row in a register.
 *
 * Text-forward: Gmail rows are almost entirely words, and every pill added to
 * a row costs some of the scannability that makes a long list usable. Status
 * is carried by weight and background tone; a chip is reserved for the one or
 * two facts that genuinely need calling out.
 */
export function Row({
  needsAction, done, selected, onClick, children,
}: {
  needsAction?: boolean
  done?: boolean
  selected?: boolean
  onClick?: () => void
  children: ReactNode
}) {
  return (
    <div
      onClick={onClick}
      className={`flex min-h-[36px] items-center gap-3 border-b border-line px-4 py-1.5 last:border-b-0 ${
        selected ? 'bg-accent-soft'
        : done    ? 'bg-sunken text-ink-secondary'
        :           'bg-surface'
      } ${needsAction ? 'font-medium' : ''} ${onClick ? 'cursor-pointer hover:bg-hover' : 'hover:bg-hover'}`}
    >
      {children}
    </div>
  )
}

/** The empty state, in Gmail's voice: an invitation, not an apology. */
export function EmptyList({ title, body }: { title: string; body: string }) {
  return (
    <div className="px-4 py-16 text-center">
      <p className="font-medium">{title}</p>
      <p className="mx-auto mt-1 max-w-[52ch] text-ink-secondary">{body}</p>
    </div>
  )
}
