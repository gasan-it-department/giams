import { useState } from 'react'
import { useForm } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import { Landmark, Plus, X } from 'lucide-react'
import { ErrorNotice } from '@/components/ErrorNotice'
import { formatAmount, type Amount } from '@/lib/money'
import { AccountSelect } from '@/components/AccountSelect'
import { useFunds, useOffices } from '@/queries/accounts'
import {
  useAppropriations, useCreateAllotment, useCreateAppropriation,
  type Appropriation,
} from '@/queries/budget'
import {
  APPROPRIATION_TYPES, APPROPRIATION_TYPE_LABELS,
  newAllotmentSchema, newAppropriationSchema,
  type NewAllotmentForm, type NewAppropriationForm,
} from '@/schemas/budget'
import { EXPENSE_CLASSES, EXPENSE_CLASS_LABELS, type ExpenseClass } from '@/schemas/accounts'
import { useAuthStore } from '@/stores/authStore'
import { usePeriodRange } from '@/stores/periodStore'

/**
 * Budget — appropriations and allotments.
 *
 * The front door to budget execution: the amount the Sanggunian authorised by
 * ordinance, entered line by line, and the releases the Budget Officer makes
 * against it. Everything downstream — obligations, vouchers, payments — is
 * measured against what is recorded here, so until an appropriation exists a
 * clerk cannot commit a single peso.
 *
 * Recording is the Budget Officer's, not the administrator's: the whole point
 * of the role split is that the person who authorises spending is not the
 * person who manages the system. So the forms appear only for `budget.manage`;
 * everyone else sees the budget but cannot change it.
 */
export function Budget() {
  const can = useAuthStore((s) => s.can)
  const period = usePeriodRange()
  const year = period.fiscalYear
  const canManage = can('budget.manage')

  const { data: appropriations, isPending, error } = useAppropriations(year)

  const [recording, setRecording] = useState(false)
  const [releaseFor, setReleaseFor] = useState<Appropriation | null>(null)

  return (
    <div className="min-h-0 flex-1 overflow-auto">
      <div className="mx-auto max-w-[1000px] px-6 py-5">
        <div className="mb-4 flex items-center justify-between">
          <div>
            <h1 className="flex items-center gap-2 text-lg font-medium">
              <Landmark className="size-5 text-ink-secondary" aria-hidden />
              Budget
            </h1>
            <p className="text-sm text-ink-muted">
              Appropriations and allotments for fiscal year {year}.
            </p>
          </div>

          {canManage && !recording && (
            <button
              type="button"
              onClick={() => setRecording(true)}
              className="flex items-center gap-2 rounded-pill bg-accent-fill px-4 py-2 text-sm font-medium text-accent-ink hover:brightness-[0.97]"
            >
              <Plus className="size-4" aria-hidden />
              Record appropriation
            </button>
          )}
        </div>

        {error && <div className="mb-4"><ErrorNotice error={error} /></div>}

        {recording && (
          <AppropriationForm year={year} onDone={() => setRecording(false)} />
        )}

        {releaseFor && (
          <AllotmentForm
            appropriation={releaseFor}
            onDone={() => setReleaseFor(null)}
          />
        )}

        {isPending && <p className="py-6 text-ink-muted">Loading…</p>}

        {appropriations?.length === 0 && !recording && (
          <div className="rounded-md border border-line bg-sunken px-6 py-10 text-center">
            <p className="font-medium">No appropriations recorded for {year} yet.</p>
            <p className="mt-1 text-sm text-ink-secondary">
              {canManage
                ? 'Record the annual appropriation ordinance to begin. Obligations and vouchers can only be charged once the budget is on record.'
                : 'The Budget Officer records the annual appropriation ordinance. Nothing can be obligated until it is entered.'}
            </p>
          </div>
        )}

        {appropriations && appropriations.length > 0 && (
          <AppropriationTable
            rows={appropriations}
            canManage={canManage}
            onRelease={setReleaseFor}
          />
        )}
      </div>
    </div>
  )
}

// ── The list ─────────────────────────────────────────────────────────────────

