import { create } from 'zustand'
import { persist } from 'zustand/middleware'

/**
 * The accounting period every screen reports on.
 *
 * One selection, used system-wide: a clerk looking at March should see March
 * in the voucher register, the obligations, the reconciliation, and the trial
 * balance without setting it four times. Screens that genuinely span the whole
 * year — the chart of accounts, user administration — simply ignore it.
 *
 * Persisted, because a period is a working context rather than a preference:
 * closing the application at the end of a Tuesday and reopening on Wednesday
 * should not silently move you to a different month.
 */

export type PeriodBasis = 'annual' | 'semester' | 'quarter' | 'month'

export interface PeriodRange {
  from: string
  to: string
  /** How a person would say it: "March 2026", "2nd quarter 2026". */
  label: string
  /** The fiscal year, which the budget registries are keyed by. */
  fiscalYear: number
}

interface PeriodState {
  basis: PeriodBasis
  year: number
  /** Which one within the year: 1-12 for months, 1-4 quarters, 1-2 semesters. */
  index: number

  setBasis: (basis: PeriodBasis) => void
  setYear: (year: number) => void
  setIndex: (index: number) => void
  /** Moves one period back or forward, rolling the year over. */
  step: (direction: -1 | 1) => void
  range: () => PeriodRange
}

const MONTHS = [
  'January', 'February', 'March', 'April', 'May', 'June',
  'July', 'August', 'September', 'October', 'November', 'December',
]

/** How many divisions a basis has in a year. */
export function divisions(basis: PeriodBasis): number {
  return basis === 'month' ? 12 : basis === 'quarter' ? 4 : basis === 'semester' ? 2 : 1
}

function iso(year: number, month: number, day: number): string {
  return `${year}-${String(month).padStart(2, '0')}-${String(day).padStart(2, '0')}`
}

/** The last day of a month, accounting for leap years. */
function lastDay(year: number, month: number): number {
  return new Date(year, month, 0).getDate()
}

export function rangeFor(basis: PeriodBasis, year: number, index: number): PeriodRange {
  if (basis === 'annual') {
    return {
      from: iso(year, 1, 1),
      to: iso(year, 12, 31),
      label: `${year}`,
      fiscalYear: year,
    }
  }

  const monthsEach = 12 / divisions(basis)
  const firstMonth = (index - 1) * monthsEach + 1
  const lastMonth = firstMonth + monthsEach - 1

  const label =
    basis === 'month'    ? `${MONTHS[firstMonth - 1]} ${year}`
    : basis === 'quarter' ? `${ordinal(index)} quarter ${year}`
    :                       `${ordinal(index)} semester ${year}`

  return {
    from: iso(year, firstMonth, 1),
    to: iso(year, lastMonth, lastDay(year, lastMonth)),
    label,
    fiscalYear: year,
  }
}

function ordinal(n: number): string {
  return n === 1 ? '1st' : n === 2 ? '2nd' : n === 3 ? '3rd' : `${n}th`
}

/** Defaults to the month we are actually in. */
function currentIndex(basis: PeriodBasis, month: number): number {
  if (basis === 'annual') return 1
  const monthsEach = 12 / divisions(basis)
  return Math.floor((month - 1) / monthsEach) + 1
}

const now = new Date()

export const usePeriodStore = create<PeriodState>()(
  persist(
    (set, get) => ({
      basis: 'month',
      year: now.getFullYear(),
      index: currentIndex('month', now.getMonth() + 1),

      setBasis(basis) {
        // Keep the same point in time when the basis changes: someone looking
        // at March who switches to quarters wants the quarter containing
        // March, not the first quarter of the year.
        const { basis: old, index } = get()
        const monthNow =
          old === 'annual' ? 1 : Math.round((index - 1) * (12 / divisions(old))) + 1

        set({ basis, index: currentIndex(basis, monthNow) })
      },

      setYear: (year) => set({ year }),
      setIndex: (index) => set({ index }),

      step(direction) {
        const { basis, year, index } = get()
        const count = divisions(basis)
        const next = index + direction

        if (next < 1) set({ year: year - 1, index: count })
        else if (next > count) set({ year: year + 1, index: 1 })
        else set({ index: next })
      },

      range() {
        const { basis, year, index } = get()
        return rangeFor(basis, year, index)
      },
    }),
    {
      name: 'giams.period',
      // Only the selection. The derived range is recomputed, so a stored one
      // could go stale against a fixed bug in the arithmetic.
      partialize: (s) => ({ basis: s.basis, year: s.year, index: s.index }),
    },
  ),
)

/**
 * The current range, as a hook.
 *
 * Subscribes to the three fields rather than calling `range()` inside a
 * component, so a change actually re-renders.
 */
export function usePeriodRange(): PeriodRange {
  const basis = usePeriodStore((s) => s.basis)
  const year = usePeriodStore((s) => s.year)
  const index = usePeriodStore((s) => s.index)
  return rangeFor(basis, year, index)
}
