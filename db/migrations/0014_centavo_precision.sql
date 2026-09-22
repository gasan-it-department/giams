-- ═══════════════════════════════════════════════════════════════════════════
--  0014 — Money is stored to the centavo, and nothing finer
-- ═══════════════════════════════════════════════════════════════════════════
--  The Accounting and Treasury offices work to two decimal places. Every
--  voucher, register, and financial statement they produce is in pesos and
--  centavos, and any figure they check by hand is checked at that precision.
--
--  Migrations 0005 to 0013 stored money as NUMERIC(18,4), on the reasoning
--  that percentage withholding produces sub-centavo intermediates. That was
--  the wrong place to keep the extra precision. A stored 3686.4050 displays as
--  3686.41 but SUMS as 3686.4050, so a column of figures adds up to something
--  that differs from the total the same screen shows — by a centavo, for no
--  visible reason, and only sometimes. That is among the hardest classes of
--  accounting bug to trace, and the office would be right to distrust the
--  system over it.
--
--  So: **intermediates may carry more precision inside a single computation,
--  but anything persisted is exactly two decimal places.** Rounding happens
--  once, at the point the figure is written, half away from zero — .005 and
--  above goes up, which is what a clerk with a calculator does.
--
--  Rates are NOT money and keep their six decimal places: 0.015000 is a
--  genuine 1.5%, not an amount.
-- ═══════════════════════════════════════════════════════════════════════════

-- ── Views first ──────────────────────────────────────────────────────────────
-- PostgreSQL refuses to alter the type of a column a view selects from, so the
-- registries and the trial balance come down and go back up around the change.
-- They are pure projections over the tables, so nothing is lost.
DROP VIEW IF EXISTS trial_balance;
DROP VIEW IF EXISTS registry_raod;
DROP VIEW IF EXISTS registry_rapal;

-- Same restriction applies to a trigger that names a column in `UPDATE OF`:
-- the column list is a dependency. These come down and go back up unchanged;
-- their functions are untouched, so no behaviour moves.
DROP TRIGGER IF EXISTS allotment_within_appropriation   ON allotment;
DROP TRIGGER IF EXISTS obligation_within_allotment      ON obligation;
DROP TRIGGER IF EXISTS dv_refreshes_obligation          ON disbursement_voucher;
DROP TRIGGER IF EXISTS disbursement_refreshes_obligation ON disbursement;
DROP TRIGGER IF EXISTS disbursement_within_voucher      ON disbursement;


-- ── Ledger ───────────────────────────────────────────────────────────────────
ALTER TABLE journal_entry_line
    ALTER COLUMN debit  TYPE numeric(18,2) USING round(debit, 2),
    ALTER COLUMN credit TYPE numeric(18,2) USING round(credit, 2);

-- ── Budget execution ─────────────────────────────────────────────────────────
ALTER TABLE appropriation
    ALTER COLUMN amount TYPE numeric(18,2) USING round(amount, 2);

ALTER TABLE allotment
    ALTER COLUMN amount TYPE numeric(18,2) USING round(amount, 2);

ALTER TABLE obligation
    ALTER COLUMN amount         TYPE numeric(18,2) USING round(amount, 2),
    ALTER COLUMN payable_amount TYPE numeric(18,2) USING round(payable_amount, 2),
    ALTER COLUMN paid_amount    TYPE numeric(18,2) USING round(paid_amount, 2);

-- ── Disbursement ─────────────────────────────────────────────────────────────
-- net_amount is generated from the other two, so it has to be dropped and
-- rebuilt around the type change rather than altered in place.
ALTER TABLE disbursement_voucher DROP COLUMN net_amount;

ALTER TABLE disbursement_voucher
    ALTER COLUMN gross_amount     TYPE numeric(18,2) USING round(gross_amount, 2),
    ALTER COLUMN total_deductions TYPE numeric(18,2) USING round(total_deductions, 2);

ALTER TABLE disbursement_voucher
    ADD COLUMN net_amount numeric(18,2)
        GENERATED ALWAYS AS (gross_amount - total_deductions) STORED;

ALTER TABLE dv_deduction
    ALTER COLUMN base_amount   TYPE numeric(18,2) USING round(base_amount, 2),
    ALTER COLUMN fixed_applied TYPE numeric(18,2) USING round(fixed_applied, 2),
    ALTER COLUMN amount        TYPE numeric(18,2) USING round(amount, 2);

ALTER TABLE disbursement
    ALTER COLUMN amount TYPE numeric(18,2) USING round(amount, 2);

-- A fixed deduction is an amount, so it follows the same rule. `rate` does
-- not: it stays numeric(9,6).
ALTER TABLE deduction_rate
    ALTER COLUMN fixed_amount TYPE numeric(18,2) USING round(fixed_amount, 2),
    ALTER COLUMN bracket_min  TYPE numeric(18,2) USING round(bracket_min, 2),
    ALTER COLUMN bracket_max  TYPE numeric(18,2) USING round(bracket_max, 2);

-- ── Period balances ──────────────────────────────────────────────────────────
ALTER TABLE account_period_balance
    ALTER COLUMN debit_total    TYPE numeric(18,2) USING round(debit_total, 2),
    ALTER COLUMN credit_total   TYPE numeric(18,2) USING round(credit_total, 2),
    ALTER COLUMN ending_balance TYPE numeric(18,2) USING round(ending_balance, 2);


