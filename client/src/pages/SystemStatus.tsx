import { CheckCircle2, CloudOff, CloudUpload, KeyRound, RefreshCw, ShieldAlert, ShieldCheck } from 'lucide-react'
import { ErrorNotice } from '@/components/ErrorNotice'
import { OneTimePasswords } from '@/components/OneTimePasswords'
import { useAuditIntegrity, useNodeHealth } from '@/queries/health'
import { useCheckRenewal, useInstallLicence, useLicence, type ProvisionedAccount } from '@/queries/licence'
import { accessLabel, isLifetime } from '@/schemas/admin'
import { useSetAutomatic, useSyncStatus, useUploadNow } from '@/queries/sync'
import { useState } from 'react'

/**
 * System status.
 *
 * The first real screen: everything on it comes from the office node over
 * HTTP. It stays in the finished product as the administrator's health page,
 * because "is the trail intact and is the server reachable" are questions
 * worth being able to answer without a developer.
 */
export function SystemStatus() {
  return (
    <div>
      <h1 className="text-xl font-medium">System status</h1>
      <p className="mt-1 text-ink-secondary">
        The condition of this workstation's connection and of the accounting records.
      </p>

      <div className="mt-6 space-y-4">
        <NodeCard />
        <LicenceCard />
        <SyncCard />
        <AuditCard />
      </div>
    </div>
  )
}

function NodeCard() {
  const { data, error, isPending } = useNodeHealth()

  return (
    <section className="rounded-md bg-sunken p-4">
      <h2 className="font-medium">Accounting office server</h2>

      {isPending && <p className="mt-2 text-ink-muted">Checking…</p>}

      {error && (
        <div className="mt-3">
          <ErrorNotice error={error} />
        </div>
      )}

      {data && (
        <dl className="mt-3 grid grid-cols-[auto_1fr] gap-x-6 gap-y-1.5 text-sm">
          <Row label="Server" value={data.node_code} />
          <Row label="Records database" value="Reachable" />
          <Row label="Software version" value={data.version} />
          <Row
            label="Server time"
            value={new Date(data.server_time).toLocaleString('en-PH', {
              dateStyle: 'medium',
              timeStyle: 'short',
            })}
          />
        </dl>
      )}
    </section>
  )
}

function Row({ label, value }: { label: string; value: string }) {
  return (
    <>
      <dt className="text-ink-muted">{label}</dt>
      <dd className="tabular">{value}</dd>
    </>
  )
}

/**
 * Runs the audit-trail verification on demand.
 *
 * Not run automatically on page load: it walks the entire chain, and an
 * administrator should choose when to spend that. The result is phrased for
 * someone who may have to repeat it to an auditor.
 */
function AuditCard() {
  const [checked, setChecked] = useState(false)
  const { data, error, isFetching } = useAuditIntegrity(checked)

  return (
    <section className="rounded-md bg-sunken p-4">
      <div className="flex items-start justify-between gap-4">
        <div>
          <h2 className="font-medium">Audit trail</h2>
          <p className="mt-1 text-sm text-ink-secondary">
            Every recorded action is linked to the one before it. Verifying
            re-checks those links and reports anything that has been altered.
          </p>
        </div>
        <button
          type="button"
          onClick={() => setChecked(true)}
          disabled={isFetching}
          className="shrink-0 rounded-pill bg-accent-fill px-4 py-2 text-sm font-medium text-accent-ink hover:brightness-[0.97] disabled:opacity-60"
        >
          {isFetching ? 'Verifying…' : 'Verify now'}
        </button>
      </div>

      {error && (
        <div className="mt-3">
          <ErrorNotice error={error} />
        </div>
      )}

      {data?.intact && (
        <p className="mt-3 flex items-center gap-2 rounded-md bg-ok-fill px-3 py-2 text-ok-ink">
          <ShieldCheck className="size-[18px] shrink-0" aria-hidden />
          {data.summary}
        </p>
      )}

      {data && !data.intact && (
        <div className="mt-3 rounded-md bg-danger-fill px-3 py-2 text-danger-ink">
          <p className="flex items-center gap-2 font-medium">
            <ShieldAlert className="size-[18px] shrink-0" aria-hidden />
            {data.summary}
          </p>
          <ul className="mt-2 space-y-1 text-sm">
            {data.problems.map((p) => (
              <li key={`${p.audit_id}-${p.problem}`}>
                Record {p.seq} — {p.problem}
              </li>
            ))}
          </ul>
        </div>
      )}

      {!checked && (
        <p className="mt-3 flex items-center gap-2 text-sm text-ink-muted">
          <CheckCircle2 className="size-4 shrink-0" aria-hidden />
          Last verified automatically when the server started.
        </p>
      )}
    </section>
  )
}

