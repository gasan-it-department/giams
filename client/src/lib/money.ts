/**
 * Displaying peso amounts.
 *
 * Amounts arrive from the server as **strings** and stay strings. There is no
 * `parseFloat` in this file and there must not be one anywhere in the client:
 * the server computes every figure with exact decimal arithmetic, and running
 * it through a JavaScript double would quietly undo that.
 *
 * If you find yourself wanting to add two amounts here, the calculation
 * belongs on the server.
 */

/**
 * An amount as it travels: a decimal string such as `"184320.00"`.
 *
 * Branded rather than a bare `string` alias, which buys two things:
 *
 *   * TypeScript already refuses arithmetic on a `string`, so `amount * 2` and
 *     `a - b` are compile errors. The brand keeps that protection even where a
 *     value is widened.
 *   * An arbitrary string can no longer be passed where an amount is expected,
 *     so a field that was never money cannot drift into a money position.
 *
 * The escape hatches TypeScript cannot see — `Number(amount)`, `parseFloat` —
 * are caught by `scripts/check-money.mjs`, which runs as part of `npm run lint`.
 */
export type Amount = string & { readonly __amount: unique symbol }

/**
 * Marks a string as an amount.
 *
 * Only for values that genuinely came from the server as money. This is an
 * assertion, not a conversion: it does no parsing and no validation, because
 * the server is the only thing that produces amounts.
 */
export function asAmount(value: string): Amount {
  return value as Amount
}

const GROUPER = new Intl.NumberFormat('en-PH', {
  minimumFractionDigits: 2,
  maximumFractionDigits: 2,
})

/**
 * Formats for display: thousands separated, always two decimals, negatives in
 * parentheses per accounting convention rather than with a minus sign.
 *
 * Grouping is done on the integer part only, and the fractional digits are
 * carried across untouched, so no rounding decision is ever made here.
 */
export function formatAmount(value: Amount | null | undefined): string {
  if (value === null || value === undefined || value === '') return '—'

  const trimmed = value.trim()
  const negative = trimmed.startsWith('-')
  const unsigned = negative ? trimmed.slice(1) : trimmed

  const [whole = '0', fraction = ''] = unsigned.split('.')
  const centavos = (fraction + '00').slice(0, 2)

  const grouped = GROUPER.format(BigInt(whole || '0')).split('.')[0] ?? '0'
  const rendered = `${grouped}.${centavos}`

  return negative ? `(${rendered})` : rendered
}

/** Formats with the peso sign, for totals and headline figures. */
export function formatPeso(value: Amount | null | undefined): string {
  const rendered = formatAmount(value)
  return rendered === '—' ? rendered : `₱${rendered}`
}

/** True when the amount is negative, for styling. Does no arithmetic. */
export function isNegative(value: Amount | null | undefined): boolean {
  return typeof value === 'string' && value.trim().startsWith('-')
}

// ── Exact addition, for entry forms only ─────────────────────────────────────
//
// A journal entry form has to show the clerk a running debit and credit total
// while they type, otherwise they cannot tell whether the entry balances until
// the server rejects it. That means adding money on the client, which
// everything else in this file exists to prevent.
//
// The way out is not to relax the rule but to add exactly: amounts are parsed
// into **integer centavos as BigInt** and summed there. No `Number`, no
// `parseFloat`, no floating point at any step — 0.1 + 0.2 cannot arise because
// there are no fractions, only whole centavos.
//
// This is a *guide* regardless. The authority on whether an entry balances is
// the deferred constraint in the database, which checks it at COMMIT and
// rejects the transaction if it does not. The form total exists so a clerk
// finds out while typing rather than on submit.

/** Parses an amount into whole centavos. Exact; throws on malformed input. */
function toCentavos(value: Amount | string): bigint {
  const trimmed = String(value).trim()
  if (trimmed === '') return 0n

  const negative = trimmed.startsWith('-')
  const unsigned = negative ? trimmed.slice(1) : trimmed

  const [whole = '0', fraction = ''] = unsigned.split('.')
  if (!/^\d*$/.test(whole) || !/^\d*$/.test(fraction)) {
    throw new Error(`"${value}" is not an amount`)
  }

  // Pad or truncate to exactly two decimal places.
  const centavos = (fraction + '00').slice(0, 2)
  const magnitude = BigInt(whole || '0') * 100n + BigInt(centavos)

  return negative ? -magnitude : magnitude
}

/** Renders whole centavos back as an amount string. */
function fromCentavos(centavos: bigint): Amount {
  const negative = centavos < 0n
  const magnitude = negative ? -centavos : centavos
  const whole = magnitude / 100n
  const fraction = (magnitude % 100n).toString().padStart(2, '0')

  return `${negative ? '-' : ''}${whole}.${fraction}` as Amount
}

/**
 * Adds amounts exactly, for a form's running total.
 *
 * Malformed entries are skipped rather than throwing: a clerk mid-keystroke
 * has "12." in the field, and a total that vanishes while typing is worse than
 * one that ignores an incomplete line.
 */
export function sumAmounts(amounts: Array<Amount | string | null | undefined>): Amount {
  let running = 0n

  for (const amount of amounts) {
    if (amount === null || amount === undefined) continue
    try {
      running += toCentavos(amount)
    } catch {
      continue
    }
  }

  return fromCentavos(running)
}

/** Compares two amounts exactly. Returns -1, 0, or 1. */
export function compareAmounts(a: Amount | string, b: Amount | string): -1 | 0 | 1 {
  const left = toCentavos(a)
  const right = toCentavos(b)
  return left < right ? -1 : left > right ? 1 : 0
}

/** True when the amount is exactly zero, however it is written. */
export function isZero(value: Amount | string | null | undefined): boolean {
  if (value === null || value === undefined) return true
  try {
    return toCentavos(value) === 0n
  } catch {
    return false
  }
}
