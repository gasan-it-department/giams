import { Link, Navigate, NavLink, Route, Routes } from 'react-router-dom'
import { ArrowUpRight, KeyRound, LogOut, ShieldCheck, Users } from 'lucide-react'
import { useAuthStore } from '@/stores/authStore'
import { Accounts } from './Accounts'
import { LicenceProfile } from './LicenceProfile'
import { Licences } from './Licences'
import { NewLicence } from './NewLicence'

/**
 * The administration console, at /admin.
 *
 * Its own page with its own frame, not a section of the accounting system:
 * no accounting menu, no period picker, no subscription banner. Issuing a
 * licence and managing accounts are the system's administration, and the
 * office's books are not the place for either.
 */
export function AdminApp() {
  const user = useAuthStore((s) => s.user)
  const can = useAuthStore((s) => s.can)
  const signOut = useAuthStore((s) => s.signOut)

  const canIssue = can('licence.issue')
  const canManage = can('user.manage')

  const nav = [
    { to: '/admin/licences', label: 'Licences', icon: KeyRound, show: canIssue },
    { to: '/admin/accounts', label: 'Accounts', icon: Users,    show: canManage },
  ].filter((n) => n.show)

  return (
    <div className="flex h-screen flex-col bg-canvas text-ink">
      <header className="flex items-center gap-4 border-b border-line bg-surface px-5 py-3">
        <span className="flex items-center gap-2.5">
          <span className="flex size-8 items-center justify-center rounded-md bg-accent-fill text-accent-ink">
            <ShieldCheck className="size-[18px]" aria-hidden />
          </span>
          <span>
            <span className="block text-base font-medium leading-tight">GIAMS Administration</span>
            <span className="block text-xs text-ink-muted">Licences and user accounts</span>
          </span>
        </span>

        <span className="ml-auto flex items-center gap-4 text-sm">
          {can('report.view') && (
            <Link to="/overview" className="flex items-center gap-1 text-ink-secondary hover:text-ink">
              Accounting system
              <ArrowUpRight className="size-3.5" aria-hidden />
            </Link>
          )}
          <span className="text-ink-secondary">{user?.full_name}</span>
          <button
            type="button"
            onClick={() => void signOut()}
            className="flex items-center gap-1.5 rounded-pill px-3 py-1.5 hover:bg-hover"
          >
            <LogOut className="size-4" aria-hidden />
            Sign out
          </button>
        </span>
      </header>

      {nav.length === 0 ? (
        <main className="flex flex-1 items-center justify-center p-6">
          <div className="max-w-[420px] text-center">
            <p className="font-medium">This account cannot use the administration console.</p>
            <p className="mt-1 text-sm text-ink-secondary">
              It is for issuing licence keys and managing accounts. Sign in with an administrator account.
            </p>
          </div>
        </main>
      ) : (
        <div className="flex min-h-0 flex-1">
          <nav className="w-[220px] shrink-0 space-y-1 border-r border-line p-3" aria-label="Administration">
            {nav.map(({ to, label, icon: Icon }) => (
              <NavLink
                key={to}
                to={to}
                className={({ isActive }) =>
                  `flex items-center gap-3 rounded-pill px-4 py-2 text-sm ${
                    isActive ? 'bg-accent-soft font-medium text-accent-soft-ink' : 'text-ink-secondary hover:bg-hover'
                  }`
                }
              >
                <Icon className="size-[18px]" aria-hidden />
                {label}
              </NavLink>
            ))}
          </nav>

          <main className="min-h-0 flex-1 overflow-auto">
            <Routes>
              <Route path="/admin" element={<Navigate to={nav[0]!.to} replace />} />
              <Route path="/admin/licences" element={<Licences />} />
              <Route path="/admin/licences/new" element={<NewLicence />} />
              <Route path="/admin/licences/:id" element={<LicenceProfile />} />
              <Route path="/admin/accounts" element={<Accounts />} />
              <Route path="*" element={<Navigate to="/admin" replace />} />
            </Routes>
          </main>
        </div>
      )}
    </div>
  )
}
