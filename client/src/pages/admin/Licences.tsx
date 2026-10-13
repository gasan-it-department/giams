import { Link, useNavigate } from 'react-router-dom'
import { Building2, Plus, ServerCog } from 'lucide-react'
import { ErrorNotice } from '@/components/ErrorNotice'
import { useLicensees, useLicensing, type LicenseeSummary } from '@/queries/admin'
import { accessLabel, isLifetime } from '@/schemas/admin'
import { useAuthStore } from '@/stores/authStore'
import { AdminPage, NoAccess, formatDate } from './shared'

/**
 * Licence profiles: every municipality a key has been issued to.
 */
export function Licences() {
  const allowed = useAuthStore((s) => s.can('licence.issue'))
  const navigate = useNavigate()

  const { data: capability } = useLicensing(allowed)
  const { data: profiles, error, isPending } = useLicensees(allowed)

  if (!allowed) {
    return <AdminPage title="Licences"><NoAccess /></AdminPage>
  }

  const canIssue = capability?.can_issue ?? false

  return (
    <AdminPage
      title="Licences"
      description="Every municipality a licence key has been issued to."
      actions={
        canIssue && (
          <Link
            to="/admin/licences/new"
            className="flex items-center gap-2 rounded-pill bg-accent-fill px-4 py-2 text-sm font-medium text-accent-ink hover:brightness-[0.97]"
          >
            <Plus className="size-4" aria-hidden />
            Issue a licence
          </Link>
        )
      }
    >
      {capability && !capability.can_issue && (
        <p className="mb-4 flex items-start gap-2 rounded-md bg-warn-fill px-3 py-2 text-sm text-warn-ink">
          <ServerCog className="mt-0.5 size-4 shrink-0" aria-hidden />
          {capability.reason}
        </p>
      )}

      {error && <div className="mb-4"><ErrorNotice error={error} /></div>}
      {isPending && <p className="py-6 text-ink-muted">Loading…</p>}

      {profiles?.length === 0 && (
        <div className="rounded-md border border-line bg-surface px-6 py-10 text-center">
          <Building2 className="mx-auto size-6 text-ink-muted" aria-hidden />
          <p className="mt-2 font-medium">No licences issued yet.</p>
          <p className="mt-1 text-sm text-ink-secondary">
            {canIssue
              ? 'Issue one for a municipality: choose it, name its accountant and staff, generate the key, then save the profile.'
              : 'Profiles appear here once keys are issued from the licensing server.'}
          </p>
        </div>
      )}

      {profiles && profiles.length > 0 && (
        <div className="overflow-hidden rounded-md border border-line bg-surface">
          <table className="w-full text-sm">
            <thead>
              <tr className="border-b border-line-strong text-left text-xs text-ink-muted">
                <th className="px-4 py-2.5 font-normal">Municipality</th>
                <th className="px-4 py-2.5 font-normal">Municipal Accountant</th>
                <th className="px-4 py-2.5 font-normal">People</th>
                <th className="px-4 py-2.5 font-normal">Access</th>
                <th className="px-4 py-2.5 font-normal">Runs until</th>
                <th className="px-4 py-2.5 text-right font-normal">Keys</th>
              </tr>
            </thead>
            <tbody>
              {profiles.map((p) => (
                <tr
                  key={p.id}
                  onClick={() => navigate(`/admin/licences/${p.id}`)}
                  className="cursor-pointer border-b border-line last:border-b-0 hover:bg-hover"
                >
                  <td className="px-4 py-3">
                    <span className="font-medium">{p.municipality_name}</span>
                    {p.for_this_server && (
                      <span className="ml-2 rounded-pill bg-accent-soft px-2 py-0.5 text-xs text-accent-soft-ink">This server</span>
                    )}
                  </td>
                  <td className="px-4 py-3">{p.accountant_name ?? '—'}</td>
                  <td className="px-4 py-3 tabular">{p.people_count}</td>
                  <td className="px-4 py-3">
                    {p.latest_plan ? `${accessLabel(p.latest_plan)} · ${p.latest_seats} workstations` : '—'}
                  </td>
                  <td className="px-4 py-3"><Expiry profile={p} /></td>
                  <td className="px-4 py-3 text-right tabular text-ink-secondary">{p.keys_issued}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </AdminPage>
  )
}

/** The latest key's end, marked when it has passed or is close. */
function Expiry({ profile }: { profile: LicenseeSummary }) {
  if (!profile.latest_expires_at) return <span className="text-ink-muted">—</span>
  if (isLifetime(profile.latest_expires_at)) return <span className="text-ok-ink">Lifetime</span>

  const days = Math.ceil((new Date(profile.latest_expires_at).getTime() - Date.now()) / 86_400_000)
  const date = formatDate(profile.latest_expires_at)

  if (days < 0) return <span className="text-danger-ink">{date} · ended</span>
  if (days <= 30) return <span className="text-warn-ink">{date} · {days} days left</span>
  return <span>{date}</span>
}
