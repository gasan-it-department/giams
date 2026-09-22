import { useEffect } from 'react'

/**
 * Page setup for printing a form.
 *
 * `@page` cannot be scoped to a class — it applies to the document — so the
 * rule is written into a style element that the open screen owns and removes
 * when it closes. Without this every form prints portrait, and the journals
 * and the budget statement lose their right-hand columns off the edge of the
 * paper.
 *
 * Legal is not offered. The office prints on A4 and Letter, and a size nobody
 * has in the tray is a size that produces a scaled, unreadable form.
 */

export type PageOrientation = 'portrait' | 'landscape'

const STYLE_ID = 'giams-page-setup'

/**
 * Sets the paper orientation while the calling screen is mounted.
 *
 * Margins are 12mm all round: enough for the office's binding punch on the
 * left without pushing the rightmost figure column off a landscape page.
 */
export function usePageSetup(orientation: PageOrientation): void {
  useEffect(() => {
    const style = document.createElement('style')
    style.id = STYLE_ID
    style.textContent = `@page { size: ${orientation}; margin: 12mm; }`
    document.head.append(style)

    return () => style.remove()
  }, [orientation])
}

/**
 * Opens the print dialog.
 *
 * Deliberately a function rather than `window.print` passed directly: the
 * browser paints the print view from the live DOM, and a click handler that
 * fires mid-render prints a half-drawn table. A frame's delay costs nothing
 * and removes the whole class of complaint.
 */
export function printForm(): void {
  requestAnimationFrame(() => window.print())
}
