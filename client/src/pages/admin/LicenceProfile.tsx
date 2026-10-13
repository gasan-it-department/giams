import { useState } from 'react'
import { Link, useParams, useSearchParams } from 'react-router-dom'
import { ArrowLeft, CalendarClock, Check, KeyRound, RotateCcw, Server } from 'lucide-react'
import { ErrorNotice } from '@/components/ErrorNotice'
import { useLicensee, useRenew, type IssuedKey, type LicenseeDetail } from '@/queries/admin'
import { useInstallLicence } from '@/queries/licence'
import { accessLabel, isLifetime } from '@/schemas/admin'
import { useAuthStore } from '@/stores/authStore'
import {
  AdminPage, CopyButton, NoAccess, OneTimePasswords,
  formatDate, formatDateTime, formatExpiry, inputClass,
} from './shared'

/** What a key grants, in a line: "Annual · 10 workstations · 30 Sep 2026 to 30 Sep 2027". */
function describeKey(k: Pick<IssuedKey, 'plan' | 'seats' | 'not_before' | 'expires_at'>): string {
  const period = isLifetime(k.expires_at)
    ? `from ${formatDate(k.not_before)}, never expires`
    : `${formatDate(k.not_before)} to ${formatExpiry(k.expires_at)}`
  return `${accessLabel(k.plan)} · ${k.seats} workstations · ${period}`
}

/**
 * One licence profile: the municipality, its people, and every key issued.
 */
export function LicenceProfile() {
  const { id = '' } = useParams()
  const [params] = useSearchParams()
  const allowed = useAuthStore((s) => s.can('licence.issue'))

  const { data: profile, error, isPending } = useLicensee(id)

  if (!allowed) {
    return <AdminPage title="Licence profile"><NoAccess /></AdminPage>
  }

  const current = profile?.keys[0]
  const justSaved = params.get('saved') === '1'

  return (
    <AdminPage
      title={profile?.municipality_name ?? 'Licence profile'}
      description={profile
        ? `Profile saved ${formatDateTime(profile.created_at)}${profile.created_by_name ? ` by ${profile.created_by_name}` : ''}`
        : undefined}
    >
      <Link to="/admin/licences" className="mb-4 inline-flex items-center gap-1.5 text-sm text-ink-secondary hover:text-ink">
        <ArrowLeft className="size-4" aria-hidden />
        Licences
      </Link>

      {error && <ErrorNotice error={error} />}
      {isPending && <p className="py-6 text-ink-muted">Loading…</p>}

      {profile && (
        <>
          {justSaved && (
            <p className="mb-4 flex items-start gap-2 rounded-md bg-ok-fill px-3 py-2 text-sm text-ok-ink">
              <Check className="mt-0.5 size-4 shrink-0" aria-hidden />
              {profile.for_this_server
                ? 'Profile saved. The key can be installed on this server below.'
                : 'Profile saved. Send the key to the municipality.'}
            </p>
          )}

          <Subscription profile={profile} />

          {current && <CurrentKey profile={profile} keyIssued={current} />}

          {!profile.for_this_server && <ServerSettings profile={profile} />}

          <People profile={profile} />

          {profile.renewals.length > 0 && <Renewals profile={profile} />}

          {profile.keys.length > 1 && <History keys={profile.keys.slice(1)} />}

          {profile.notes && (
            <section className="mt-5">
              <h3 className="mb-1 text-sm font-medium">Notes</h3>
              <p className="text-sm text-ink-secondary">{profile.notes}</p>
            </section>
          )}
        </>
      )}
    </AdminPage>
  )
}

