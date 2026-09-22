import { LicenceBanner } from '@/components/LicenceBanner'
import { useEffect, useRef, type ReactNode } from 'react'
import { NavLink } from 'react-router-dom'
import {
  BarChart3, BookOpen, Cloud, CloudOff, FileText, LayoutDashboard, ListTree, LogOut, Menu, Receipt, Scale, Search, Settings, UploadCloud,
} from 'lucide-react'
import { PeriodPicker } from '@/components/PeriodPicker'
import { useNodeHealth } from '@/queries/health'
import { useAuthStore } from '@/stores/authStore'
import { useDraftStore } from '@/stores/draftStore'

/**
 * The application frame.
 *
 * Gmail's layout, because it is the one this office already knows: a rail of
 * pill-shaped destinations on the left, a wide search field across the top,
 * and the working area as a rounded white panel floating on a faint canvas.
 *
 * Hierarchy comes from the gap between `canvas` and `surface` and from font
 * weight. There is no shadow anywhere — the utilities do not exist, see
 * `index.css`.
 */

interface NavItem {
  label: string
  to: string
  icon: typeof Receipt
  /** Permission required to see the destination at all. */
  permission?: string
}

const NAV: NavItem[] = [
  { label: 'Overview',    to: '/overview',    icon: LayoutDashboard, permission: 'report.view' },
  { label: 'Accounts',    to: '/accounts',    icon: ListTree,  permission: 'account.view' },
  { label: 'Vouchers',    to: '/vouchers',    icon: Receipt,   permission: 'dv.draft' },
  { label: 'Obligations', to: '/obligations', icon: FileText,  permission: 'budget.view' },
  { label: 'Books',       to: '/books',       icon: BookOpen,  permission: 'report.view' },
  { label: 'Reconcile',   to: '/reconcile',   icon: Scale,     permission: 'reconcile.perform' },
  { label: 'Statements',  to: '/statements',  icon: BarChart3, permission: 'report.view' },
  { label: 'System',      to: '/system',      icon: Settings },
]

export function AppShell({ children }: { children: ReactNode }) {
  // Fixed viewport height with the list scrolling inside it, as Gmail does.
  // Scrolling the whole page would take the toolbar and tabs off screen, and
  // those are the controls a clerk reaches for most.
  return (
    <div className="flex h-screen flex-col overflow-hidden bg-canvas text-ink">
      <TopBar />
      {/* Above the working area rather than inside it, so it is present on
          every screen and scrolls away on none of them. */}
      <LicenceBanner />
      <div className="flex min-h-0 flex-1 gap-2 px-3 pb-3">
        <Sidebar />
        <main className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden rounded-lg bg-surface">
          {children}
        </main>
      </div>
    </div>
  )
}

function TopBar() {
  return (
    <header className="no-print flex items-center gap-4 px-4 py-3">
      <button
        type="button"
        aria-label="Toggle navigation"
        className="rounded-pill p-2 text-ink-secondary hover:bg-hover"
      >
        <Menu className="size-5" aria-hidden />
      </button>

      <span className="text-lg font-medium tracking-tight">GIAMS</span>

      <div className="flex min-w-0 max-w-[420px] flex-1 items-center gap-3 rounded-pill bg-search px-4 py-2">
        <Search className="size-[18px] shrink-0 text-ink-secondary" aria-hidden />
        <input
          type="search"
          placeholder="Search vouchers, payees, accounts"
          aria-label="Search"
          className="min-w-0 flex-1 bg-transparent text-ink placeholder:text-ink-muted focus:outline-none"
        />
      </div>

      <PeriodPicker />
      <ConnectionBadge />
      <UserMenu />
    </header>
  )
}

