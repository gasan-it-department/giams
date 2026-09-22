-- ═══════════════════════════════════════════════════════════════════════════
--  0011 — Disbursement vouchers, deductions, and payments
-- ═══════════════════════════════════════════════════════════════════════════
--  §4.1: "The system shall process Disbursement Vouchers and compute gross,
--         mandatory deductions, and net amounts on the server using exact
--         decimal arithmetic to guarantee centavo accuracy."
--
--  The load-bearing decision in this migration is that **rates are data, not
--  code**. BIR expanded withholding, GSIS, PhilHealth, and Pag-IBIG all change
--  by circular, sometimes mid-year. A rate hardcoded anywhere — in a trigger,
--  in Rust, in the client — means that reprinting a 2026 voucher in 2029
--  silently recomputes it at 2029 rates, and the reprint no longer matches the
--  paper in the file. That is an audit finding.
--
--  So: every rate is a row with an effectivity window, the rate in force on
--  the voucher date is resolved at computation time, and the resolved value is
--  **stored on the voucher line**. The voucher is then reproducible forever
--  regardless of what the tables say later.
-- ═══════════════════════════════════════════════════════════════════════════


-- ── Bank accounts ────────────────────────────────────────────────────────────
CREATE TABLE bank_account (
    id              uuid        PRIMARY KEY DEFAULT uuid_generate_v7(),
    tenant_id       uuid        NOT NULL REFERENCES tenant(id) ON DELETE RESTRICT,

    fund_id         uuid        NOT NULL,
    -- The cash-in-bank GL account this bank account is represented by.
    account_id      uuid        NOT NULL,

    bank_name       text        NOT NULL,
    branch          text,
    account_no      text        NOT NULL,
    account_name    text        NOT NULL,
    account_kind    text        NOT NULL DEFAULT 'current'
                                CHECK (account_kind IN ('current','savings','time_deposit')),

    is_active       boolean     NOT NULL DEFAULT true,
    created_at      timestamptz NOT NULL DEFAULT now(),
    updated_at      timestamptz NOT NULL DEFAULT now(),

    UNIQUE (tenant_id, bank_name, account_no),
    FOREIGN KEY (tenant_id, fund_id)    REFERENCES fund    (tenant_id, id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, account_id) REFERENCES account (tenant_id, id) ON DELETE RESTRICT
);

ALTER TABLE bank_account ADD CONSTRAINT bank_account_tenant_key UNIQUE (tenant_id, id);


-- ── Deduction types ──────────────────────────────────────────────────────────
-- What is withheld, and which liability account it lands in. The balance of
-- that account is what the municipality owes the agency at remittance.
CREATE TABLE deduction_type (
    id                   uuid        PRIMARY KEY DEFAULT uuid_generate_v7(),
    tenant_id            uuid        NOT NULL REFERENCES tenant(id) ON DELETE RESTRICT,

    code                 text        NOT NULL,
    name                 text        NOT NULL,
    remit_to             text        NOT NULL,

    -- Credited when the deduction is taken.
    liability_account_id uuid        NOT NULL,

    -- How the amount is arrived at.
    --   percent_of_gross  — rate × gross
    --   percent_of_base   — rate × a base the caller supplies (e.g. net of VAT)
    --   fixed             — a flat amount
    --   bracket           — a flat amount chosen by which bracket the base falls in
    basis                text        NOT NULL
                                     CHECK (basis IN ('percent_of_gross','percent_of_base','fixed','bracket')),

    -- Withholding applies to suppliers; premiums apply to employees. Used to
    -- offer the right deductions on the right kind of voucher.
    applies_to           text        NOT NULL DEFAULT 'supplier'
                                     CHECK (applies_to IN ('supplier','employee','both')),

    is_active            boolean     NOT NULL DEFAULT true,
    created_at           timestamptz NOT NULL DEFAULT now(),
    updated_at           timestamptz NOT NULL DEFAULT now(),

    UNIQUE (tenant_id, code),
    FOREIGN KEY (tenant_id, liability_account_id) REFERENCES account (tenant_id, id) ON DELETE RESTRICT
);

ALTER TABLE deduction_type ADD CONSTRAINT deduction_type_tenant_key UNIQUE (tenant_id, id);