/** The key worth installing, with Copy — and, for this server, Install. */
function CurrentKey({ profile, keyIssued }: { profile: LicenseeDetail; keyIssued: IssuedKey }) {
  const install = useInstallLicence()
  const [confirming, setConfirming] = useState(false)

  const created = (install.data?.provisioned ?? []).filter((p) => p.status === 'created' && p.password)
  const existing = (install.data?.provisioned ?? []).filter((p) => p.status === 'existing')

  return (
    <section className="mb-5 rounded-md border border-line bg-surface p-4">
      <div className="mb-3 flex items-start justify-between gap-4">
        <div>
          <h3 className="flex items-center gap-2 font-medium">
            <KeyRound className="size-[18px] text-ink-secondary" aria-hidden />
            Licence key
          </h3>
          {/* The key's own first period. Where the subscription stands now —
              renewals, the grace days — is the Subscription panel's to say. */}
          <p className="mt-0.5 text-sm text-ink-secondary">
            {isLifetime(keyIssued.expires_at)
              ? describeKey(keyIssued)
              : `${accessLabel(keyIssued.plan)} · ${keyIssued.seats} workstations · first period ${formatDate(keyIssued.not_before)} to ${formatDate(keyIssued.expires_at)}`}
          </p>
          <p className="text-xs text-ink-muted">Installed once. Renewals reach the office's server without a new key.</p>
        </div>
        <CopyButton text={keyIssued.token} label="Copy licence key" />
      </div>

      <textarea
        readOnly
        value={keyIssued.token}
        onFocus={(e) => e.currentTarget.select()}
        rows={4}
        aria-label="Licence key"
        className="w-full resize-none rounded-sm border border-line bg-sunken px-3 py-2 font-mono text-xs break-all text-ink-secondary"
      />

      <p className="mt-2 text-xs text-ink-muted">
        Issued {formatDateTime(keyIssued.issued_at)}
        {keyIssued.issued_by_name && ` by ${keyIssued.issued_by_name}`}. The municipality pastes it into
        System → Subscription.
      </p>

      {profile.for_this_server && (
        <div className="mt-4 border-t border-line pt-4">
          {!install.isSuccess && (
            confirming ? (
              <div className="flex flex-wrap items-center gap-2">
                <span className="text-sm text-ink-secondary">
                  This replaces the subscription installed here, and creates the accounts below.
                </span>
                <button
                  type="button"
                  disabled={install.isPending}
                  onClick={() => install.mutate(keyIssued.token)}
                  className="rounded-pill bg-accent-fill px-4 py-1.5 text-sm font-medium text-accent-ink disabled:opacity-60"
                >
                  {install.isPending ? 'Installing…' : 'Install'}
                </button>
                <button
                  type="button"
                  onClick={() => setConfirming(false)}
                  className="rounded-pill px-3 py-1.5 text-sm hover:bg-hover"
                >
                  Cancel
                </button>
              </div>
            ) : (
              <button
                type="button"
                onClick={() => setConfirming(true)}
                className="flex items-center gap-2 rounded-pill bg-accent-fill px-4 py-2 text-sm font-medium text-accent-ink hover:brightness-[0.97]"
              >
                <Server className="size-4" aria-hidden />
                Install on this server
              </button>
            )
          )}

          {install.error && <div className="mt-3"><ErrorNotice error={install.error} /></div>}

          {install.isSuccess && (
            <>
              <p className="mb-3 flex items-center gap-2 rounded-md bg-ok-fill px-3 py-2 text-sm text-ok-ink">
                <Check className="size-4" aria-hidden />
                Installed. This server now runs on this key.
              </p>
              {created.length > 0 && (
                <OneTimePasswords
                  title="Accounts created"
                  people={created.map((p) => ({ full_name: p.full_name, username: p.username, password: p.password ?? '' }))}
                />
              )}
              {existing.length > 0 && (
                <p className="text-sm text-ink-secondary">
                  Already had an account, left unchanged: {existing.map((p) => p.full_name).join(', ')}.
                </p>
              )}
            </>
          )}
        </div>
      )}
    </section>
  )
}

/**
 * What another municipality's server needs, before it is first started, so
 * the key issued for it installs there.
 */