-- ── The rounding rule, in one place ──────────────────────────────────────────
/*
 * Rounds to the centavo, half away from zero.
 *
 * PostgreSQL's `round(numeric, int)` already rounds half away from zero, so
 * this is a thin wrapper — but a named function means every computation in the
 * schema states which rule it is using, and a future change happens once
 * instead of being hunted through every trigger.
 *
 * Half away from zero, not banker's rounding: .005 goes up. That is what a
 * clerk with a calculator does, and a system that disagrees with the person
 * checking it loses that argument every time.
 */
CREATE OR REPLACE FUNCTION to_centavo(p_amount numeric)
RETURNS numeric(18,2)
LANGUAGE sql
IMMUTABLE
AS $$
    SELECT round(p_amount, 2)::numeric(18,2)
$$;

COMMENT ON FUNCTION to_centavo(numeric) IS
    'The single rounding rule for money: two decimal places, half away from '
    'zero. Use wherever a computed amount is about to be stored.';


-- ── Triggers, rebuilt ────────────────────────────────────────────────────────
-- Identical to their definitions in 0008 and 0011. The ceilings and running
-- totals they enforce are unchanged; only the column type underneath moved.

CREATE TRIGGER allotment_within_appropriation
    BEFORE INSERT OR UPDATE OF amount, appropriation_id ON allotment
    FOR EACH ROW EXECUTE FUNCTION assert_allotment_within_appropriation();

CREATE TRIGGER obligation_within_allotment
    BEFORE INSERT OR UPDATE OF amount, allotment_id, status ON obligation
    FOR EACH ROW EXECUTE FUNCTION assert_obligation_within_allotment();

CREATE TRIGGER dv_refreshes_obligation
    AFTER INSERT OR UPDATE OF status, gross_amount, obligation_id OR DELETE
    ON disbursement_voucher
    FOR EACH ROW EXECUTE FUNCTION refresh_obligation_status();

CREATE TRIGGER disbursement_refreshes_obligation
    AFTER INSERT OR UPDATE OF status, amount, dv_id OR DELETE
    ON disbursement
    FOR EACH ROW EXECUTE FUNCTION refresh_obligation_from_disbursement();

CREATE TRIGGER disbursement_within_voucher
    BEFORE INSERT OR UPDATE OF amount, dv_id, status ON disbursement
    FOR EACH ROW EXECUTE FUNCTION assert_disbursement_within_voucher();


-- ── Views, rebuilt over the centavo columns ──────────────────────────────────
-- Identical to their definitions in 0008 and 0013. They now inherit
-- numeric(18,2) from the underlying columns, so a registry total and the
-- detail behind it are computed at the same precision.

CREATE VIEW registry_rapal AS
SELECT
    ap.tenant_id,
    ap.fund_id,
    ap.fiscal_year,
    ap.expense_class,
    ap.office_id,
    ap.id                                   AS appropriation_id,
    ap.ordinance_no,
    ap.account_id,
    ap.amount                               AS appropriation_amount,
    coalesce(sum(al.amount), 0)             AS allotment_amount,
    ap.amount - coalesce(sum(al.amount), 0) AS unreleased_amount
  FROM appropriation ap
  LEFT JOIN allotment al ON al.appropriation_id = ap.id
 GROUP BY ap.id;

CREATE VIEW registry_raod AS
SELECT
    al.tenant_id,
    al.fund_id,
    al.fiscal_year,
    al.expense_class,
    al.office_id,
    al.id                                       AS allotment_id,
    al.allotment_no,
    al.release_date,
    al.amount                                   AS allotment_amount,
    coalesce(sum(ob.amount)      FILTER (WHERE ob.status <> 'cancelled'), 0) AS obligation_amount,
    coalesce(sum(ob.paid_amount) FILTER (WHERE ob.status <> 'cancelled'), 0) AS disbursement_amount,
    al.amount
      - coalesce(sum(ob.amount)  FILTER (WHERE ob.status <> 'cancelled'), 0) AS unobligated_balance
  FROM allotment al
  LEFT JOIN obligation ob ON ob.allotment_id = al.id
 GROUP BY al.id;

COMMENT ON VIEW registry_raod IS
    'RAOD. Filter by expense_class for the per-class registry COA expects '
    '(RAOD-PS, RAOD-MOOE, RAOD-FE, RAOD-CO).';

CREATE VIEW trial_balance AS
SELECT je.tenant_id,
       je.fund_id,
       je.period_id,
       l.account_id,
       a.code  AS account_code,
       a.name  AS account_name,
       a.account_type,
       a.normal_balance,
       sum(l.debit)  AS debit_total,
       sum(l.credit) AS credit_total,
       CASE WHEN sum(l.debit) >= sum(l.credit)
            THEN sum(l.debit) - sum(l.credit) ELSE 0 END AS debit_balance,
       CASE WHEN sum(l.credit) > sum(l.debit)
            THEN sum(l.credit) - sum(l.debit) ELSE 0 END AS credit_balance
  FROM journal_entry_line l
  JOIN journal_entry je ON je.id = l.entry_id
  JOIN account a        ON a.id  = l.account_id
 WHERE je.status = 'posted'
 GROUP BY je.tenant_id, je.fund_id, je.period_id, l.account_id,
          a.code, a.name, a.account_type, a.normal_balance;
