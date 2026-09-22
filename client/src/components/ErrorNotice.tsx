import { AlertCircle, WifiOff } from 'lucide-react'
import { ApiError } from '@/lib/api'

interface ErrorNoticeProps {
  error: unknown
  /** Shown when the error is not one we recognise. */
  fallback?: string
}

/**
 * The single way an error is shown to a user.
 *
 * Takes whatever was thrown and renders something a municipal accounting clerk
 * can act on. An unrecognised error never has its raw text displayed — the
 * message could be a stack trace, a SQL fragment, or a type name, and none of
 * those help the person reading it.
 */
export function ErrorNotice({ error, fallback }: ErrorNoticeProps) {
  const known = error instanceof ApiError ? error : null
  const offline = known?.code === 'NODE_UNREACHABLE'

  const message =
    known?.message ??
    fallback ??
    'Something went wrong and this could not be completed.'

  const hint =
    known?.hint ??
    (known ? undefined : 'Nothing was saved. Try again in a moment.')

  const Icon = offline ? WifiOff : AlertCircle

  return (
    <div
      role="alert"
      className={`flex gap-3 rounded-md px-4 py-3 ${
        offline
          ? 'bg-info-fill text-info-ink'
          : 'bg-danger-fill text-danger-ink'
      }`}
    >
      <Icon className="mt-0.5 size-[18px] shrink-0" aria-hidden />
      <div className="min-w-0">
        <p className="font-medium">{message}</p>
        {hint && <p className="mt-1 text-sm opacity-90">{hint}</p>}
        {known && known.reference !== 'LOCAL' && (
          <p className="mt-1.5 text-xs opacity-75">Reference {known.reference}</p>
        )}
      </div>
    </div>
  )
}
