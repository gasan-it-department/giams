import { useState } from 'react'
import { useForm } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import { KeyRound, Loader2 } from 'lucide-react'
import { z } from 'zod'
import { ErrorNotice } from '@/components/ErrorNotice'
import { api } from '@/lib/api'
import { useAuthStore } from '@/stores/authStore'

/**
 * The forced password change.
 *
 * Stands in front of the entire application, not beside it. An account created
 * by an administrator, or reset after a lockout, carries a password that a
 * second person has seen — written on a slip of paper, or read aloud over the
 * phone. Until it is replaced, the person signed in is not provably the person
 * the account belongs to, and every audit record attributed to them is weaker
 * for it.
 *
 * So there is no way past this screen except changing the password or signing
 * out. A prompt that can be dismissed is a prompt that is always dismissed.
 */

const schema = z
  .object({
    current: z.string().min(1, 'Enter the password you signed in with.'),
    next: z
      .string()
      .min(12, 'Use at least 12 characters. A short password on a finance system is not enough.')
      .max(200, 'That is longer than the system can store.'),
    confirm: z.string().min(1, 'Type the new password again.'),
  })
  .refine((v) => v.next === v.confirm, {
    path: ['confirm'],
    message: 'The two new passwords do not match.',
  })
  .refine((v) => v.next !== v.current, {
    path: ['next'],
    message: 'The new password must be different from the one you signed in with.',
  })

type Fields = z.infer<typeof schema>

export function ChangePassword() {
  const user = useAuthStore((s) => s.user)
  const signOut = useAuthStore((s) => s.signOut)
  const [failure, setFailure] = useState<unknown>(null)

  const {
    register,
    handleSubmit,
    formState: { errors, isSubmitting },
  } = useForm<Fields>({ resolver: zodResolver(schema) })

  async function submit(values: Fields) {
    setFailure(null)
    try {
      await api.post('/api/auth/password', {
        current_password: values.current,
        new_password: values.next,
      })

      // Cleared locally so the gate lifts without another round trip. The
      // server has already recorded the change.
      useAuthStore.setState((s) =>
        s.user ? { user: { ...s.user, must_change_password: false } } : s,
      )
    } catch (e) {
      setFailure(e)
    }
  }

  return (
    <div className="flex min-h-screen items-center justify-center bg-canvas px-4">
      <div className="w-full max-w-[26rem] rounded-lg border border-line bg-surface p-8">
        <KeyRound className="size-6 text-ink-secondary" aria-hidden />

        <h1 className="mt-3 text-xl">Choose a new password</h1>
        <p className="mt-2 text-sm text-ink-secondary">
          {user?.full_name ? `${user.full_name}, this ` : 'This '}
          account is still using the password it was given. Choose one only you
          know before going any further.
        </p>

        <form className="mt-6 space-y-4" onSubmit={handleSubmit(submit)} noValidate>
          <Field
            label="The password you signed in with"
            error={errors.current?.message}
            {...register('current')}
          />
          <Field
            label="New password"
            error={errors.next?.message}
            hint="At least 12 characters. A phrase you can remember is stronger than a short word with symbols in it."
            {...register('next')}
          />
          <Field
            label="New password again"
            error={errors.confirm?.message}
            {...register('confirm')}
          />

          {failure != null && (
            <ErrorNotice error={failure} fallback="The password could not be changed." />
          )}

          <button
            type="submit"
            disabled={isSubmitting}
            className="flex w-full items-center justify-center gap-2 rounded-pill bg-accent-fill px-4 py-2.5 font-medium text-accent-ink hover:brightness-[0.97] disabled:opacity-60"
          >
            {isSubmitting && <Loader2 className="size-4 animate-spin" aria-hidden />}
            {isSubmitting ? 'Saving…' : 'Save and continue'}
          </button>
        </form>

        <button
          type="button"
          onClick={() => void signOut()}
          className="mt-4 w-full rounded-pill px-4 py-2 text-sm text-ink-secondary hover:bg-hover"
        >
          Sign out instead
        </button>
      </div>
    </div>
  )
}

/** A labelled password box, with its message underneath. */
const Field = ({
  label,
  error,
  hint,
  ...props
}: React.InputHTMLAttributes<HTMLInputElement> & {
  label: string
  error?: string
  hint?: string
}) => (
  <label className="block">
    <span className="text-sm text-ink-secondary">{label}</span>
    <input
      type="password"
      autoComplete="new-password"
      {...props}
      className={`mt-1 w-full rounded-md border bg-surface px-3 py-2 outline-none focus:border-accent-solid ${
        error ? 'border-danger-ink' : 'border-line'
      }`}
    />
    {error ? (
      <span className="mt-1 block text-sm text-danger-ink">{error}</span>
    ) : hint ? (
      <span className="mt-1 block text-xs text-ink-muted">{hint}</span>
    ) : null}
  </label>
)
