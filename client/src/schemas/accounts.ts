import { z } from 'zod'

/** The five account groups of the Revised Chart of Accounts. */
export const ACCOUNT_TYPES = ['asset', 'liability', 'equity', 'income', 'expense'] as const
export type AccountType = (typeof ACCOUNT_TYPES)[number]

export const ACCOUNT_TYPE_LABELS: Record<AccountType, string> = {
  asset:     'Asset',
  liability: 'Liability',
  equity:    'Equity',
  income:    'Income',
  expense:   'Expense',
}

/** Allotments and obligations are tracked per expense class. */
export const EXPENSE_CLASSES = ['PS', 'MOOE', 'FE', 'CO'] as const
export type ExpenseClass = (typeof EXPENSE_CLASSES)[number]

export const EXPENSE_CLASS_LABELS: Record<ExpenseClass, string> = {
  PS:   'Personal services',
  MOOE: 'Maintenance and other operating expenses',
  FE:   'Financial expenses',
  CO:   'Capital outlay',
}

export const accountSchema = z.object({
  id:                  z.string(),
  code:                z.string(),
  name:                z.string(),
  account_type:        z.enum(ACCOUNT_TYPES),
  normal_balance:      z.enum(['debit', 'credit']),
  parent_id:           z.string().nullable(),
  is_postable:         z.boolean(),
  is_contra:           z.boolean(),
  requires_subsidiary: z.boolean(),
  expense_class:       z.enum(EXPENSE_CLASSES).nullable(),
  is_cash:             z.boolean(),
  is_active:           z.boolean(),
  description:         z.string().nullable(),
})

export type Account = z.infer<typeof accountSchema>

/**
 * A new account.
 *
 * The code rules mirror the server's, so a typing mistake is caught before a
 * round trip. The server validates independently and is the authority — this
 * is a convenience, not a control.
 */
export const newAccountSchema = z.object({
  code: z
    .string()
    .trim()
    .min(1, 'Enter the account code.')
    .regex(/^[0-9-]+$/, 'An account code uses only numbers and dashes, like 1-07-03-010.')
    .refine(
      (v) => (v.match(/\d/g)?.length ?? 0) === 8,
      'An account code has eight digits, like 1-07-03-010.',
    ),
  name:                z.string().trim().min(1, 'Enter the account name.'),
  account_type:        z.enum(ACCOUNT_TYPES),
  parent_id:           z.string().nullable().default(null),
  is_postable:         z.boolean().default(true),
  is_contra:           z.boolean().default(false),
  requires_subsidiary: z.boolean().default(false),
  expense_class:       z.enum(EXPENSE_CLASSES).nullable().default(null),
  is_cash:             z.boolean().default(false),
  description:         z.string().trim().nullable().default(null),
})
.refine((v) => v.expense_class === null || v.account_type === 'expense', {
  path: ['expense_class'],
  message: 'Only expense accounts have an expense class.',
})

export type NewAccount = z.input<typeof newAccountSchema>

export interface AccountFilter {
  search?:           string
  account_type?:     AccountType
  postable_only?:    boolean
  include_inactive?: boolean
  /** Zero-based. */
  page?:             number
}
