/**
 * The heading block every COA form carries.
 *
 * Not decoration: it is what makes a printed page identifiable once it leaves
 * the screen and goes into a folder, which is where these end up. The annex
 * number sits in the corner exactly as it does on the paper form, so a clerk
 * can tell at a glance which form they are holding.
 */
export function FormHeading({
  annex, title, municipality, lines,
}: {
  annex?: string
  title: string
  municipality: string
  lines: string[]
}) {
  return (
    <div className="relative border-b border-line px-6 py-4 text-center">
      {annex && <p className="absolute left-6 top-4 text-xs text-ink-muted">{annex}</p>}
      <p className="text-sm">{municipality}</p>
      <p className="mt-0.5 font-medium tracking-wide">{title}</p>
      {lines.map((l) => (
        <p key={l} className="mt-0.5 text-sm text-ink-secondary">{l}</p>
      ))}
    </div>
  )
}
