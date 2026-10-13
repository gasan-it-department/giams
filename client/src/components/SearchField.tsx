import { useEffect, useState } from 'react'
import { Search, X } from 'lucide-react'

/**
 * A screen's own search box.
 *
 * Each register searches what it lists — accounts by code and title, vouchers
 * by number and payee — so the box says so in its placeholder instead of
 * promising one search for everything.
 */
export function SearchField({
  value, onChange, placeholder, className = '',
}: {
  value: string
  onChange: (value: string) => void
  placeholder: string
  className?: string
}) {
  return (
    <div className={`flex min-w-0 items-center gap-2 rounded-pill bg-search px-3 py-1.5 ${className}`}>
      <Search className="size-4 shrink-0 text-ink-secondary" aria-hidden />
      <input
        type="search"
        value={value}
        onChange={(e) => onChange(e.target.value)}
        onKeyDown={(e) => { if (e.key === 'Escape') onChange('') }}
        placeholder={placeholder}
        aria-label={placeholder}
        className="min-w-0 flex-1 bg-transparent text-sm text-ink placeholder:text-ink-muted focus:outline-none [&::-webkit-search-cancel-button]:hidden"
      />
      {value && (
        <button
          type="button"
          onClick={() => onChange('')}
          aria-label="Clear search"
          className="rounded-pill p-0.5 text-ink-muted hover:bg-hover"
        >
          <X className="size-3.5" aria-hidden />
        </button>
      )}
    </div>
  )
}

/** The value once typing has paused, so the server is not asked per keystroke. */
export function useDebounced<T>(value: T, delay = 250): T {
  const [settled, setSettled] = useState(value)

  useEffect(() => {
    const timer = setTimeout(() => setSettled(value), delay)
    return () => clearTimeout(timer)
  }, [value, delay])

  return settled
}
