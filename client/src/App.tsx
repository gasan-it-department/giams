import { useEffect } from 'react'
import { Navigate, Route, Routes } from 'react-router-dom'
import { Loader2 } from 'lucide-react'
import { AppShell } from '@/components/AppShell'
import { Books } from '@/pages/Books'
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
    return <Login />
  }

  // In front of the application, not beside it. Until the password an
  // administrator issued has been replaced, the person signed in is not
  // provably the person the account belongs to — and every audit record
  // attributed to them is weaker for it.
  if (user.must_change_password) {
    return <ChangePassword />
  }

  return (
    <AppShell>
      <Routes>
        <Route path="/overview" element={<Dashboard />} />
        <Route path="/accounts" element={<ChartOfAccounts />} />
        <Route path="/system" element={<SystemStatus />} />
        <Route path="/vouchers" element={<Vouchers />} />
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