-- ── Rates, with effectivity ──────────────────────────────────────────────────
CREATE TABLE deduction_rate (
    id                uuid          PRIMARY KEY DEFAULT uuid_generate_v7(),
    tenant_id         uuid          NOT NULL REFERENCES tenant(id) ON DELETE RESTRICT,
    deduction_type_id uuid          NOT NULL,

    -- Six decimal places: 0.015000 is 1.5%, and some BIR rates go finer.
    rate              numeric(9,6),
    fixed_amount      numeric(18,4),

    -- For bracketed schedules. NULL bounds mean open-ended.
    bracket_min       numeric(18,4),
    bracket_max       numeric(18,4),

    effective_from    date          NOT NULL,
    -- NULL means "still in force".
    effective_to      date,

    -- The circular or issuance that set it. An auditor asking "why 2%?"
    -- gets an answer from the row itself.
    authority         text,

    created_at        timestamptz   NOT NULL DEFAULT now(),

    FOREIGN KEY (tenant_id, deduction_type_id) REFERENCES deduction_type (tenant_id, id) ON DELETE RESTRICT,

    CONSTRAINT deduction_rate_has_a_value
        CHECK (rate IS NOT NULL OR fixed_amount IS NOT NULL),
    CONSTRAINT deduction_rate_window_is_ordered
        CHECK (effective_to IS NULL OR effective_to >= effective_from),
    CONSTRAINT deduction_rate_bracket_is_ordered
        CHECK (bracket_max IS NULL OR bracket_min IS NULL OR bracket_max >= bracket_min)
);

CREATE INDEX deduction_rate_lookup
    ON deduction_rate (deduction_type_id, effective_from DESC);

/*
 * Two rates for the same deduction and bracket may not overlap in time.
 *
 * Without this, "the rate on 15 March" has two answers, and which one a
 * voucher gets depends on row order. An exclusion constraint makes the
 * ambiguity impossible rather than something to notice later.
 */
CREATE EXTENSION IF NOT EXISTS btree_gist;

ALTER TABLE deduction_rate
    ADD CONSTRAINT deduction_rate_no_overlapping_windows
    EXCLUDE USING gist (
        deduction_type_id WITH =,
        coalesce(bracket_min, 0) WITH =,
        daterange(effective_from, effective_to, '[]') WITH &&
    );


/*
 * The rate in force for a deduction on a given date.
 *
 * Returns the rate row, not a computed amount: the caller stores what it
 * resolved so the voucher can be reproduced later.
 */
CREATE OR REPLACE FUNCTION resolve_deduction_rate(
    p_deduction_type_id uuid,
    p_on_date           date,
    p_base              numeric DEFAULT NULL
)
RETURNS TABLE (
    rate_id      uuid,
    rate         numeric,
    fixed_amount numeric,
    authority    text
)
LANGUAGE sql
STABLE
AS $$
    SELECT dr.id, dr.rate, dr.fixed_amount, dr.authority
      FROM deduction_rate dr
     WHERE dr.deduction_type_id = p_deduction_type_id
       AND p_on_date >= dr.effective_from
       AND (dr.effective_to IS NULL OR p_on_date <= dr.effective_to)
       AND (dr.bracket_min IS NULL OR p_base IS NULL OR p_base >= dr.bracket_min)
       AND (dr.bracket_max IS NULL OR p_base IS NULL OR p_base <= dr.bracket_max)
     ORDER BY dr.effective_from DESC
     LIMIT 1
$$;

COMMENT ON FUNCTION resolve_deduction_rate IS
    'The rate in force on a date. Callers must store what this returns on the '
    'voucher line, so reprinting an old voucher reproduces the old figures.';


