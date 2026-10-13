import { z } from 'zod'
import { EXPENSE_CLASSES } from '@/schemas/accounts'

/** The kinds of appropriating instrument the database accepts. */
export const APPROPRIATION_TYPES = ['annual', 'supplemental', 'continuing'] as const
export type AppropriationType = (typeof APPROPRIATION_TYPES)[number]

export const APPROPRIATION_TYPE_LABELS: Record<AppropriationType, string> = {
  annual:      'Annual',
  supplemental: 'Supplemental',
  continuing:  'Continuing',
}

/**
 * A money field: digits with up to two decimal places, greater than zero.
 *
 * Kept as a string end to end — never parsed to a number — so a peso figure
 * stays exact. The server validates independently and remains the authority.
 */
const money = z
  .string()
  .trim()
  .regex(/^\d+(\.\d{1,2})?$/, 'Enter an amount like 1500.00.')
  .refine((v) => Number(v) > 0, 'Enter an amount greater than zero.')

/**
 * A new appropriation.
 *
 * Mirrors the `appropriation` table and the server's checks, so a mistake is
 * caught before a round trip and the offending field is named.
 */
export const newAppropriationSchema = z.object({
  fund_id:            z.string().min(1, 'Choose the fund.'),
  fiscal_year:        z.number().int(),
  ordinance_no:       z.string().trim().min(1, 'Enter the ordinance number that authorises this.'),
  ordinance_date:     z.string().min(1, 'Enter the date of the ordinance.'),
  appropriation_type: z.enum(APPROPRIATION_TYPES),
  office_id:          z.string().min(1, 'Choose the office this is appropriated to.'),
  account_id:         z.string().min(1, 'Choose the account.'),
  expense_class:      z.enum(EXPENSE_CLASSES),
  amount:             money,
  particulars:        z.string().trim().optional(),
})

export type NewAppropriationForm = z.input<typeof newAppropriationSchema>

/** A release of part of an appropriation into a spendable allotment. */
export const newAllotmentSchema = z.object({
  appropriation_id: z.string().min(1, 'Choose the appropriation to release from.'),
  release_date:     z.string().min(1, 'Enter the release date.'),
  amount:           money,
})

export type NewAllotmentForm = z.input<typeof newAllotmentSchema>
