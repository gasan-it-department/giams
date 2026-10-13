import { z } from 'zod'

/**
 * The admin console's forms.
 *
 * The rules mirror the server's, and the field paths match the ones it
 * reports errors on (`staff.0.username`, `terms.seats`), so a refusal lands on
 * the exact field. The server remains the authority.
 */

const username = z
  .string()
  .trim()
  .toLowerCase()
  .regex(/^[a-z0-9._-]{3,40}$/, 'A username is 3 to 40 letters, digits, dots, dashes, or underscores — like m.santos.')

const fullName = z.string().trim().min(1, "Enter the person's full name.")

export const newAccountSchema = z.object({
  full_name:      fullName,
  username,
  position_title: z.string().trim().optional(),
  role:           z.string().min(1, 'Choose a role.'),
})

export type NewAccountForm = z.input<typeof newAccountSchema>

/** How long a key gives access for, in the order the form offers them. */
export const ACCESS_OPTIONS = [
  { value: 'monthly',  label: 'Monthly renewal', detail: 'Renewed each month from this console. The office keeps the same key.' },
  { value: 'annual',   label: 'Annual',          detail: 'Renewed each year from this console. The office keeps the same key.' },
  { value: 'lifetime', label: 'Lifetime',        detail: 'Never expires, so it is never renewed.' },
] as const

export const termsSchema = z.object({
  access:    z.enum(['monthly', 'annual', 'lifetime']),
  seats:     z.coerce.number().int().min(1, 'At least one workstation.').max(500, 'Up to 500 workstations.'),
  starts_on: z.string().min(1, 'Choose the day the key starts.'),
})

export type TermsForm = z.input<typeof termsSchema>

const person = z.object({
  full_name: fullName,
  username,
})

export const newLicenseeSchema = z
  .object({
    municipality_name: z.string().trim().min(1, 'Choose the municipality.').max(120, 'That name is too long.'),
    for_this_server:   z.boolean(),
    accountant:        person,
    staff:             z.array(person).min(1, 'Add at least one member of the Accounting Staff.'),
    terms:             termsSchema,
  })
  .superRefine((v, ctx) => {
    // One username per person on a profile — the same check the server makes.
    const seen = new Set([v.accountant.username.trim().toLowerCase()])
    v.staff.forEach((s, i) => {
      const u = s.username.trim().toLowerCase()
      if (u && seen.has(u)) {
        ctx.addIssue({
          code:    'custom',
          path:    ['staff', i, 'username'],
          message: 'This username is already used by someone else on this profile.',
        })
      }
      seen.add(u)
    })
  })

export type NewLicenseeForm = z.input<typeof newLicenseeSchema>

/**
 * The municipalities of Marinduque, offered first when choosing who a licence
 * is for. Any other municipality can still be typed in.
 */
export const MARINDUQUE = [
  'Municipality of Boac',
  'Municipality of Buenavista',
  'Municipality of Gasan',
  'Municipality of Mogpog',
  'Municipality of Santa Cruz',
  'Municipality of Torrijos',
] as const

/** A lifetime key's end date is a sentinel, never shown as a date. */
export function isLifetime(expiresAt: string | null | undefined): boolean {
  return !!expiresAt && new Date(expiresAt).getUTCFullYear() >= 9999
}

export function accessLabel(plan: string | null | undefined): string {
  return ACCESS_OPTIONS.find((o) => o.value === plan)?.label ?? (plan ? plan.charAt(0).toUpperCase() + plan.slice(1) : '—')
}

/** Words that belong to the surname after them: "dela Cruz", "de los Santos". */
const PARTICLES = new Set(['de', 'del', 'dela', 'delos', 'la', 'las', 'los', 'san', 'santa', 'sta', 'van', 'von'])

/** Generational suffixes, which are not part of anyone's username. */
const SUFFIXES = new Set(['jr', 'sr', 'ii', 'iii', 'iv', 'v'])

/**
 * A username from a full name: first initial, a dot, the surname.
 *
 *   Maria Clara Santos  → m.santos      (the middle name is dropped)
 *   Maria Dela Cruz     → m.delacruz    (particles stay with the surname)
 *   Juan Reyes Jr.      → j.reyes
 *
 * A suggestion only, offered while the field is empty; an office with its own
 * convention just types over it.
 */
export function suggestUsername(name: string): string {
  const words = name
    .normalize('NFD')
    .replace(/[̀-ͯ]/g, '')
    .toLowerCase()
    .replace(/[^a-z\s]/g, '')
    .split(/\s+/)
    .filter(Boolean)

  while (words.length > 1 && SUFFIXES.has(words.at(-1) ?? '')) words.pop()

  const first = words[0]
  if (!first) return ''
  if (words.length === 1) return first.slice(0, 40)

  // Walk back from the last word, taking any particles in front of it, but
  // never the first name.
  let start = words.length - 1
  while (start > 1 && PARTICLES.has(words[start - 1] ?? '')) start--

  return `${first.charAt(0)}.${words.slice(start).join('')}`.slice(0, 40)
}