/**
 * Uploading to the cloud.
 *
 * The office asked for both: on its own, or only when told. Both are here, and
 * neither is presented as something that can go wrong with the books. A
 * voucher recorded on a node with no internet is recorded — so the language on
 * this card is about the *copy*, never about the record.
 */
function SyncCard() {
  const { data, error, isPending } = useSyncStatus()
  const upload = useUploadNow()
  const automatic = useSetAutomatic()

  return (
    <section className="rounded-md bg-sunken p-4">
      <div className="flex items-start justify-between gap-4">
        <div>
          <h2 className="font-medium">Copy held off-site</h2>
          <p className="mt-1 text-sm text-ink-secondary">
            Records are kept here first and copied to the cloud service
            afterwards. Nothing waits on the connection.
          </p>
        </div>

        {data?.configured && (
          <button
            type="button"
            onClick={() => upload.mutate()}
            disabled={upload.isPending || data.halted}
            className="shrink-0 rounded-pill bg-accent-fill px-4 py-2 text-sm font-medium text-accent-ink hover:brightness-[0.97] disabled:opacity-60"
          >
            {upload.isPending ? 'Uploading…' : 'Upload now'}
          </button>
        )}
      </div>

      {isPending && <p className="mt-2 text-ink-muted">Checking…</p>}
      {error && <div className="mt-3"><ErrorNotice error={error} /></div>}
      {upload.error && <div className="mt-3"><ErrorNotice error={upload.error} /></div>}

      {data && !data.configured && (
        <p className="mt-3 flex items-center gap-2 text-sm text-ink-secondary">
          <CloudOff className="size-4 shrink-0" aria-hidden />
          No cloud service has been set up for this municipality. Everything is
          recorded on the office server.
        </p>
      )}

      {data?.configured && (
        <>
          <dl className="mt-3 grid grid-cols-[auto_1fr] gap-x-6 gap-y-1.5 text-sm">
            <Row
              label="Waiting to upload"
              value={data.pending === 0 ? 'Nothing — the copy is current' : `${data.pending} record${data.pending === 1 ? '' : 's'}`}
            />
            <Row label="Last uploaded" value={when(data.last_success_at)} />
            <Row label="Last attempt" value={when(data.last_attempt_at)} />
          </dl>

          <label className="mt-3 flex items-center gap-2 text-sm">
            <input
              type="checkbox"
              checked={data.automatic}
              onChange={(e) => automatic.mutate(e.target.checked)}
              className="size-4 accent-[var(--color-accent-solid)]"
            />
            Upload on its own, every few minutes
          </label>

          {data.halted && (
            <p className="mt-3 flex items-start gap-2 rounded-md bg-danger-fill px-3 py-2 text-danger-ink">
              <ShieldAlert className="mt-0.5 size-[18px] shrink-0" aria-hidden />
              {data.problem}
            </p>
          )}

          {!data.halted && data.problem && (
            <p className="mt-3 flex items-start gap-2 rounded-md bg-info-fill px-3 py-2 text-info-ink">
              <CloudUpload className="mt-0.5 size-[18px] shrink-0" aria-hidden />
              {data.problem}
            </p>
          )}
        </>
      )}
    </section>
  )
}

function when(value: string | null): string {
  if (!value) return 'Not yet'
  return new Date(value).toLocaleString('en-PH', { dateStyle: 'medium', timeStyle: 'short' })
}

/** Collects a renewal from the licensing server now, and says what it found. */
function CheckForRenewal() {
  const check = useCheckRenewal()

  return (
    <div className="mt-3">
      <button
        type="button"
        onClick={() => check.mutate()}
        disabled={check.isPending}
        className="flex items-center gap-2 rounded-pill bg-surface px-4 py-2 text-sm font-medium hover:bg-hover disabled:opacity-60"
      >
        <RefreshCw className={`size-4 ${check.isPending ? 'animate-spin' : ''}`} aria-hidden />
        {check.isPending ? 'Checking…' : 'Check for renewal'}
      </button>
      {check.data && (
        <p
          className={`mt-2 rounded-md px-3 py-2 text-sm ${
            check.data.outcome === 'renewed' ? 'bg-ok-fill text-ok-ink' : 'bg-surface text-ink-secondary'
          }`}
        >
          {check.data.message}
        </p>
      )}
      {check.error && <div className="mt-2"><ErrorNotice error={check.error} /></div>}
    </div>
  )
}

/**
 * The accounts a licence key brought with it: the Municipal Accountant and
 * Accounting Staff named when the key was issued. New ones come with a
 * one-time password; anyone already here is left exactly as they were.
 */
