import { useState } from 'react'
import { useForm } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import { UserPlus, X } from 'lucide-react'
import type { z } from 'zod'
import { ErrorNotice } from '@/components/ErrorNotice'
import {
  useCreateUser, useResetPassword, useRoles, useSetActive, useUserAccounts,
  type Account,
} from '@/queries/admin'
import { newAccountSchema, suggestUsername } from '@/schemas/admin'
import { useAuthStore } from '@/stores/authStore'
import {
  AdminPage, Field, NoAccess, OneTimePasswords, applyServerErrors, formatDateTime, inputClass,
} from './shared'

type Issued = { title: string; people: { full_name: string; username: string; password: string }[] }

/**
 * Accounts on this server.
 *
 * Creating, resetting, and switching off accounts used to be a console
 * command; this is the same thing with a screen, under the same rules — a new
 * account gets a one-time password shown once, and must choose its own at
 * first sign-in.
 */
export function Accounts() {
  const can = useAuthStore((s) => s.can)
  const me = useAuthStore((s) => s.user)

  const { data: accounts, error, isPending } = useUserAccounts()
  const reset = useResetPassword()
  const setActive = useSetActive()

  const [creating, setCreating] = useState(false)
  const [issued, setIssued] = useState<Issued | null>(null)
  const [confirming, setConfirming] = useState<{ id: string; action: 'reset' | 'off' } | null>(null)

  if (!can('user.manage')) {
    return <AdminPage title="Accounts"><NoAccess /></AdminPage>
  }

  function confirmReset(account: Account) {
    reset.mutate(account.id, {
      onSuccess: ({ password }) => {
        setIssued({
          title: `New password for ${account.full_name}`,
          people: [{ full_name: account.full_name, username: account.username, password }],
        })
        setConfirming(null)
      },
    })
  }

  function confirmOff(account: Account) {
    setActive.mutate({ id: account.id, active: false }, { onSuccess: () => setConfirming(null) })
  }

  return (
    <AdminPage
      title="Accounts"
      description="The people who can sign in to this server."
      actions={
        !creating && (
          <button
            type="button"
            onClick={() => { setCreating(true); setIssued(null) }}
            className="flex items-center gap-2 rounded-pill bg-accent-fill px-4 py-2 text-sm font-medium text-accent-ink hover:brightness-[0.97]"
          >
            <UserPlus className="size-4" aria-hidden />
            New account
          </button>
        )
      }
    >
      {issued && <OneTimePasswords title={issued.title} people={issued.people} onDone={() => setIssued(null)} />}

      {creating && (
        <NewAccount
          onCancel={() => setCreating(false)}
          onCreated={(result) => { setCreating(false); setIssued(result) }}
        />
      )}

      {error && <div className="mb-4"><ErrorNotice error={error} /></div>}
      {reset.error && <div className="mb-4"><ErrorNotice error={reset.error} /></div>}
      {setActive.error && <div className="mb-4"><ErrorNotice error={setActive.error} /></div>}

      {isPending && <p className="py-6 text-ink-muted">Loading…</p>}

      {accounts && (
        <table className="w-full text-sm">
          <thead>
            <tr className="border-b border-line-strong text-left text-xs text-ink-muted">
              <th className="py-2 pr-3 font-normal">Name</th>
              <th className="py-2 pr-3 font-normal">Username</th>
              <th className="py-2 pr-3 font-normal">Role</th>
              <th className="py-2 pr-3 font-normal">Status</th>
              <th className="py-2 pr-3 font-normal">Last sign-in</th>
              <th className="py-2" />
            </tr>
          </thead>
          <tbody>
            {accounts.map((a) => {
              const isMe = a.id === me?.id
              const asking = confirming?.id === a.id ? confirming.action : null

              return (
                <tr key={a.id} className={`border-b border-line align-top ${a.is_active ? '' : 'text-ink-muted'}`}>
                  <td className="py-2 pr-3">
                    <span className="font-medium">{a.full_name}</span>
                    {isMe && <span className="ml-2 rounded-pill bg-sunken px-2 py-0.5 text-xs text-ink-secondary">You</span>}
                    {a.position_title && <span className="block text-xs text-ink-muted">{a.position_title}</span>}
                  </td>
                  <td className="py-2 pr-3"><code className="tabular">{a.username}</code></td>
                  <td className="py-2 pr-3">{a.role_names.join(', ') || '—'}</td>
                  <td className="whitespace-nowrap py-2 pr-3"><Status account={a} /></td>
                  <td className="py-2 pr-3 text-ink-secondary">{formatDateTime(a.last_login_at)}</td>
                  <td className="py-2 text-right">
                    {isMe ? null : asking ? (
                      <span className="inline-flex items-center gap-1.5">
                        <span className="text-xs text-ink-secondary">
                          {asking === 'reset' ? 'Reset and sign them out?' : 'Switch off and sign them out?'}
                        </span>
                        <button
                          type="button"
                          disabled={reset.isPending || setActive.isPending}
                          onClick={() => (asking === 'reset' ? confirmReset(a) : confirmOff(a))}
                          className="rounded-pill bg-danger-fill px-3 py-1 text-xs font-medium text-danger-ink disabled:opacity-60"
                        >
                          {asking === 'reset' ? 'Reset' : 'Switch off'}
                        </button>
                        <button
                          type="button"
                          onClick={() => setConfirming(null)}
                          className="rounded-pill px-2 py-1 text-xs hover:bg-hover"
                        >
                          Cancel
                        </button>
                      </span>
                    ) : (
                      <span className="inline-flex gap-1">
                        {a.is_active && (
                          <button
                            type="button"
                            onClick={() => setConfirming({ id: a.id, action: 'reset' })}
                            className="whitespace-nowrap rounded-pill px-3 py-1 text-xs hover:bg-hover"
                          >
                            Reset password
                          </button>
                        )}
                        {a.is_active ? (
                          <button
                            type="button"
                            onClick={() => setConfirming({ id: a.id, action: 'off' })}
                            className="whitespace-nowrap rounded-pill px-3 py-1 text-xs hover:bg-hover"
                          >
                            Switch off
                          </button>
                        ) : (
                          <button
                            type="button"
                            disabled={setActive.isPending}
                            onClick={() => setActive.mutate({ id: a.id, active: true })}
                            className="whitespace-nowrap rounded-pill bg-sunken px-3 py-1 text-xs font-medium hover:bg-hover disabled:opacity-60"
                          >
                            Switch on
                          </button>
                        )}
                      </span>
                    )}
                  </td>
                </tr>
              )
            })}
          </tbody>
        </table>
      )}
    </AdminPage>
  )
}

