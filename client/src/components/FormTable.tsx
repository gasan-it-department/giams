import type { ReactNode } from 'react'

/**
 * The cells the COA forms are ruled with.
 *
 * Column headings are normal weight because on these forms the figures are
 * what the eye should land on, not the labels above them. Rows are tight —
 * 1.5 units of padding — so a full page of a journal fits on one screen the
 * way it fits on one sheet.
 */

export function Th({ children, className = '' }: { children?: ReactNode; className?: string }) {
  return <th className={`px-3 py-2 font-normal ${className}`}>{children}</th>
}

export function Td({
  children, className = '', colSpan,
}: {
  children?: ReactNode
  className?: string
  colSpan?: number
}) {
  return <td colSpan={colSpan} className={`px-3 py-1.5 ${className}`}>{children}</td>
}
