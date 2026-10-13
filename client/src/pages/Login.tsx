import { useState, type InputHTMLAttributes } from 'react'
import { useForm } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import { Building2, Loader2, ShieldCheck } from 'lucide-react'
import { ErrorNotice } from '@/components/ErrorNotice'
import { ApiError } from '@/lib/api'
import { loginSchema, type LoginInput } from '@/schemas/auth'
import { useAuthStore } from '@/stores/authStore'

/**
 * Sign-in.
 *
 * Field-level messages come from Zod before the round trip; whole-form
 * failures come from the server and are shown verbatim, because the server
 * already phrases them for a clerk (`SIGN_IN_FAILED`, `ACCOUNT_LOCKED`).
 *
 * The same accounts sign in to both front doors; `admin` only changes what
 * the page says it is, so nobody mistakes the administration console for the
 * accounting system.
 */
export function Login({ variant = 'accounting' }: { variant?: 'accounting' | 'admin' }) {
  const admin = variant === 'admin'
  const signIn = useAuthStore((s) => s.signIn)
  const [failure, setFailure] = useState<unknown>(null)

  const {
    register,
    handleSubmit,
    setError,
    formState: { errors, isSubmitting },
  } = useForm<LoginInput>({
    resolver: zodResolver(loginSchema),
    defaultValues: { username: '', password: '' },
  })

  async function onSubmit(values: LoginInput) {
    setFailure(null)
    try {
      await signIn(values.username, values.password)
    } catch (e) {
      // The server may attribute a failure to specific fields; if it does,
      // show it against those fields rather than as a banner.
      if (e instanceof ApiError && Object.keys(e.fields).length > 0) {
        for (const [field, message] of Object.entries(e.fields)) {
          setError(field as keyof LoginInput, { message })
        }
        return
      }
      setFailure(e)
    }
  }

  return (
    <main className="flex min-h-screen items-center justify-center bg-canvas px-4">
      <div className="w-full max-w-[400px]">
        <div className="mb-8 flex flex-col items-center text-center">
          <div className="mb-4 flex size-14 items-center justify-center rounded-lg bg-accent-fill text-accent-ink">
            {admin
              ? <ShieldCheck className="size-7" aria-hidden />
              : <Building2 className="size-7" aria-hidden />}
          </div>
          <h1 className="text-xl font-medium">{admin ? 'GIAMS Administration' : 'Accounting Management System'}</h1>
          <p className="mt-1 text-ink-secondary">
            {admin ? 'Licence keys and user accounts' : 'Municipality of Gasan'}
          </p>
        </div>

        <form
          onSubmit={handleSubmit(onSubmit)}
          noValidate
          className="rounded-lg bg-surface p-6"
        >
          {failure != null && (
            <div className="mb-4">
              <ErrorNotice error={failure} />
            </div>
          )}

          <Field
            label="Username"
            error={errors.username?.message}
            {...register('username')}
            autoComplete="username"
            autoFocus
          />

          <Field
            label="Password"
            type="password"
            error={errors.password?.message}
            {...register('password')}
            autoComplete="current-password"
          />

          <button
            type="submit"
            disabled={isSubmitting}
            className="mt-6 flex w-full items-center justify-center gap-2 rounded-pill bg-accent-fill px-4 py-3 font-medium text-accent-ink hover:brightness-[0.97] disabled:opacity-60"
          >
            {isSubmitting && <Loader2 className="size-4 animate-spin" aria-hidden />}
            {isSubmitting ? 'Signing in…' : 'Sign in'}
          </button>
        </form>

        <p className="mt-6 text-center text-sm text-ink-muted">
          If you cannot sign in, contact the Information Technology office.
        </p>
      </div>
    </main>
  )
}

interface FieldProps extends InputHTMLAttributes<HTMLInputElement> {
  label: string
  error?: string
}

/**
 * A labelled input.
 *
 * `aria-invalid` and `aria-describedby` are wired up so a screen reader
 * announces the message, and the error is placed after the input so the
 * reading order matches the visual one.
 */
function Field({ label, error, id, ...props }: FieldProps) {
  const inputId = id ?? props.name ?? label.toLowerCase()
  const errorId = `${inputId}-error`

  return (
    <div className="mt-4 first:mt-0">
      <label htmlFor={inputId} className="mb-1.5 block font-medium">
        {label}
      </label>
      <input
        id={inputId}
        aria-invalid={error ? true : undefined}
        aria-describedby={error ? errorId : undefined}
        className={`w-full rounded-md bg-sunken px-3.5 py-2.5 text-ink placeholder:text-ink-muted ${
          error ? 'ring-1 ring-danger-ink' : ''
        }`}
        {...props}
      />
      {error && (
        <p id={errorId} className="mt-1.5 text-sm text-danger-ink">
          {error}
        </p>
      )}
    </div>
  )
}