-- ── Disbursement vouchers ────────────────────────────────────────────────────
CREATE TABLE disbursement_voucher (
    id                uuid            PRIMARY KEY DEFAULT uuid_generate_v7(),
    tenant_id         uuid            NOT NULL REFERENCES tenant(id) ON DELETE RESTRICT,

    dv_no             text            NOT NULL,
    fund_id           uuid            NOT NULL,
    fiscal_year       smallint        NOT NULL,
    period_id         uuid            NOT NULL,

    -- Every disbursement traces to an obligation, which traces to an
    -- allotment, which traces to an appropriation ordinance. RA 7160 §305.
    obligation_id     uuid            NOT NULL,
    payee_id          uuid            NOT NULL,
    office_id         uuid            NOT NULL,

    dv_date           date            NOT NULL,
    particulars       text            NOT NULL,

    gross_amount      numeric(18,4)   NOT NULL CHECK (gross_amount > 0),
    -- Maintained by trigger from the deduction lines. Not settable directly.
    total_deductions  numeric(18,4)   NOT NULL DEFAULT 0 CHECK (total_deductions >= 0),
    -- Derived, so gross - deductions = net can never drift out of agreement.
    net_amount        numeric(18,4)   GENERATED ALWAYS AS (gross_amount - total_deductions) STORED,

    status            document_status NOT NULL DEFAULT 'draft',
    cancelled_reason  text,

    prepared_by       uuid            REFERENCES app_user(id) ON DELETE RESTRICT,
    prepared_at       timestamptz,
    certified_by      uuid            REFERENCES app_user(id) ON DELETE RESTRICT,
    certified_at      timestamptz,
    approved_by       uuid            REFERENCES app_user(id) ON DELETE RESTRICT,
    approved_at       timestamptz,

    created_at        timestamptz     NOT NULL DEFAULT now(),
    updated_at        timestamptz     NOT NULL DEFAULT now(),

    UNIQUE (tenant_id, fund_id, fiscal_year, dv_no),

    FOREIGN KEY (tenant_id, fund_id)       REFERENCES fund           (tenant_id, id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, period_id)     REFERENCES posting_period (tenant_id, id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, obligation_id) REFERENCES obligation     (tenant_id, id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, payee_id)      REFERENCES payee          (tenant_id, id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, office_id)     REFERENCES office         (tenant_id, id) ON DELETE RESTRICT,

    CONSTRAINT dv_deductions_within_gross
        CHECK (total_deductions <= gross_amount),
    CONSTRAINT dv_approved_needs_actor
        CHECK (status <> 'approved' OR (approved_by IS NOT NULL AND approved_at IS NOT NULL)),
    CONSTRAINT dv_cancellation_needs_reason
        CHECK (status <> 'cancelled' OR cancelled_reason IS NOT NULL)
);

CREATE INDEX dv_register     ON disbursement_voucher (tenant_id, fund_id, fiscal_year, dv_date DESC);
CREATE INDEX dv_by_obligation ON disbursement_voucher (obligation_id) WHERE status <> 'cancelled';
CREATE INDEX dv_by_payee     ON disbursement_voucher (tenant_id, payee_id);

ALTER TABLE disbursement_voucher ADD CONSTRAINT dv_tenant_key UNIQUE (tenant_id, id);


-- ── Deduction lines ──────────────────────────────────────────────────────────
-- The resolved rate is stored, not referenced. Migration 0011's whole point.
CREATE TABLE dv_deduction (
    id                uuid          PRIMARY KEY DEFAULT uuid_generate_v7(),
    tenant_id         uuid          NOT NULL REFERENCES tenant(id) ON DELETE RESTRICT,
    dv_id             uuid          NOT NULL,
    deduction_type_id uuid          NOT NULL,

    base_amount       numeric(18,4) NOT NULL CHECK (base_amount >= 0),
    -- What was in force on the voucher date, copied here at computation time.
    rate_applied      numeric(9,6),
    fixed_applied     numeric(18,4),
    rate_id           uuid          REFERENCES deduction_rate(id) ON DELETE RESTRICT,
    authority         text,

    amount            numeric(18,4) NOT NULL CHECK (amount >= 0),

    created_at        timestamptz   NOT NULL DEFAULT now(),

    UNIQUE (dv_id, deduction_type_id),

    FOREIGN KEY (tenant_id, dv_id)             REFERENCES disbursement_voucher (tenant_id, id) ON DELETE CASCADE,
    FOREIGN KEY (tenant_id, deduction_type_id) REFERENCES deduction_type        (tenant_id, id) ON DELETE RESTRICT
);

CREATE INDEX dv_deduction_by_dv ON dv_deduction (dv_id);


/*
 * Keeps the voucher's total in step with its lines.
 *
 * `net_amount` is a generated column over `total_deductions`, so maintaining
 * this one figure keeps gross, deductions, and net in agreement by
 * construction — there is no path that updates one without the others.
 */
CREATE OR REPLACE FUNCTION refresh_dv_total_deductions()
RETURNS trigger
LANGUAGE plpgsql
AS $$
DECLARE
    v_dv_id uuid := CASE TG_OP WHEN 'DELETE' THEN OLD.dv_id ELSE NEW.dv_id END;
BEGIN
    UPDATE disbursement_voucher
       SET total_deductions = coalesce(
               (SELECT sum(amount) FROM dv_deduction WHERE dv_id = v_dv_id), 0)
     WHERE id = v_dv_id;

    RETURN NULL;
END
$$;

CREATE TRIGGER dv_deduction_refreshes_total
    AFTER INSERT OR UPDATE OR DELETE ON dv_deduction
    FOR EACH ROW EXECUTE FUNCTION refresh_dv_total_deductions();


