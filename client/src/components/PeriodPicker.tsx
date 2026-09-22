import { CalendarRange, ChevronLeft, ChevronRight } from 'lucide-react'
import {
  divisions, usePeriodRange, usePeriodStore, type PeriodBasis,
} from '@/stores/periodStore'

/**
 * The period every screen reports on.
 *
 * Sits in the top bar rather than on each page, because it applies to all of
 * them. Showing it once is also what makes it obvious that the figures on
 * screen are for a period and not for all time — a clerk who cannot see which
 * month they are looking at will eventually read March's figures as the year's.
 */

const BASES: Array<{ key: PeriodBasis; label: string }> = [
  { key: 'annual',   label: 'Year' },
  { key: 'semester', label: 'Semester' },
  { key: 'quarter',  label: 'Quarter' },
  { key: 'month',    label: 'Month' },
]

export function PeriodPicker() {
  const basis = usePeriodStore((s) => s.basis)
  const year = usePeriodStore((s) => s.year)
  const index = usePeriodStore((s) => s.index)
  const { setBasis, setYear, setIndex, step } = usePeriodStore()
  const range = usePeriodRange()

  const count = divisions(basis)

  return (
    <div className="flex items-center gap-1 rounded-pill bg-search py-1 pl-3 pr-1">
      <CalendarRange className="size-4 shrink-0 text-ink-secondary" aria-hidden />

      <select
        value={basis}
        onChange={(e) => setBasis(e.target.value as PeriodBasis)}
        aria-label="Reporting period type"
        className="cursor-pointer border-0 bg-transparent py-0.5 text-sm text-ink-secondary focus:outline-none"
      >
        {BASES.map((b) => (
          <option key={b.key} value={b.key}>{b.label}</option>
        ))}
      </select>

      <button
        type="button"
        onClick={() => step(-1)}
        aria-label="Previous period"
        className="rounded-pill p-1 text-ink-secondary hover:bg-hover"
      >
        <ChevronLeft className="size-4" aria-hidden />
      </button>

      {/* The label is the thing being read, so it gets the weight. */}
      <span className="min-w-[112px] text-center text-sm font-medium tabular">
        {range.label}
      </span>

      <button
        type="button"
        onClick={() => step(1)}
        aria-label="Next period"
        className="rounded-pill p-1 text-ink-secondary hover:bg-hover"
      >
        <ChevronRight className="size-4" aria-hidden />
      </button>

      {/* Jumping straight to a period, for when stepping would take a while. */}
      {count > 1 && (
        <select
          value={index}
          onChange={(e) => setIndex(Number(e.target.value))}
          aria-label="Period within the year"
          className="cursor-pointer border-0 bg-transparent py-0.5 text-sm text-ink-secondary focus:outline-none"
        >
          {Array.from({ length: count }, (_, i) => i + 1).map((i) => (
            <option key={i} value={i}>{shortLabel(basis, i)}</option>
          ))}
        </select>
      )}

      <select
        value={year}
        onChange={(e) => setYear(Number(e.target.value))}
        aria-label="Fiscal year"
        className="cursor-pointer rounded-pill border-0 bg-transparent py-0.5 pr-1 text-sm text-ink-secondary focus:outline-none"
      >
        {yearsAround(year).map((y) => (
          <option key={y} value={y}>{y}</option>
        ))}
      </select>
    </div>
  )
}

function shortLabel(basis: PeriodBasis, index: number): string {
  if (basis === 'month') {
    return ['Jan','Feb','Mar','Apr','May','Jun','Jul','Aug','Sep','Oct','Nov','Dec'][index - 1] ?? ''
  }
  if (basis === 'quarter') return `Q${index}`
  return `S${index}`
}

/**
 * A window of years around the one selected.
 *
 * Wide enough to reach a prior year for comparison, and to look ahead when a
 * budget for next year is being prepared, without becoming a long list.
 */
function yearsAround(year: number): number[] {
  const current = new Date().getFullYear()
  const newest = Math.max(current + 1, year)
  return Array.from({ length: 7 }, (_, i) => newest - i)
}
