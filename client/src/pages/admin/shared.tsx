import type { ReactNode } from 'react'
import type { FieldValues, Path, UseFormSetError } from 'react-hook-form'
import { ApiError } from '@/lib/api'
import { isLifetime } from '@/schemas/admin'

/**
 * Pieces shared by the admin console's pages.
 */

export const inputClass =
  'w-full rounded-sm border border-line bg-canvas px-2.5 py-1.5 outline-none focus:border-accent-solid'

/** The frame every page of the console sits in: its title, and its actions. */
export function AdminPage({
  title, description, actions, children,
}: {
  title: string
  description?: string
  actions?: ReactNode
  children: ReactNode
}) {
  return (
    <div className="mx-auto max-w-[1040px] px-6 py-6">
      <div className="mb-5 flex items-start justify-between gap-4">
        <div>
          <h1 className="text-lg font-medium">{title}</h1>
          {description && <p className="mt-0.5 text-sm text-ink-muted">{description}</p>}
        </div>
        {actions}
      </div>
      {children}
    </div>
  )
}

/** Shown to someone who opened an admin page without the permission for it. */
export function NoAccess() {
  return (
    <div className="rounded-md border border-line bg-sunken px-6 py-10 text-center">
      <p className="font-medium">You do not have access to this.</p>
      <p className="mt-1 text-sm text-ink-secondary">Ask an administrator if you need it for your work.</p>
    </div>
  )
}

export function Field({
  label, error, hint, children,
}: {
  label: string
  error?: string
  hint?: string
  children: ReactNode
}) {
  return (
    <label className="block">
      <span className="mb-1 block text-xs text-ink-muted">{label}</span>
      {children}
      {error
        ? <span className="mt-1 block text-xs text-danger-ink">{error}</span>
        : hint && <span className="mt-1 block text-xs text-ink-muted">{hint}</span>}
    </label>
  )
}

export { CopyButton, OneTimePasswords } from '@/components/OneTimePasswords'

/**
 * Puts the server's per-field refusals on the fields they name.
 *
 * The server reports paths the form uses too (`staff.0.username`), so a
 * username already taken is highlighted where it was typed. Returns whether
 * anything was placed, so the caller can fall back to a banner.
 */
export function applyServerErrors<T extends FieldValues>(
  error: unknown,
  setError: UseFormSetError<T>,
): boolean {
  if (!(error instanceof ApiError)) return false
  const entries = Object.entries(error.fields)
  for (const [path, message] of entries) {
    setError(path as Path<T>, { type: 'server', message })
  }
  return entries.length > 0
}

export function formatDate(value: string | null | undefined): string {
  if (!value) return '—'
  return new Date(value).toLocaleDateString('en-PH', { dateStyle: 'medium' })
}

/** A key's end, as the office reads it: a date, or "Lifetime". */
export function formatExpiry(value: string | null | undefined): string {
  return isLifetime(value) ? 'Lifetime' : formatDate(value)
}

export function formatDateTime(value: string | null | undefined): string {
  if (!value) return 'Never'
  return new Date(value).toLocaleString('en-PH', { dateStyle: 'medium', timeStyle: 'short' })
}