function ServerSettings({ profile }: { profile: LicenseeDetail }) {
  const settings = [
    `GIAMS_TENANT_ID="${profile.target_tenant_id}"`,
    `GIAMS_TENANT_CODE="${profile.target_tenant_code}"`,
    `GIAMS_MUNICIPALITY="${profile.municipality_name}"`,
  ].join('\n')

  return (
    <section className="mb-5 rounded-md border border-line bg-surface p-4">
      <div className="mb-2 flex items-start justify-between gap-4">
        <h3 className="flex items-center gap-2 font-medium">
          <Server className="size-[18px] text-ink-secondary" aria-hidden />
          Setting up their server
        </h3>
        <CopyButton text={settings} label="Copy settings" />
      </div>

      <ol className="mb-3 list-decimal space-y-1 pl-5 text-sm text-ink-secondary">
        <li>Before their server is started for the first time, put these lines in its <code>.env</code> file.</li>
        <li>Start it, and sign in as its administrator.</li>
        <li>Paste the licence key into System → Subscription and install it.</li>
        <li>The accounts below are created then, each with a one-time password shown to their administrator.</li>
      </ol>

      <pre className="overflow-x-auto rounded-sm bg-sunken px-3 py-2 font-mono text-xs">{settings}</pre>
    </section>
  )
}