function ProvisionedFromKey({ accounts }: { accounts: ProvisionedAccount[] }) {
  const created = accounts.filter((a) => a.status === 'created' && a.password)
  const existing = accounts.filter((a) => a.status === 'existing')

  if (accounts.length === 0) return null

  return (
    <div className="mt-3">
      {created.length > 0 && (
        <OneTimePasswords
          title="Accounts created from this key"
          people={created.map((a) => ({ full_name: a.full_name, username: a.username, password: a.password ?? '' }))}
        />
      )}
      {existing.length > 0 && (
        <p className="text-sm text-ink-secondary">
          Already had an account, left unchanged: {existing.map((a) => a.full_name).join(', ')}.
        </p>
      )}
    </div>
  )
}

/**
 * The subscription, and where a renewal key is entered.
 *
 * Deliberately plain about what expiry does and does not do. The commonest
 * fear an office has about a subscription is being locked out of its own
 * records, and the answer — that everything stays readable and printable
 * forever — is worth stating on the screen rather than in a manual.
 */
function LicenceCard() {
  const { data, error, isPending } = useLicence()
  const install = useInstallLicence()
  const [key, setKey] = useState('')

  return (
    <section className="rounded-md bg-sunken p-4">
      <h2 className="font-medium">Subscription</h2>

      {isPending && <p className="mt-2 text-ink-muted">Checking…</p>}
      {error && <div className="mt-3"><ErrorNotice error={error} /></div>}

      {data && (
        <>
          <dl className="mt-3 grid grid-cols-[auto_1fr] gap-x-6 gap-y-1.5 text-sm">
            <Row label="Plan" value={data.plan === 'on-premise' ? 'Local installation' : accessLabel(data.plan)} />
            <Row label="Workstations" value={data.seats > 1000 ? 'No limit' : String(data.seats)} />
            <Row
              label="Runs until"
              value={isLifetime(data.expires_at)
                ? 'Lifetime — never expires'
                : data.expires_at
                  ? new Date(data.expires_at).toLocaleDateString('en-PH', { dateStyle: 'long' })
                  : 'No expiry'}
            />
            <Row
              label="Recording"
              value={data.can_record ? 'Permitted' : 'Paused — viewing and printing continue'}
            />
          </dl>

          {data.notice && (
            <p className={`mt-3 flex items-start gap-2 rounded-md px-3 py-2 ${
              data.state === 'expiring' ? 'bg-warn-fill text-warn-ink' : 'bg-danger-fill text-danger-ink'
            }`}>
              <ShieldAlert className="mt-0.5 size-[18px] shrink-0" aria-hidden />
              {data.notice}
            </p>
          )}

          <p className="mt-3 text-sm text-ink-secondary">
            Everyone is warned seven days before the subscription ends. After it ends the
            system keeps working for five more days, then locks until it is renewed.
            Nothing is ever deleted. Renewals reach this server by themselves — no new
            key is needed — and <span className="font-medium text-ink">Check for renewal</span> collects
            one straight away.
          </p>

          <CheckForRenewal />

          <form
            className="mt-3 flex items-start gap-2"
            onSubmit={(e) => {
              e.preventDefault()
              install.mutate(key.trim(), { onSuccess: () => setKey('') })
            }}
          >
            <span className="relative grow">
              <KeyRound
                className="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-ink-muted"
                aria-hidden
              />
              <input
                value={key}
                onChange={(e) => setKey(e.target.value)}
                placeholder="Paste a licence key to renew"
                aria-label="Licence key"
                className="w-full rounded-pill border border-line bg-surface py-2 pl-9 pr-3 text-sm outline-none focus:border-accent-solid"
              />
            </span>
            <button
              type="submit"
              disabled={key.trim() === '' || install.isPending}
              className="shrink-0 rounded-pill bg-accent-fill px-4 py-2 text-sm font-medium text-accent-ink hover:brightness-[0.97] disabled:opacity-60"
            >
              {install.isPending ? 'Checking…' : 'Install'}
            </button>
          </form>

          {install.error && <div className="mt-3"><ErrorNotice error={install.error} /></div>}
          {install.isSuccess && !install.error && (
            <>
              <p className="mt-3 flex items-center gap-2 rounded-md bg-ok-fill px-3 py-2 text-ok-ink">
                <ShieldCheck className="size-[18px] shrink-0" aria-hidden />
                The licence key has been installed.
              </p>
              <ProvisionedFromKey accounts={install.data.provisioned} />
            </>
          )}
        </>
      )}
    </section>
  )
}
