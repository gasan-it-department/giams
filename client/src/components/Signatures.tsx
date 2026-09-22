/**
 * The certification blocks a form carries when it leaves this system.
 *
 * Print-only. On screen they are clutter — nobody signs a monitor — but a
 * form filed without them is not a form, and an auditor who receives one
 * cannot tell who stands behind the figures.
 *
 * Every act and officer below is taken from the office's own workbook
 * (`Accounting forms.xlsx`), not from practice assumed on its behalf:
 *
 *   * JEV, Annex 23 — "Prepared by: / Accounting Staff" and
 *     "Approved by: / Municipal Accountant". Two signatures, not three.
 *   * General Journal, Annex 1 — "Certified Correct:", the Municipal
 *     Accountant, and a date line.
 *   * Cash Receipts and Check Disbursements journals, Annexes 2 and 3 —
 *     "Certified correct:", the rule labelled "Signature", the Municipal
 *     Accountant.
 *
 * The small differences between those are the office's own, so they are
 * reproduced rather than tidied into one house style.
 */

interface Signatory {
  /** What this officer is attesting — "Certified correct:", "Approved by:". */
  act: string
  /** Printed under the rule, where the name is written. */
  role: string
  /** Filled in where the system knows who did it. */
  name?: string
  /** Annexes 2 and 3 label the rule itself. */
  signatureLabel?: boolean
  /** Annex 1 carries a date line under the name. */
  dated?: boolean
}

export function Signatures({
  signatories,
  note,
}: {
  signatories: Signatory[]
  note?: string
}) {
  // A lone certification sits on the right of the form and is narrow — in the
  // office's workbook it is at column F of the General Journal and column R of
  // the Cash Receipts Journal, never a rule ruled across the whole page.
  // Several signatories spread across instead, as on the JEV.
  const alone = signatories.length === 1

  return (
    <div className="print-only print-signatures mt-10">
      {/* Pushed right with an auto margin rather than flex: the print rules
          force `.print-only` to `display: block`, which would discard a flex
          container and leave the rule ruled across the whole page. */}
      <div
        className="grid gap-x-10 gap-y-8"
        style={
          alone
            ? { gridTemplateColumns: 'minmax(0, 1fr)', width: '42%', marginLeft: 'auto' }
            : { gridTemplateColumns: `repeat(${Math.min(signatories.length, 3)}, minmax(0, 1fr))` }
        }
      >
        {signatories.map((s) => (
          <div key={`${s.act}-${s.role}`}>
            <p className="text-sm">{s.act}</p>

            {/* The rule is what is signed over. It sits above the name, as on
                the paper form, so a signature does not collide with the
                printed title beneath it. */}
            <p className="mt-8 border-t border-black pt-1 text-center text-sm font-medium uppercase">
              {s.name ?? ' '}
            </p>

            {s.signatureLabel && <p className="text-center text-xs">Signature</p>}
            <p className="text-center text-xs">{s.role}</p>

            {s.dated && (
              <p className="mx-auto mt-7 w-4/5 border-t border-black pt-1 text-center text-xs">
                Date
              </p>
            )}
          </div>
        ))}
      </div>

      {note && <p className="mt-6 text-xs">{note}</p>}
    </div>
  )
}

/** Certified correct by the Municipal Accountant — the books and registries. */
export function CertifiedCorrect({
  by = 'Municipal Accountant',
  name,
  signatureLabel = false,
  dated = false,
}: {
  by?: string
  name?: string
  signatureLabel?: boolean
  dated?: boolean
}) {
  return (
    <Signatures
      signatories={[{ act: 'Certified correct:', role: by, name, signatureLabel, dated }]}
    />
  )
}