function AppropriationTable({
  rows, canManage, onRelease,
}: {
  rows: Appropriation[]
  canManage: boolean
  onRelease: (a: Appropriation) => void
}) {
  return (
    <table className="w-full text-sm">
      <thead>
        <tr className="border-b border-line-strong text-left text-xs text-ink-muted">
          <th className="py-2 pr-3 font-normal">Ordinance</th>
          <th className="py-2 pr-3 font-normal">Office</th>
          <th className="py-2 pr-3 font-normal">Account</th>
          <th className="py-2 pr-3 font-normal">Class</th>
          <th className="py-2 pr-3 text-right font-normal">Appropriated</th>
          <th className="py-2 pr-3 text-right font-normal">Released</th>
          <th className="py-2 pr-3 text-right font-normal">Unreleased</th>
          {canManage && <th className="py-2" />}
        </tr>
      </thead>
      <tbody>
        {rows.map((a) => (
          <tr key={a.id} className="border-b border-line align-top">
            <td className="py-2 pr-3">
              <span className="tabular">{a.ordinance_no}</span>
              <span className="block text-xs text-ink-muted">
                {APPROPRIATION_TYPE_LABELS[a.appropriation_type as keyof typeof APPROPRIATION_TYPE_LABELS] ?? a.appropriation_type}
                {' · '}{a.ordinance_date}
              </span>
            </td>
            <td className="py-2 pr-3">{a.office_name}</td>
            <td className="py-2 pr-3">
              <code className="tabular text-xs text-ink-secondary">{a.account_code}</code>
              <span className="block">{a.account_name}</span>
            </td>
            <td className="py-2 pr-3 text-ink-secondary">{a.expense_class}</td>
            <td className="py-2 pr-3 text-right tabular">{formatAmount(a.amount)}</td>
            <td className="py-2 pr-3 text-right tabular text-ink-secondary">
              {formatAmount(a.released_amount)}
            </td>
            <td className="py-2 pr-3 text-right tabular font-medium">
              {formatAmount(a.unreleased_amount)}
            </td>
            {canManage && (
              <td className="py-2 text-right">
                <button
                  type="button"
                  onClick={() => onRelease(a)}
                  disabled={a.unreleased_amount === '0.00'}
                  title={a.unreleased_amount === '0.00' ? 'Fully released' : undefined}
                  className="rounded-pill bg-sunken px-3 py-1 text-xs font-medium hover:bg-hover disabled:opacity-40"
                >
                  Release
                </button>
              </td>
            )}
          </tr>
        ))}
      </tbody>
    </table>
  )
}

// ── Recording an appropriation ───────────────────────────────────────────────

function AppropriationForm({ year, onDone }: { year: number; onDone: () => void }) {
  const { data: funds } = useFunds()
  const { data: offices } = useOffices()
  const create = useCreateAppropriation()

  const {
    register, handleSubmit, setValue, watch,
    formState: { errors },
  } = useForm<NewAppropriationForm>({
    resolver: zodResolver(newAppropriationSchema),
    defaultValues: {
      fiscal_year:        year,
      appropriation_type: 'annual',
      ordinance_date:     `${year}-01-01`,
      fund_id:            '',
      office_id:          '',
      account_id:         '',
      amount:             '',
      particulars:        '',
    },
  })

  const accountId = watch('account_id')

  async function submit(values: NewAppropriationForm) {
    await create.mutateAsync({
      ...values,
      amount: values.amount as Amount,
      particulars: values.particulars?.trim() ? values.particulars : undefined,
    })
    onDone()
  }

  return (
    <form
      onSubmit={handleSubmit(submit)}
      className="mb-5 rounded-md border border-line bg-sunken p-4"
    >
      <div className="mb-3 flex items-center justify-between">
        <h2 className="font-medium">Record an appropriation</h2>
        <button type="button" onClick={onDone} aria-label="Cancel" className="rounded-pill p-1 hover:bg-hover">
          <X className="size-[18px]" aria-hidden />
        </button>
      </div>

      <div className="grid grid-cols-2 gap-x-4 gap-y-3">
        <Field label="Fund" error={errors.fund_id?.message}>
          <select {...register('fund_id')} className={selectClass}>
            <option value="">Choose…</option>
            {funds?.map((f) => <option key={f.id} value={f.id}>{f.name}</option>)}
          </select>
        </Field>

        <Field label="Office" error={errors.office_id?.message}>
          <select {...register('office_id')} className={selectClass}>
            <option value="">Choose…</option>
            {offices?.map((o) => <option key={o.id} value={o.id}>{o.name}</option>)}
          </select>
        </Field>

        <div className="col-span-2">
          <Field label="Account" error={errors.account_id?.message}>
            {/* Expense accounts only: an appropriation against a cash account
                is a data-entry error the picker should not offer. */}
            <AccountSelect
              accountType="expense"
              placeholder="Choose an expense account"
              value={accountId}
              invalid={!!errors.account_id}
              onChange={(a) => {
                setValue('account_id', a.id, { shouldValidate: true })
                if (a.expense_class) {
                  setValue('expense_class', a.expense_class as ExpenseClass, { shouldValidate: true })
                }
              }}
            />
          </Field>
        </div>

        <Field label="Expense class" error={errors.expense_class?.message}>
          <select {...register('expense_class')} className={selectClass} defaultValue="">
            <option value="" disabled>Choose…</option>
            {EXPENSE_CLASSES.map((c) => (
              <option key={c} value={c}>{c} — {EXPENSE_CLASS_LABELS[c]}</option>
            ))}
          </select>
        </Field>

        <Field label="Type" error={errors.appropriation_type?.message}>
          <select {...register('appropriation_type')} className={selectClass}>
            {APPROPRIATION_TYPES.map((t) => (
              <option key={t} value={t}>{APPROPRIATION_TYPE_LABELS[t]}</option>
            ))}
          </select>
        </Field>

        <Field label="Ordinance no." error={errors.ordinance_no?.message}>
          <input {...register('ordinance_no')} placeholder="e.g. 2026-01" className={inputClass} />
        </Field>

        <Field label="Ordinance date" error={errors.ordinance_date?.message}>
          <input type="date" {...register('ordinance_date')} className={inputClass} />
        </Field>

        <Field label="Amount" error={errors.amount?.message}>
          <input
            {...register('amount')}
            inputMode="decimal"
            placeholder="0.00"
            className={`${inputClass} text-right tabular`}
          />
        </Field>

        <div className="col-span-2">
          <Field label="Particulars (optional)" error={errors.particulars?.message}>
            <input {...register('particulars')} placeholder="What this appropriation is for" className={inputClass} />
          </Field>
        </div>
      </div>

      {create.error && <div className="mt-3"><ErrorNotice error={create.error} /></div>}

      <div className="mt-4 flex items-center gap-2">
        <button
          type="submit"
          disabled={create.isPending}
          className="rounded-pill bg-accent-fill px-4 py-2 text-sm font-medium text-accent-ink hover:brightness-[0.97] disabled:opacity-60"
        >
          {create.isPending ? 'Saving…' : 'Record appropriation'}
        </button>
        <button type="button" onClick={onDone} className="rounded-pill px-4 py-2 text-sm hover:bg-hover">
          Cancel
        </button>
      </div>
    </form>
  )
}

