/**
 * Hands a file the app made to the person, as a download.
 *
 * One place for it, so the JEV form, exports and the spreadsheet editor all
 * save the same way.
 */
export function saveFile(blob: Blob, filename: string): void {
  const url = URL.createObjectURL(blob)
  const anchor = document.createElement('a')
  anchor.href = url
  anchor.download = filename
  document.body.append(anchor)
  anchor.click()
  anchor.remove()
  // revoked on the next tick so the click has taken the download
  setTimeout(() => URL.revokeObjectURL(url), 1000)
}

export const XLSX_TYPE = 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet'