-- ── Payments ─────────────────────────────────────────────────────────────────
CREATE TABLE disbursement (
    id                uuid          PRIMARY KEY DEFAULT uuid_generate_v7(),
    tenant_id         uuid          NOT NULL REFERENCES tenant(id) ON DELETE RESTRICT,
    dv_id             uuid          NOT NULL,

    mode              text          NOT NULL CHECK (mode IN ('check','ada','cash')),
    -- Check or ADA number. Cash payments have neither.
    instrument_no     text,
    bank_account_id   uuid,

    disbursement_date date          NOT NULL,
    amount            numeric(18,4) NOT NULL CHECK (amount > 0),

    status            text          NOT NULL DEFAULT 'issued'
                                    CHECK (status IN ('issued','cancelled','stale')),
    cancelled_reason  text,

    released_to       text,
    released_at       timestamptz,

    created_by        uuid          REFERENCES app_user(id) ON DELETE RESTRICT,
    created_at        timestamptz   NOT NULL DEFAULT now(),
    updated_at        timestamptz   NOT NULL DEFAULT now(),

    FOREIGN KEY (tenant_id, dv_id)           REFERENCES disbursement_voucher (tenant_id, id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, bank_account_id) REFERENCES bank_account         (tenant_id, id) ON DELETE RESTRICT,

    CONSTRAINT disbursement_check_needs_bank
        CHECK (mode = 'cash' OR (bank_account_id IS NOT NULL AND instrument_no IS NOT NULL)),
    CONSTRAINT disbursement_cancellation_needs_reason
        CHECK (status <> 'cancelled' OR cancelled_reason IS NOT NULL)
);

-- A check number must be unique within its bank account. Reusing one is how a
-- reconciliation becomes unmatchable.
CREATE UNIQUE INDEX disbursement_instrument_unique
    ON disbursement (bank_account_id, instrument_no)
    WHERE instrument_no IS NOT NULL AND status <> 'cancelled';

CREATE INDEX disbursement_by_dv   ON disbursement (dv_id);
CREATE INDEX disbursement_by_date ON disbursement (tenant_id, disbursement_date);


-- ── Keeping the obligation's running status true ─────────────────────────────
/*
 * The ORS form carries obligated / payable / paid / balance. Those figures are
 * derived from the vouchers and payments drawn against it, so they are
 * recomputed here rather than adjusted incrementally — an increment that is
 * ever missed stays wrong forever, while a recomputation is self-correcting.
 *
 * The CHECK constraints on `obligation` then enforce that vouchers never
 * exceed the obligation and payments never exceed what is payable.
 */
CREATE OR REPLACE FUNCTION refresh_obligation_status()
RETURNS trigger
LANGUAGE plpgsql
AS $$
DECLARE
    v_obligation_id uuid;
BEGIN
    v_obligation_id := CASE TG_OP WHEN 'DELETE' THEN OLD.obligation_id ELSE NEW.obligation_id END;

    UPDATE obligation ob
       SET payable_amount = coalesce((
               SELECT sum(dv.gross_amount)
                 FROM disbursement_voucher dv
                WHERE dv.obligation_id = ob.id
                  AND dv.status NOT IN ('draft','cancelled')), 0),
           paid_amount = coalesce((
               SELECT sum(d.amount)
                 FROM disbursement d
                 JOIN disbursement_voucher dv ON dv.id = d.dv_id
                WHERE dv.obligation_id = ob.id
                  AND d.status = 'issued'), 0)
     WHERE ob.id = v_obligation_id;

    RETURN NULL;
END
$$;

CREATE TRIGGER dv_refreshes_obligation
    AFTER INSERT OR UPDATE OF status, gross_amount, obligation_id OR DELETE
    ON disbursement_voucher
    FOR EACH ROW EXECUTE FUNCTION refresh_obligation_status();

/* Payments reach the obligation through their voucher. */
CREATE OR REPLACE FUNCTION refresh_obligation_from_disbursement()
RETURNS trigger
LANGUAGE plpgsql
AS $$
DECLARE
    v_dv_id uuid := CASE TG_OP WHEN 'DELETE' THEN OLD.dv_id ELSE NEW.dv_id END;
    v_obligation_id uuid;
BEGIN
    SELECT obligation_id INTO v_obligation_id
      FROM disbursement_voucher WHERE id = v_dv_id;

    IF v_obligation_id IS NULL THEN
        RETURN NULL;
    END IF;

    UPDATE obligation ob
       SET paid_amount = coalesce((
               SELECT sum(d.amount)
                 FROM disbursement d
                 JOIN disbursement_voucher dv ON dv.id = d.dv_id
                WHERE dv.obligation_id = ob.id
                  AND d.status = 'issued'), 0)
     WHERE ob.id = v_obligation_id;

    RETURN NULL;
