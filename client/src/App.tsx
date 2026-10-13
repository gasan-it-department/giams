import { useEffect } from 'react'
import { Navigate, Route, Routes, useLocation } from 'react-router-dom'
import { Loader2 } from 'lucide-react'
import { AppShell } from '@/components/AppShell'
import { LockedScreen } from '@/components/LockedScreen'
import { useLicence } from '@/queries/licence'
import { AdminApp } from '@/pages/admin/AdminApp'
import { Books } from '@/pages/Books'
import { Budget } from '@/pages/Budget'
import { ChangePassword } from '@/pages/ChangePassword'
import { ChartOfAccounts } from '@/pages/ChartOfAccounts'
import { Dashboard } from '@/pages/Dashboard'
import { Login } from '@/pages/Login'
import { NewJournalEntry } from '@/pages/NewJournalEntry'
import { Obligations } from '@/pages/Obligations'
import { Reconcile } from '@/pages/Reconcile'
import { Statements } from '@/pages/Statements'
import { SystemStatus } from '@/pages/SystemStatus'
import { Vouchers } from '@/pages/Vouchers'
import { useAuthStore } from '@/stores/authStore'

/**
 * The authentication gate and top-level routes.
 *
 * Nothing behind the gate renders until a session is known, so no screen ever
 * flashes signed-in content before being sent back to the sign-in page.
 *
 * The gate is a convenience, not a control: every route on the server checks
 * permissions independently, and a client that skipped this would simply
 * receive errors.
 */
export default function App() {
  const { user, initialising, restore } = useAuthStore()

  // The administration console is its own application at /admin: its own
  // sign-in page and its own frame, never inside the accounting system's.
  const { pathname } = useLocation()
  const inAdmin = pathname === '/admin' || pathname.startsWith('/admin/')

  // Read before any early return, as hooks must be; only fetched once someone
  // is signed in to the accounting system.
  const { data: licence } = useLicence(!!user && !user.must_change_password && !inAdmin)

  useEffect(() => {
    void restore()
  }, [restore])

  if (initialising) {
    return (
      <div className="flex min-h-screen items-center justify-center bg-canvas">
        <span className="flex items-center gap-3 text-ink-secondary">
          <Loader2 className="size-5 animate-spin" aria-hidden />
          Starting…
        </span>
      </div>
    )
  }

  if (!user) {
    return <Login variant={inAdmin ? 'admin' : 'accounting'} />
  }

  // In front of the application, not beside it. Until the password an
  // administrator issued has been replaced, the person signed in is not
  // provably the person the account belongs to — and every audit record
  // attributed to them is weaker for it.
  if (user.must_change_password) {
    return <ChangePassword />
  }

  if (inAdmin) {
    return <AdminApp />
  }

  // Past the grace days, the accounting system gives way to the lock screen.
  // The server refuses its requests anyway; this is so nobody is left
  // clicking menus that can only fail.
  if (licence?.state === 'locked') {
    return <LockedScreen status={licence} />
  }

  return (
    <AppShell>
      <Routes>
        <Route path="/overview" element={<Dashboard />} />
        <Route path="/accounts" element={<ChartOfAccounts />} />
        <Route path="/system" element={<SystemStatus />} />
        <Route path="/vouchers" element={<Vouchers />} />
        <Route path="/budget" element={<Budget />} />
        <Route path="/obligations" element={<Obligations />} />
        <Route path="/books" element={<Books />} />
        <Route path="/books/new" element={<NewJournalEntry />} />
        <Route path="/reconcile" element={<Reconcile />} />
        <Route path="/statements" element={<Statements />} />
        <Route path="*" element={<Navigate to="/overview" replace />} />
      </Routes>
    </AppShell>
  )
}