function People({ profile }: { profile: LicenseeDetail }) {
  return (
    <section className="mb-5">
      <h3 className="mb-2 font-medium">People</h3>
      <div className="overflow-hidden rounded-md border border-line bg-surface">
        <table className="w-full text-sm">
          <thead>
            <tr className="border-b border-line-strong text-left text-xs text-ink-muted">
              <th className="px-4 py-2 font-normal">Name</th>
              <th className="px-4 py-2 font-normal">Position</th>
              <th className="px-4 py-2 font-normal">Username</th>
              <th className="px-4 py-2 font-normal">Role on their server</th>
            </tr>
          </thead>
          <tbody>
            {profile.people.map((p) => (
              <tr key={p.username} className="border-b border-line last:border-b-0">
                <td className="px-4 py-2 font-medium">{p.full_name}</td>
                <td className="px-4 py-2 text-ink-secondary">{p.position_title ?? '—'}</td>
                <td className="px-4 py-2"><code className="tabular">{p.username}</code></td>
                <td className="px-4 py-2">{p.role === 'ACCOUNTANT' ? 'Municipal Accountant' : 'Accounting Staff'}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </section>
  )
}

function History({ keys }: { keys: IssuedKey[] }) {
  return (
    <section className="mb-5">
      <h3 className="mb-2 font-medium">Earlier keys</h3>
      <ul className="divide-y divide-line rounded-md border border-line bg-surface">
        {keys.map((k) => (
          <li key={k.id} className="flex items-center justify-between gap-4 px-4 py-2.5 text-sm">
            <span>
              {describeKey(k)}
              <span className="block text-xs text-ink-muted">
                Issued {formatDateTime(k.issued_at)}{k.issued_by_name && ` by ${k.issued_by_name}`}
              </span>
            </span>
            <CopyButton text={k.token} label="Copy key" />
          </li>
        ))}
      </ul>
    </section>
  )
}

/**
 * The subscription: what it is paid through, and renewing it.
 *
 * Renewing records another month or year on the same key. Nothing is issued
 * and nothing is sent: the municipality's server collects the new date by
 * itself, and "Check for renewal" on their side collects it at once.
 */
function Subscription({ profile }: { profile: LicenseeDetail }) {
  const renew = useRenew(profile.id)
  const [confirming, setConfirming] = useState(false)
  const [notes, setNotes] = useState('')

  const lifetime = profile.access === 'lifetime' || isLifetime(profile.paid_through)
  const period = profile.access === 'monthly' ? 'one month' : 'one year'
  const standing = subscriptionStanding(profile.paid_through)

  return (
    <section className="mb-5 rounded-md border border-line bg-surface p-4">
      <div className="flex items-start justify-between gap-4">
        <div>
          <h3 className="flex items-center gap-2 font-medium">
            <CalendarClock className="size-[18px] text-ink-secondary" aria-hidden />
            Subscription
          </h3>
          <p className="mt-1 text-sm">
            {accessLabel(profile.access)}
            {lifetime
              ? ' — never expires.'
              : <> — paid through <span className="font-medium">{formatExpiry(profile.paid_through)}</span></>}
          </p>
          {!lifetime && standing && (
            <p className={`mt-0.5 text-sm ${standing.tone}`}>{standing.text}</p>
          )}
        </div>

        {!lifetime && !confirming && (
          <button
            type="button"
            onClick={() => { setConfirming(true); renew.reset() }}
            className="flex shrink-0 items-center gap-2 rounded-pill bg-accent-fill px-4 py-2 text-sm font-medium text-accent-ink hover:brightness-[0.97]"
          >
            <RotateCcw className="size-4" aria-hidden />
            Renew — add {period}
          </button>
        )}
      </div>

      {confirming && (
        <div className="mt-4 border-t border-line pt-4">
          <p className="text-sm text-ink-secondary">
            Adds {period}, counted from{' '}
            {standing?.lapsed ? 'today, as the subscription has already ended' : `${formatExpiry(profile.paid_through)}, so no paid day is lost`}.
            The office keeps the same key.
          </p>
          <div className="mt-3 flex items-end gap-2">
            <label className="grow">
              <span className="mb-1 block text-xs text-ink-muted">Notes (optional)</span>
              <input
                value={notes}
                onChange={(e) => setNotes(e.target.value)}
                placeholder="Official receipt no., amount paid…"
                className={inputClass}
              />
            </label>
            <button
              type="button"
              disabled={renew.isPending}
              onClick={() =>
                renew.mutate(
                  { notes: notes.trim() || undefined },
                  { onSuccess: () => { setConfirming(false); setNotes('') } },
                )
              }
              className="shrink-0 rounded-pill bg-accent-fill px-4 py-2 text-sm font-medium text-accent-ink disabled:opacity-60"
            >
              {renew.isPending ? 'Renewing…' : 'Renew'}
            </button>
            <button
              type="button"
              onClick={() => setConfirming(false)}
              className="shrink-0 rounded-pill px-3 py-2 text-sm hover:bg-hover"
            >
              Cancel
            </button>
          </div>
        </div>
      )}

      {renew.error && <div className="mt-3"><ErrorNotice error={renew.error} /></div>}

      {renew.isSuccess && (
        <p className="mt-3 flex items-start gap-2 rounded-md bg-ok-fill px-3 py-2 text-sm text-ok-ink">
          <Check className="mt-0.5 size-4 shrink-0" aria-hidden />
          Renewed — paid through {formatExpiry(profile.paid_through)}.{' '}
          {profile.for_this_server
            ? 'This server picks it up with Check for renewal on System → Subscription, or by itself within six hours.'
            : "The municipality's server picks it up by itself within six hours, or at once with Check for renewal."}
        </p>
      )}
    </section>
  )
}

/** Where a paid-through date leaves the office today, in its own terms. */
function subscriptionStanding(paidThrough: string | null) {
  if (!paidThrough || isLifetime(paidThrough)) return null

  const end = new Date(paidThrough).getTime()
  const days = Math.ceil((end - Date.now()) / 86_400_000)
  const locksOn = formatDate(new Date(end + GRACE_DAYS * 86_400_000).toISOString())

  if (days > WARN_DAYS) return { text: `Active, ${days} days left.`, tone: 'text-ink-secondary', lapsed: false }
  if (days >= 0) return { text: `Ends in ${days} day${days === 1 ? '' : 's'}. Users are being warned.`, tone: 'text-warn-ink', lapsed: false }
  if (-days < GRACE_DAYS) return { text: `Ended. The office's system locks on ${locksOn}.`, tone: 'text-danger-ink', lapsed: true }
  return { text: `Ended. The office's system has been locked since ${locksOn}.`, tone: 'text-danger-ink', lapsed: true }
}

/** Mirrors the server: warned seven days before, locked five days after. */
const WARN_DAYS = 7
const GRACE_DAYS = 5

function Renewals({ profile }: { profile: LicenseeDetail }) {
  return (
    <section className="mb-5">
      <h3 className="mb-2 font-medium">Renewals</h3>
      <ul className="divide-y divide-line rounded-md border border-line bg-surface">
        {profile.renewals.map((r) => (
          <li key={r.id} className="px-4 py-2.5 text-sm">
            {formatDate(r.period_start)} to {formatDate(r.paid_through)}
            {r.notes && <span className="text-ink-secondary"> · {r.notes}</span>}
            <span className="block text-xs text-ink-muted">
              Recorded {formatDateTime(r.renewed_at)}{r.renewed_by_name && ` by ${r.renewed_by_name}`}
            </span>
          </li>
        ))}
      </ul>
    </section>
  )
}