END
$$;

CREATE TRIGGER disbursement_refreshes_obligation
    AFTER INSERT OR UPDATE OF status, amount, dv_id OR DELETE
    ON disbursement
    FOR EACH ROW EXECUTE FUNCTION refresh_obligation_from_disbursement();


/*
 * A payment may not exceed what the voucher is for.
 *
 * Partial payment is legitimate — a voucher can be settled in instalments —
 * but the sum of payments against a voucher can never exceed its net.
 */
CREATE OR REPLACE FUNCTION assert_disbursement_within_voucher()
RETURNS trigger
LANGUAGE plpgsql
AS $$
DECLARE
    v_net      numeric(18,4);
    v_paid     numeric(18,4);
    v_dv_no    text;
    v_status   document_status;
BEGIN
    IF NEW.status = 'cancelled' THEN
        RETURN NEW;
    END IF;

    SELECT net_amount, dv_no, status INTO v_net, v_dv_no, v_status
      FROM disbursement_voucher
     WHERE id = NEW.dv_id
       FOR UPDATE;

    IF v_status <> 'approved' AND v_status <> 'posted' THEN
        RAISE EXCEPTION
            'Voucher % has not been approved for payment yet.', v_dv_no
            USING ERRCODE = 'check_violation',
                  HINT    = 'It has to be approved before a check or ADA can be issued against it.';
    END IF;

    SELECT coalesce(sum(amount), 0) INTO v_paid
      FROM disbursement
     WHERE dv_id = NEW.dv_id AND status = 'issued' AND id <> NEW.id;

    IF NEW.amount > (v_net - v_paid) THEN
        RAISE EXCEPTION
            'This payment of % is more than the % still unpaid on voucher %.',
            to_char(NEW.amount,        'FM999,999,999,990.00'),
            to_char(v_net - v_paid,    'FM999,999,999,990.00'),
            v_dv_no
            USING ERRCODE = 'check_violation',
                  HINT    = 'Reduce the amount to what is still outstanding.';
    END IF;

    RETURN NEW;
END
$$;

CREATE TRIGGER disbursement_within_voucher
    BEFORE INSERT OR UPDATE OF amount, dv_id, status ON disbursement
    FOR EACH ROW EXECUTE FUNCTION assert_disbursement_within_voucher();


/* A voucher may not be raised against an obligation that is not yet certified. */
CREATE OR REPLACE FUNCTION assert_obligation_certified()
RETURNS trigger
LANGUAGE plpgsql
AS $$
DECLARE
    v_status document_status;
    v_ors_no text;
BEGIN
    IF NEW.status = 'draft' OR NEW.status = 'cancelled' THEN
        RETURN NEW;
    END IF;

    SELECT status, ors_no INTO v_status, v_ors_no
      FROM obligation WHERE id = NEW.obligation_id;

    IF v_status NOT IN ('certified','approved','posted') THEN
        RAISE EXCEPTION
            'Obligation request % has not been certified yet.', v_ors_no
            USING ERRCODE = 'check_violation',
                  HINT    = 'The Budget Officer and the Accountant both have to certify it first.';
    END IF;

    RETURN NEW;
END
$$;

CREATE TRIGGER dv_needs_certified_obligation
    BEFORE INSERT OR UPDATE OF status, obligation_id ON disbursement_voucher
    FOR EACH ROW EXECUTE FUNCTION assert_obligation_certified();


CREATE TRIGGER bank_account_touch   BEFORE UPDATE ON bank_account          FOR EACH ROW EXECUTE FUNCTION touch_updated_at();
CREATE TRIGGER deduction_type_touch BEFORE UPDATE ON deduction_type        FOR EACH ROW EXECUTE FUNCTION touch_updated_at();
CREATE TRIGGER dv_touch             BEFORE UPDATE ON disbursement_voucher  FOR EACH ROW EXECUTE FUNCTION touch_updated_at();
CREATE TRIGGER disbursement_touch   BEFORE UPDATE ON disbursement          FOR EACH ROW EXECUTE FUNCTION touch_updated_at();

SELECT apply_tenant_guards('bank_account');
SELECT apply_tenant_guards('deduction_type');
SELECT apply_tenant_guards('deduction_rate');
SELECT apply_tenant_guards('disbursement_voucher');
SELECT apply_tenant_guards('dv_deduction');
SELECT apply_tenant_guards('disbursement');
