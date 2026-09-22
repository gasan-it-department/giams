#!/usr/bin/env node
/**
 * Fails the build if anything in the client turns a peso amount into a number.
 *
 * Amounts arrive from the server as strings and are displayed as strings. The
 * server computes every figure with exact decimal arithmetic; passing one
 * through a JavaScript `number` silently undoes that, and the damage shows up
 * much later as a reconciliation that is off by a centavo for no visible
 * reason.
 *
 * TypeScript already blocks the obvious form — arithmetic on a `string` is a
 * type error, and `Amount` is branded so it stays one. What TypeScript cannot
 * see is an explicit conversion, so those are caught here.
 *
 * This is the same reasoning as removing the shadow utilities from the Tailwind
 * theme: a rule the tooling enforces beats a rule a reviewer has to remember.
 */

import { readdir, readFile } from 'node:fs/promises'
import { join, relative } from 'node:path'
import { fileURLToPath } from 'node:url'

const SRC = join(fileURLToPath(new URL('../src', import.meta.url)))

/** Words that mark an identifier as holding money. */
const MONEY = String.raw`(?:amount|balance|total|gross|net|debit|credit|peso|deduction|payable|paid|allot)`

const RULES = [
  {
    // Number(amount), Number(row.total), Number(x.net_amount)
    pattern: new RegExp(String.raw`\bNumber\s*\(\s*[\w.\[\]'"]*${MONEY}`, 'i'),
    message: 'Number() applied to an amount',
  },
  {
    pattern: /\bparseFloat\s*\(/,
    message: 'parseFloat() — amounts must stay strings',
  },
  {
    // parseInt is fine on a page number, never on money.
    pattern: new RegExp(String.raw`\bparseInt\s*\(\s*[\w.\[\]'"]*${MONEY}`, 'i'),
    message: 'parseInt() applied to an amount',
  },
  {
    // Unary plus: +amount
    pattern: new RegExp(String.raw`[^\w)]\+\s*[\w.]*${MONEY}\b`, 'i'),
    message: 'unary + coerces an amount to a number',
  },
  {
    // Arithmetic between two money-ish identifiers.
    pattern: new RegExp(String.raw`\b[\w.]*${MONEY}[\w.]*\s*[-*/]\s*[\w.]*${MONEY}`, 'i'),
    message: 'arithmetic on amounts — this belongs on the server',
  },
]

/** Lines allowed to mention the patterns: the rules themselves, and prose. */
function isExempt(line, file) {
  return (
    file.endsWith('check-money.mjs') ||
    line.trimStart().startsWith('*') ||
    line.trimStart().startsWith('//') ||
    line.includes('eslint-disable') ||
    line.includes('check-money: allow')
  )
}

async function* walk(dir) {
  for (const entry of await readdir(dir, { withFileTypes: true })) {
    const path = join(dir, entry.name)
    if (entry.isDirectory()) yield* walk(path)
    else if (/\.(ts|tsx)$/.test(entry.name)) yield path
  }
}

const findings = []

for await (const file of walk(SRC)) {
  const text = await readFile(file, 'utf8')
  text.split('\n').forEach((line, i) => {
    if (isExempt(line, file)) return
    for (const rule of RULES) {
      if (rule.pattern.test(line)) {
        findings.push({
          file: relative(process.cwd(), file),
          line: i + 1,
          message: rule.message,
          source: line.trim(),
        })
      }
    }
  })
}

if (findings.length === 0) {
  console.log('money check: no amounts are converted to numbers')
  process.exit(0)
}

console.error('\nAmounts must never become JavaScript numbers.\n')
for (const f of findings) {
  console.error(`  ${f.file}:${f.line}  ${f.message}`)
  console.error(`      ${f.source}\n`)
}
console.error(
  'The server computes every figure with exact decimal arithmetic. Format the\n' +
  'string for display, or move the calculation to the server.\n' +
  'If a line is genuinely not about money, append:  // check-money: allow\n'
)
process.exit(1)
