import { useState } from 'react'
import { KeyRound, Lock, LogOut, RefreshCw } from 'lucide-react'
import { ErrorNotice } from '@/components/ErrorNotice'
import { OneTimePasswords } from '@/components/OneTimePasswords'
import { useCheckRenewal, useInstallLicence, type LicenceStatus } from '@/queries/licence'
import { useAuthStore } from '@/stores/authStore'

/**
 * What the accounting system shows once the subscription has ended and the
 * grace days have run out.
 *
 * In place of the whole application, not over it: a locked system that still
 * showed its menus would invite clicks that can only fail. It says plainly
 * that nothing has been deleted — the question everyone asks first — and
 * offers the one thing that fixes it: collecting the renewal.
 */
export function LockedScreen({ status }: { status: LicenceStatus }) {
  const user = useAuthStore((s) => s.user)
  const can = useAuthStore((s) => s.can)
  const signOut = useAuthStore((s) => s.signOut)

  const check = useCheckRenewal()
  const install = useInstallLicence()
  const [key, setKey] = useState('')

  const ended = status.expires_at
    ? new Date(status.expires_at).toLocaleDateString('en-PH', { dateStyle: 'long' })
    : null
  const locked = status.locks_at
    ? new Date(status.locks_at).toLocaleDateString('en-PH', { dateStyle: 'long' })
    : null

  const created = (install.data?.provisioned ?? []).filter((p) => p.status === 'created' && p.password)

  return (
    <main className="flex min-h-screen items-center justify-center bg-canvas px-4 py-10">
      <div className="w-full max-w-[520px]">
        <div className="mb-6 flex flex-col items-center text-center">
          <div className="mb-4 flex size-14 items-center justify-center rounded-lg bg-danger-fill text-danger-ink">
            <Lock className="size-7" aria-hidden />
          </div>
          <h1 className="text-xl font-medium">This system is locked</h1>
          <p className="mt-2 text-ink-secondary">{status.notice}</p>
        </div>

        <section className="rounded-lg bg-surface p-6">
          <dl className="grid grid-cols-[auto_1fr] gap-x-6 gap-y-1.5 text-sm">
            {ended && (<><dt className="text-ink-muted">Subscription ended</dt><dd>{ended}</dd></>)}
            {locked && (<><dt className="text-ink-muted">Locked since</dt><dd>{locked}</dd></>)}
            <dt className="text-ink-muted">Your records</dt>
            <dd>All kept. Nothing has been deleted.</dd>
          </dl>

          <p className="mt-4 text-sm text-ink-secondary">
            Once the subscription is renewed, choose <span className="font-medium text-ink">Check for renewal</span>.
            The system unlocks at once — no new key has to be entered.
          </p>

          <button
            type="button"
            onClick={() => check.mutate()}
            disabled={check.isPending}
            className="mt-4 flex w-full items-center justify-center gap-2 rounded-pill bg-accent-fill px-4 py-2.5 font-medium text-accent-ink hover:brightness-[0.97] disabled:opacity-60"
          >
            <RefreshCw className={`size-4 ${check.isPending ? 'animate-spin' : ''}`} aria-hidden />
            {check.isPending ? 'Checking…' : 'Check for renewal'}
          </button>

          {check.data && check.data.outcome !== 'renewed' && (
            <p className="mt-3 rounded-md bg-sunken px-3 py-2 text-sm text-ink-secondary">{check.data.message}</p>
          )}
          {check.error && <div className="mt-3"><ErrorNotice error={check.error} /></div>}

          {can('tenant.manage') && (
            <details className="mt-5 border-t border-line pt-4 text-sm">
              <summary className="cursor-pointer text-ink-secondary">Install a licence key instead</summary>
              <form
                className="mt-3 flex items-start gap-2"
                onSubmit={(e) => {
                  e.preventDefault()
                  install.mutate(key.trim(), { onSuccess: () => setKey('') })
                }}
              >
                <span className="relative grow">
                  <KeyRound className="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-ink-muted" aria-hidden />
                  <input
                    value={key}
                    onChange={(e) => setKey(e.target.value)}
                    placeholder="Paste a licence key"
                    aria-label="Licence key"
                    className="w-full rounded-pill border border-line bg-canvas py-2 pl-9 pr-3 text-sm outline-none focus:border-accent-solid"
                  />
                </span>
                <button
                  type="submit"
                  disabled={key.trim() === '' || install.isPending}
                  className="shrink-0 rounded-pill bg-sunken px-4 py-2 text-sm font-medium hover:bg-hover disabled:opacity-60"
                >
                  {install.isPending ? 'Checking…' : 'Install'}
                </button>
              </form>
              {install.error && <div className="mt-3"><ErrorNotice error={install.error} /></div>}
              {created.length > 0 && (
                <div className="mt-3">
                  <OneTimePasswords
                    title="Accounts created from this key"
                    people={created.map((p) => ({ full_name: p.full_name, username: p.username, password: p.password ?? '' }))}
                  />
                </div>
              )}
            </details>
          )}
        </section>

        <div className="mt-5 flex items-center justify-center gap-4 text-sm text-ink-muted">
          <span>Signed in as {user?.full_name}</span>
          <button
            type="button"
            onClick={() => void signOut()}
            className="flex items-center gap-1.5 rounded-pill px-3 py-1.5 hover:bg-hover"
          >
            <LogOut className="size-4" aria-hidden />
            Sign out
          </button>
        </div>
      </div>
    </main>
  )
}