// ── Releasing an allotment ───────────────────────────────────────────────────

function AllotmentForm({
  appropriation, onDone,
}: {
  appropriation: Appropriation
  onDone: () => void
}) {
  const create = useCreateAllotment()

  const {
    register, handleSubmit,
    formState: { errors },
  } = useForm<NewAllotmentForm>({
    resolver: zodResolver(newAllotmentSchema),
    defaultValues: {
      appropriation_id: appropriation.id,
      release_date:     new Date().toISOString().slice(0, 10),
      amount:           '',
    },
  })

  async function submit(values: NewAllotmentForm) {
    await create.mutateAsync({ ...values, amount: values.amount as Amount })
    onDone()
  }

  return (
    <form
      onSubmit={handleSubmit(submit)}
      className="mb-5 rounded-md border border-line bg-accent-soft p-4"
    >
      <div className="mb-3 flex items-center justify-between">
        <h2 className="font-medium">Release an allotment</h2>
        <button type="button" onClick={onDone} aria-label="Cancel" className="rounded-pill p-1 hover:bg-hover">
          <X className="size-[18px]" aria-hidden />
        </button>
      </div>

      <p className="mb-3 text-sm text-ink-secondary">
        From <span className="font-medium text-ink">{appropriation.account_name}</span>{' '}
        ({appropriation.expense_class}) under ordinance{' '}
        <span className="tabular">{appropriation.ordinance_no}</span>.{' '}
        Unreleased balance:{' '}
        <span className="tabular font-medium text-ink">
          {formatAmount(appropriation.unreleased_amount)}
        </span>.
      </p>

      <div className="grid grid-cols-2 gap-x-4 gap-y-3">
        <Field label="Release date" error={errors.release_date?.message}>
          <input type="date" {...register('release_date')} className={inputClass} />
        </Field>

        <Field label="Amount" error={errors.amount?.message}>
          <input
            {...register('amount')}
            inputMode="decimal"
            placeholder="0.00"
            className={`${inputClass} text-right tabular`}
          />
        </Field>
      </div>

      {create.error && <div className="mt-3"><ErrorNotice error={create.error} /></div>}

      <div className="mt-4 flex items-center gap-2">
        <button
          type="submit"
          disabled={create.isPending}
          className="rounded-pill bg-accent-fill px-4 py-2 text-sm font-medium text-accent-ink hover:brightness-[0.97] disabled:opacity-60"
        >
          {create.isPending ? 'Releasing…' : 'Release allotment'}
        </button>
        <button type="button" onClick={onDone} className="rounded-pill px-4 py-2 text-sm hover:bg-hover">
          Cancel
        </button>
      </div>
    </form>
  )
}

// ── Small shared pieces ──────────────────────────────────────────────────────

const inputClass = 'w-full rounded-sm border border-line bg-canvas px-2 py-1'
const selectClass = 'w-full rounded-sm border border-line bg-canvas px-2 py-1'

function Field({
  label, error, children,
}: {
  label: string
  error?: string
  children: React.ReactNode
}) {
  return (
    <label className="block">
      <span className="mb-1 block text-xs text-ink-muted">{label}</span>
      {children}
      {error && <span className="mt-1 block text-xs text-danger-ink">{error}</span>}
    </label>
  )
}