function Status({ account }: { account: Account }) {
  if (!account.is_active) return <span className="text-ink-muted">Switched off</span>
  if (account.locked) return <span className="text-danger-ink">Locked out</span>
  if (account.must_change_password) return <span className="text-warn-ink">Awaiting first sign-in</span>
  return <span className="text-ok-ink">Active</span>
}

function NewAccount({
  onCancel, onCreated,
}: {
  onCancel: () => void
  onCreated: (issued: Issued) => void
}) {
  const { data: roles } = useRoles()
  const create = useCreateUser()

  const {
    register, handleSubmit, setValue, getValues, setError,
    formState: { errors },
  } = useForm<z.input<typeof newAccountSchema>, unknown, z.output<typeof newAccountSchema>>({
    resolver: zodResolver(newAccountSchema),
    defaultValues: { full_name: '', username: '', position_title: '', role: '' },
  })

  function submit(values: z.output<typeof newAccountSchema>) {
    create.mutate(values, {
      onSuccess: ({ account, password }) =>
        onCreated({
          title: `Account created for ${account.full_name}`,
          people: [{ full_name: account.full_name, username: account.username, password }],
        }),
      onError: (e) => { applyServerErrors(e, setError) },
    })
  }

  return (
    <form onSubmit={handleSubmit(submit)} className="mb-5 rounded-md border border-line bg-sunken p-4">
      <div className="mb-3 flex items-center justify-between">
        <h2 className="font-medium">New account</h2>
        <button type="button" onClick={onCancel} aria-label="Cancel" className="rounded-pill p-1 hover:bg-hover">
          <X className="size-[18px]" aria-hidden />
        </button>
      </div>

      <div className="grid grid-cols-2 gap-x-4 gap-y-3">
        <Field label="Full name" error={errors.full_name?.message}>
          <input
            {...register('full_name', {
              onBlur: () => {
                if (!getValues('username')) {
                  setValue('username', suggestUsername(getValues('full_name')), { shouldValidate: true })
                }
              },
            })}
            placeholder="Maria Santos"
            className={inputClass}
          />
        </Field>

        <Field label="Username" error={errors.username?.message} hint="What they type to sign in.">
          <input {...register('username')} placeholder="m.santos" className={`${inputClass} tabular`} />
        </Field>

        <Field label="Role" error={errors.role?.message}>
          <select {...register('role')} className={inputClass}>
            <option value="">Choose…</option>
            {roles?.map((r) => <option key={r.code} value={r.code}>{r.name}</option>)}
          </select>
        </Field>

        <Field label="Position (optional)" error={errors.position_title?.message}>
          <input {...register('position_title')} placeholder="Administrative Aide IV" className={inputClass} />
        </Field>
      </div>

      {create.error && !(Object.keys(errors).length > 0) && (
        <div className="mt-3"><ErrorNotice error={create.error} /></div>
      )}

      <div className="mt-4 flex items-center gap-2">
        <button
          type="submit"
          disabled={create.isPending}
          className="rounded-pill bg-accent-fill px-4 py-2 text-sm font-medium text-accent-ink hover:brightness-[0.97] disabled:opacity-60"
        >
          {create.isPending ? 'Creating…' : 'Create account'}
        </button>
        <span className="text-xs text-ink-muted">A one-time password is made for them. They choose their own at first sign-in.</span>
      </div>
    </form>
  )
}