function UserMenu() {
  const user = useAuthStore((s) => s.user)
  const signOut = useAuthStore((s) => s.signOut)

  if (!user) return null

  const initials = user.full_name
    .split(/\s+/)
    .filter(Boolean)
    .slice(0, 2)
    .map((part) => part[0]?.toUpperCase() ?? '')
    .join('')

  return (
    <div className="flex items-center gap-2">
      <span
        title={user.full_name}
        className="flex size-8 shrink-0 items-center justify-center rounded-pill bg-accent-soft text-xs text-accent-soft-ink"
      >
        {initials}
      </span>
      <button
        type="button"
        onClick={() => void signOut()}
        aria-label="Sign out"
        title="Sign out"
        className="rounded-pill p-2 text-ink-secondary hover:bg-hover"
      >
        <LogOut className="size-[18px]" aria-hidden />
      </button>
    </div>
  )
}

/**
 * Whether this workstation is talking to the office node, and what is waiting.
 *
 * Phrased as a state of the work rather than of the network — "Working
 * offline" tells a clerk what is happening to their entries; "Disconnected"
 * only tells them something is broken.
 *
 * This is also where waiting work gets sent. When the connection returns, the
 * drafts flush on their own: a clerk should never have to know there is a
 * queue, let alone press something to empty it.
 */
function ConnectionBadge() {
  const { data, isError, isPending } = useNodeHealth()
  const { pending, flushing, refresh, flush } = useDraftStore()

  const connected = !isPending && !isError && data !== undefined
  const wasConnected = useRef(connected)

  useEffect(() => {
    void refresh()
  }, [refresh])

  useEffect(() => {
    // Only on the transition into being connected. Firing on every poll would
    // re-send while a flush is already running.
    if (connected && !wasConnected.current) {
      void flush()
    }
    wasConnected.current = connected
  }, [connected, flush])

  if (isPending) {
    return <span className="text-sm text-ink-muted">Connecting…</span>
  }

  if (!connected) {
    return (
      <span
        className="flex items-center gap-2 rounded-pill bg-warn-fill px-3 py-1.5 text-sm text-warn-ink"
        title="Your work is being saved on this computer and will be sent when the connection returns."
      >
        <CloudOff className="size-4" aria-hidden />
        {pending > 0
          ? `Working offline — ${pending} waiting`
          : 'Working offline'}
      </span>
    )
  }

  if (flushing || pending > 0) {
    return (
      <span className="flex items-center gap-2 rounded-pill bg-info-fill px-3 py-1.5 text-sm text-info-ink">
        <UploadCloud className={`size-4 ${flushing ? 'animate-pulse' : ''}`} aria-hidden />
        {flushing ? 'Sending your work…' : `${pending} waiting to send`}
      </span>
    )
  }

  return (
    <span
      className="flex items-center gap-2 rounded-pill bg-ok-fill px-3 py-1.5 text-sm text-ok-ink"
      title={`Connected to ${data.node_code}`}
    >
      <Cloud className="size-4" aria-hidden />
      Connected
    </span>
  )
}

/**
 * Destinations the signed-in user can actually reach.
 *
 * Hiding a link the user has no permission for is a courtesy, not a control —
 * the server refuses the request regardless. Showing links that always fail
 * would just teach people to ignore errors.
 */
function Sidebar() {
  const can = useAuthStore((s) => s.can)
  const visible = NAV.filter((item) => !item.permission || can(item.permission))

  return (
    <nav aria-label="Main" className="no-print w-[196px] shrink-0 overflow-y-auto pr-1 pt-2">
      <ul>
        {visible.map(({ label, to, icon: Icon }) => (
          <li key={to}>
            <NavLink
              to={to}
              className={({ isActive }) =>
                // Tighter than a comfortable admin nav: Gmail's rail items are
                // about 32px, which keeps every destination above the fold.
                `flex items-center gap-4 rounded-r-pill py-1.5 pl-5 pr-3 text-sm ${
                  isActive
                    ? 'bg-accent-soft font-medium text-accent-soft-ink'
                    : 'text-ink-secondary hover:bg-hover'
                }`
              }
            >
              <Icon className="size-[18px] shrink-0" aria-hidden />
              <span className="flex-1 truncate">{label}</span>
            </NavLink>
          </li>
        ))}
      </ul>
    </nav>
  )
}
