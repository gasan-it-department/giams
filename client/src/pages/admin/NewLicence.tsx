import { useState, type ReactNode } from 'react'
import { Link, useNavigate } from 'react-router-dom'
import { useFieldArray, useForm } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import { AlertTriangle, ArrowLeft, ArrowRight, Check, KeyRound, Plus, Save, Trash2 } from 'lucide-react'
import type { z } from 'zod'
import { ErrorNotice } from '@/components/ErrorNotice'
import {
  useGenerateLicence, useLicensing, useSaveLicensee, type GeneratedKey,
} from '@/queries/admin'
import {
  ACCESS_OPTIONS, MARINDUQUE, accessLabel, newLicenseeSchema, suggestUsername,
} from '@/schemas/admin'
import { useAuthStore } from '@/stores/authStore'
import {
  AdminPage, CopyButton, Field, NoAccess, applyServerErrors, formatDate, formatExpiry, inputClass,
} from './shared'

type Input = z.input<typeof newLicenseeSchema>
type Output = z.output<typeof newLicenseeSchema>
type Step = 1 | 2 | 3

/** Today, in the office's own calendar rather than UTC's. */
export function today(): string {
  const d = new Date()
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`
}

const OTHER = '__other__'

/**
 * Issuing a licence, in three steps.
 *
 *   1. Details — the municipality, its Municipal Accountant and Accounting
 *      Staff, and the key: monthly renewal, annual, or lifetime.
 *   2. Generate — the key is signed and shown, ready to copy. Nothing is
 *      saved yet.
 *   3. Save — the profile is saved from the key itself, so what is kept is
 *      exactly what was copied.
 */
export function NewLicence() {
  const allowed = useAuthStore((s) => s.can('licence.issue'))
  const navigate = useNavigate()

  const { data: capability } = useLicensing(allowed)
  const generate = useGenerateLicence()
  const save = useSaveLicensee()

  const [step, setStep] = useState<Step>(1)
  const [key, setKey] = useState<GeneratedKey | null>(null)
  const [notes, setNotes] = useState('')
  const [typingOther, setTypingOther] = useState(false)

  const {
    register, control, handleSubmit, watch, setValue, getValues, setError,
    formState: { errors },
  } = useForm<Input, unknown, Output>({
    resolver: zodResolver(newLicenseeSchema),
    defaultValues: {
      municipality_name: '',
      for_this_server:   false,
      accountant:        { full_name: '', username: '' },
      staff:             [{ full_name: '', username: '' }],
      terms:             { access: 'annual', seats: 10, starts_on: today() },
    },
  })

  const { fields: staff, append, remove } = useFieldArray({ control, name: 'staff' })

  if (!allowed) {
    return <AdminPage title="Issue a licence"><NoAccess /></AdminPage>
  }

  const thisServer = capability?.this_server.municipality
  const choices = [
    ...(thisServer ? [thisServer] : []),
    ...MARINDUQUE.filter((m) => m !== thisServer),
  ]

  const municipality = watch('municipality_name')
  const access = watch('terms.access')

  /** Choosing a municipality also decides whether the key binds to this server. */
  function chooseMunicipality(choice: string) {
    if (choice === OTHER) {
      setTypingOther(true)
      setValue('municipality_name', '', { shouldValidate: false })
      setValue('for_this_server', false)
      return
    }
    setTypingOther(false)
    setValue('municipality_name', choice, { shouldValidate: true })
    setValue('for_this_server', choice === thisServer)
  }

  /** Offers a username when a name is entered and the username is still blank. */
  function suggestFor(prefix: 'accountant' | `staff.${number}`) {
    if (!getValues(`${prefix}.username`)) {
      setValue(`${prefix}.username`, suggestUsername(getValues(`${prefix}.full_name`)), { shouldValidate: true })
    }
  }

  function onGenerate(values: Output) {
    generate.mutate(values, {
      onSuccess: (generated) => { setKey(generated); setStep(2) },
      onError:   (e) => { applyServerErrors(e, setError) },
    })
  }

  function onSave() {
    if (!key) return
    save.mutate(
      { token: key.token, notes: notes.trim() || undefined },
      { onSuccess: (detail) => navigate(`/admin/licences/${detail.id}?saved=1`) },
    )
  }

  const staffError = errors.staff?.message ?? errors.staff?.root?.message

  return (
    <AdminPage
      title="Issue a licence"
      description="Nothing is saved until the last step."
    >
      <Link to="/admin/licences" className="mb-5 inline-flex items-center gap-1.5 text-sm text-ink-secondary hover:text-ink">
        <ArrowLeft className="size-4" aria-hidden />
        Licences
      </Link>

      <Stepper step={step} />

      {capability && !capability.can_issue && (
        <p className="mb-4 rounded-md bg-warn-fill px-3 py-2 text-sm text-warn-ink">{capability.reason}</p>
      )}

      {/* ── Step 1: details ─────────────────────────────────────────────── */}
      {step === 1 && (
        <form onSubmit={handleSubmit(onGenerate)} className="space-y-4">
          <Section title="Municipality">
            <div className="grid grid-cols-2 gap-x-4 gap-y-3">
              <Field label="Municipality" error={typingOther ? undefined : errors.municipality_name?.message}>
                <select
                  value={typingOther ? OTHER : municipality}
                  onChange={(e) => chooseMunicipality(e.target.value)}
                  className={inputClass}
                >
                  <option value="">Choose…</option>
                  {choices.map((m) => (
                    <option key={m} value={m}>{m === thisServer ? `${m} — this server` : m}</option>
                  ))}
                  <option value={OTHER}>Another municipality…</option>
                </select>
              </Field>

              {typingOther && (
                <Field label="Name of the municipality" error={errors.municipality_name?.message}>
                  <input
                    {...register('municipality_name', {
                      onChange: (e) => setValue('for_this_server', e.target.value.trim() === thisServer),
                    })}
                    autoFocus
                    placeholder="Municipality of …"
                    className={inputClass}
                  />
                </Field>
              )}
            </div>

            {municipality && municipality === thisServer && (
              <p className="mt-2 text-xs text-ink-secondary">
                This is the municipality this server runs, so the key can also be installed here.
              </p>
            )}
          </Section>

          <Section title="Municipal Accountant">
            <div className="grid grid-cols-2 gap-x-4 gap-y-3">
              <Field label="Full name" error={errors.accountant?.full_name?.message}>
                <input
                  {...register('accountant.full_name', { onBlur: () => suggestFor('accountant') })}
                  placeholder="Maria Santos"
                  className={inputClass}
                />
              </Field>
              <Field label="Username" error={errors.accountant?.username?.message} hint="What they type to sign in.">
                <input {...register('accountant.username')} placeholder="m.santos" className={`${inputClass} tabular`} />
              </Field>
            </div>
          </Section>

          <Section title="Accounting Staff">
            <div className="space-y-2">
              {staff.map((row, i) => (
                <div key={row.id} className="grid grid-cols-[1fr_1fr_auto] items-start gap-x-4">
                  <Field label={i === 0 ? 'Full name' : ''} error={errors.staff?.[i]?.full_name?.message}>
                    <input
                      {...register(`staff.${i}.full_name`, { onBlur: () => suggestFor(`staff.${i}`) })}
                      placeholder="Juan Reyes"
                      className={inputClass}
                    />
                  </Field>
                  <Field label={i === 0 ? 'Username' : ''} error={errors.staff?.[i]?.username?.message}>
                    <input {...register(`staff.${i}.username`)} placeholder="j.reyes" className={`${inputClass} tabular`} />
                  </Field>
                  <button
                    type="button"
                    onClick={() => remove(i)}
                    disabled={staff.length === 1}
                    aria-label="Remove this person"
                    title={staff.length === 1 ? 'At least one member of the Accounting Staff is needed' : undefined}
                    className={`rounded-pill p-2 text-ink-muted hover:bg-hover hover:text-danger-ink disabled:opacity-30 ${i === 0 ? 'mt-5' : ''}`}
                  >
                    <Trash2 className="size-4" aria-hidden />
                  </button>
                </div>
              ))}
            </div>

            {staffError && <p className="mt-2 text-xs text-danger-ink">{staffError}</p>}

            <button
              type="button"
              onClick={() => append({ full_name: '', username: '' })}
              className="mt-3 flex items-center gap-1.5 rounded-pill px-3 py-1 text-sm text-ink-secondary hover:bg-hover"
            >
              <Plus className="size-4" aria-hidden />
              Add another
            </button>
          </Section>

          <Section title="Licence key">
            <div className="mb-4 grid grid-cols-3 gap-2" role="radiogroup" aria-label="Access">
              {ACCESS_OPTIONS.map((o) => (
                <button
                  key={o.value}
                  type="button"
                  role="radio"
                  aria-checked={access === o.value}
                  onClick={() => setValue('terms.access', o.value, { shouldValidate: true })}
                  className={`rounded-md border px-3 py-2.5 text-left ${
                    access === o.value ? 'border-accent-solid bg-accent-soft' : 'border-line hover:bg-hover'
                  }`}
                >
                  <span className={`block text-sm ${access === o.value ? 'font-medium text-accent-soft-ink' : ''}`}>
                    {o.label}
                  </span>
                  <span className="mt-0.5 block text-xs text-ink-secondary">{o.detail}</span>
                </button>
              ))}
            </div>

            <div className="grid grid-cols-2 gap-x-4 gap-y-3">
              <Field label="Workstations" error={errors.terms?.seats?.message} hint="How many computers may use it.">
                <input type="number" min={1} max={500} {...register('terms.seats')} className={`${inputClass} tabular`} />
              </Field>
              <Field label="Starts on" error={errors.terms?.starts_on?.message}>
                <input type="date" {...register('terms.starts_on')} className={inputClass} />
              </Field>
            </div>
          </Section>

          {generate.error && Object.keys(errors).length === 0 && <ErrorNotice error={generate.error} />}

          <div className="flex items-center justify-end border-t border-line pt-4">
            <button
              type="submit"
              disabled={generate.isPending || !capability?.can_issue}
              className="flex items-center gap-2 rounded-pill bg-accent-fill px-5 py-2 text-sm font-medium text-accent-ink hover:brightness-[0.97] disabled:opacity-60"
            >
              {generate.isPending ? 'Generating…' : 'Next: generate licence'}
              <ArrowRight className="size-4" aria-hidden />
            </button>
          </div>
        </form>
      )}

      {/* ── Step 2: the generated key ───────────────────────────────────── */}
      {step === 2 && key && (
        <div className="space-y-4">
          <Section title="Licence generated">
            <Summary licence={key} />

            <div className="mt-4 flex items-center justify-between gap-4">
              <span className="flex items-center gap-2 text-sm font-medium">
                <KeyRound className="size-4 text-ink-secondary" aria-hidden />
                Licence key
              </span>
              <CopyButton text={key.token} label="Copy licence key" />
            </div>
            <textarea
              readOnly
              value={key.token}
              onFocus={(e) => e.currentTarget.select()}
              rows={5}
              aria-label="Licence key"
              className="mt-2 w-full resize-none rounded-sm border border-line bg-sunken px-3 py-2 font-mono text-xs break-all text-ink-secondary"
            />

            <p className="mt-3 flex items-start gap-2 rounded-md bg-warn-fill px-3 py-2 text-sm text-warn-ink">
              <AlertTriangle className="mt-0.5 size-4 shrink-0" aria-hidden />
              Not saved yet. The key works as soon as it is installed, but there is no profile for it until
              you save it in the next step.
            </p>
          </Section>

          <div className="flex items-center justify-between border-t border-line pt-4">
            <button
              type="button"
              onClick={() => setStep(1)}
              className="flex items-center gap-2 rounded-pill px-4 py-2 text-sm hover:bg-hover"
            >
              <ArrowLeft className="size-4" aria-hidden />
              Change details
            </button>
            <button
              type="button"
              onClick={() => setStep(3)}
              className="flex items-center gap-2 rounded-pill bg-accent-fill px-5 py-2 text-sm font-medium text-accent-ink hover:brightness-[0.97]"
            >
              Next: save profile
              <ArrowRight className="size-4" aria-hidden />
            </button>
          </div>
        </div>
      )}

      {/* ── Step 3: save the profile ────────────────────────────────────── */}
      {step === 3 && key && (
        <div className="space-y-4">
          <Section title="Save profile">
            <Summary licence={key} />

            <div className="mt-4">
              <Field label="Notes (optional)">
                <input
                  value={notes}
                  onChange={(e) => setNotes(e.target.value)}
                  placeholder="Official receipt no., contact person…"
                  className={inputClass}
                />
              </Field>
            </div>

            <p className="mt-3 text-xs text-ink-muted">
              The profile is saved from the key itself — the same key you copied in the previous step.
            </p>
          </Section>

          {save.error && <ErrorNotice error={save.error} />}

          <div className="flex items-center justify-between border-t border-line pt-4">
            <button
              type="button"
              onClick={() => setStep(2)}
              className="flex items-center gap-2 rounded-pill px-4 py-2 text-sm hover:bg-hover"
            >
              <ArrowLeft className="size-4" aria-hidden />
              Back to the key
            </button>
            <button
              type="button"
              onClick={onSave}
              disabled={save.isPending}
              className="flex items-center gap-2 rounded-pill bg-accent-fill px-5 py-2 text-sm font-medium text-accent-ink hover:brightness-[0.97] disabled:opacity-60"
            >
              <Save className="size-4" aria-hidden />
              {save.isPending ? 'Saving…' : 'Save profile'}
            </button>
          </div>
        </div>
      )}
    </AdminPage>
  )
}

/** Where the office is in the three steps. */
function Stepper({ step }: { step: Step }) {
  const steps = ['Details', 'Generate licence', 'Save profile']

  return (
    <ol className="mb-6 flex items-center gap-3" aria-label="Steps">
      {steps.map((label, i) => {
        const n = (i + 1) as Step
        const done = n < step
        const current = n === step
        return (
          <li key={label} className="flex items-center gap-3">
            <span className="flex items-center gap-2">
              <span
                className={`flex size-7 items-center justify-center rounded-pill text-xs font-medium ${
                  done ? 'bg-ok-fill text-ok-ink' : current ? 'bg-accent-fill text-accent-ink' : 'bg-sunken text-ink-muted'
                }`}
              >
                {done ? <Check className="size-3.5" aria-hidden /> : n}
              </span>
              <span className={`text-sm ${current ? 'font-medium' : 'text-ink-secondary'}`}>{label}</span>
            </span>
            {i < steps.length - 1 && <span className="h-px w-10 bg-line-strong" aria-hidden />}
          </li>
        )
      })}
    </ol>
  )
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="rounded-md border border-line bg-surface p-4">
      <h2 className="mb-3 font-medium">{title}</h2>
      {children}
    </section>
  )
}

/** What a generated key grants, and to whom. */
function Summary({ licence }: { licence: GeneratedKey }) {
  const accountant = licence.people.find((p) => p.role === 'ACCOUNTANT')
  const staff = licence.people.filter((p) => p.role === 'STAFF')

  return (
    <dl className="grid grid-cols-[auto_1fr] gap-x-6 gap-y-1.5 text-sm">
      <dt className="text-ink-muted">Municipality</dt>
      <dd>
        {licence.municipality_name}
        {licence.for_this_server && (
          <span className="ml-2 rounded-pill bg-accent-soft px-2 py-0.5 text-xs text-accent-soft-ink">This server</span>
        )}
      </dd>

      <dt className="text-ink-muted">Access</dt>
      <dd>
        {accessLabel(licence.access)}
        {licence.access === 'lifetime'
          ? ` — from ${formatDate(licence.not_before)}, never expires`
          : ` — ${formatDate(licence.not_before)} to ${formatExpiry(licence.expires_at)}, then ${licence.grace_days} days' grace`}
      </dd>

      <dt className="text-ink-muted">Workstations</dt>
      <dd className="tabular">{licence.seats}</dd>

      <dt className="text-ink-muted">Municipal Accountant</dt>
      <dd>{accountant ? `${accountant.full_name} (${accountant.username})` : '—'}</dd>

      <dt className="text-ink-muted">Accounting Staff</dt>
      <dd>{staff.map((p) => `${p.full_name} (${p.username})`).join(', ')}</dd>
    </dl>
  )
}
