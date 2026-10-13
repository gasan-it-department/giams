import { useState } from 'react'
import { Check, Copy, KeyRound } from 'lucide-react'

/**
 * Copies text, and says so. Falls back to a hidden field where the clipboard
 * API is unavailable, so the button never silently does nothing.
 */
export function CopyButton({ text, label = 'Copy' }: { text: string; label?: string }) {
  const [copied, setCopied] = useState(false)

  async function copy() {
    try {
      await navigator.clipboard.writeText(text)
    } catch {
      const area = document.createElement('textarea')
      area.value = text
      area.style.position = 'fixed'
      area.style.opacity = '0'
      document.body.append(area)
      area.select()
      document.execCommand('copy')
      area.remove()
    }
    setCopied(true)
    window.setTimeout(() => setCopied(false), 2000)
  }

  return (
    <button
      type="button"
      onClick={() => void copy()}
      className="flex shrink-0 items-center gap-1.5 rounded-pill bg-sunken px-3 py-1 text-xs font-medium hover:bg-hover"
    >
      {copied ? <Check className="size-3.5 text-ok-ink" aria-hidden /> : <Copy className="size-3.5" aria-hidden />}
      {copied ? 'Copied' : label}
    </button>
  )
}

/**
 * One-time passwords, shown once.
 *
 * Said plainly on the screen because it is true and it matters: the password
 * is not stored anywhere readable, so this is the last chance to copy it, and
 * the person must choose their own when they first sign in.
 */
export function OneTimePasswords({
  title, people, onDone,
}: {
  title: string
  people: { full_name: string; username: string; password: string }[]
  onDone?: () => void
}) {
  return (
    <section className="mb-5 rounded-md border border-line bg-ok-fill p-4">
      <h2 className="flex items-center gap-2 font-medium text-ok-ink">
        <KeyRound className="size-[18px]" aria-hidden />
        {title}
      </h2>
      <p className="mt-1 text-sm text-ink-secondary">
        These passwords are shown once and are not stored anywhere you can read them again.
        Copy them now and hand them over; each person must choose their own at first sign-in.
      </p>

      <table className="mt-3 w-full text-sm">
        <thead>
          <tr className="text-left text-xs text-ink-muted">
            <th className="py-1 pr-3 font-normal">Name</th>
            <th className="py-1 pr-3 font-normal">Username</th>
            <th className="py-1 pr-3 font-normal">One-time password</th>
            <th className="py-1" />
          </tr>
        </thead>
        <tbody>
          {people.map((p) => (
            <tr key={p.username} className="border-t border-line">
              <td className="py-1.5 pr-3">{p.full_name}</td>
              <td className="py-1.5 pr-3"><code className="tabular">{p.username}</code></td>
              <td className="py-1.5 pr-3"><code className="tabular font-medium">{p.password}</code></td>
              <td className="py-1.5 text-right">
                <CopyButton text={`Username: ${p.username}\nPassword: ${p.password}`} />
              </td>
            </tr>
          ))}
        </tbody>
      </table>

      {onDone && (
        <button
          type="button"
          onClick={onDone}
          className="mt-3 rounded-pill bg-surface px-4 py-1.5 text-sm font-medium hover:bg-hover"
        >
          I have copied these
        </button>
      )}
    </section>
  )
}
