-- ═══════════════════════════════════════════════════════════════════════════
--  0015 — Collections: official receipts and reports of collections
-- ═══════════════════════════════════════════════════════════════════════════
--  §4.2 requires the automated matching of disbursement AND collection records
--  against the bank statement. Disbursements already exist; this is the other
--  side.
--
--  The flow under the GAM:
--
--      Payor → Collecting Officer issues an OFFICIAL RECEIPT
--            → Report of Collections and Deposits (RCD) summarises the day
--            → deposit to the government depository bank
--            → Cash Receipts Journal, and thence the general ledger
--
--  The deposit is what appears on the bank statement — usually one line for a
--  day's takings rather than one per receipt. That is why the RCD exists as a
--  record in its own right and why reconciliation matches against it, not
--  against individual receipts.
-- ═══════════════════════════════════════════════════════════════════════════


-- ── Official receipts ────────────────────────────────────────────────────────
CREATE TABLE official_receipt (
    id                  uuid            PRIMARY KEY DEFAULT uuid_generate_v7(),
    tenant_id           uuid            NOT NULL REFERENCES tenant(id) ON DELETE RESTRICT,

    or_no               text            NOT NULL,
    fund_id             uuid            NOT NULL,
    fiscal_year         smallint        NOT NULL,
    period_id           uuid            NOT NULL,

    receipt_date        date            NOT NULL,
    -- A payor is often a walk-in with no standing record, so the name is kept
    -- on the receipt. `payee_id` links the ones who are on file.
    payor_name          text            NOT NULL,
    payee_id            uuid,
    particulars         text            NOT NULL,

    amount              numeric(18,2)   NOT NULL CHECK (amount > 0),

    collecting_officer  uuid            REFERENCES app_user(id) ON DELETE RESTRICT,
    status              document_status NOT NULL DEFAULT 'draft',
    cancelled_reason    text,

    -- Set once the receipt has been included in a report of collections.
    rcd_id              uuid,

    created_at          timestamptz     NOT NULL DEFAULT now(),
    updated_at          timestamptz     NOT NULL DEFAULT now(),

    UNIQUE (tenant_id, fund_id, fiscal_year, or_no),

    FOREIGN KEY (tenant_id, fund_id)   REFERENCES fund           (tenant_id, id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, period_id) REFERENCES posting_period (tenant_id, id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, payee_id)  REFERENCES payee          (tenant_id, id) ON DELETE RESTRICT,

    CONSTRAINT receipt_cancellation_needs_reason
        CHECK (status <> 'cancelled' OR cancelled_reason IS NOT NULL)
);

CREATE INDEX receipt_register ON official_receipt (tenant_id, fund_id, receipt_date DESC);
CREATE INDEX receipt_by_rcd   ON official_receipt (rcd_id) WHERE rcd_id IS NOT NULL;

ALTER TABLE official_receipt ADD CONSTRAINT receipt_tenant_key UNIQUE (tenant_id, id);

-- One receipt may cover several revenue accounts — a business permit taking in
-- the permit fee, the sanitary fee, and the fire fee at once.
CREATE TABLE receipt_line (
    id          uuid          PRIMARY KEY DEFAULT uuid_generate_v7(),
    tenant_id   uuid          NOT NULL REFERENCES tenant(id) ON DELETE RESTRICT,
    receipt_id  uuid          NOT NULL,
    line_no     smallint      NOT NULL CHECK (line_no > 0),
    account_id  uuid          NOT NULL,
    amount      numeric(18,2) NOT NULL CHECK (amount > 0),
    particulars text,

    UNIQUE (receipt_id, line_no),
    FOREIGN KEY (tenant_id, receipt_id) REFERENCES official_receipt (tenant_id, id) ON DELETE CASCADE,
    FOREIGN KEY (tenant_id, account_id) REFERENCES account          (tenant_id, id) ON DELETE RESTRICT
);

CREATE INDEX receipt_line_by_receipt ON receipt_line (receipt_id);

/*
 * The lines must add up to the receipt.
 *
 * Deferred, so the receipt and its lines can be written in any order within a
 * transaction — the same reasoning as the balanced-entry rule on journals.
 */
CREATE OR REPLACE FUNCTION assert_receipt_lines_total()
RETURNS trigger
LANGUAGE plpgsql
AS $$
DECLARE
    v_receipt_id uuid := CASE TG_OP WHEN 'DELETE' THEN OLD.receipt_id ELSE NEW.receipt_id END;
    v_amount     numeric(18,2);
    v_lines      numeric(18,2);
    v_or_no      text;
    v_status     document_status;
BEGIN
    SELECT amount, or_no, status INTO v_amount, v_or_no, v_status
      FROM official_receipt WHERE id = v_receipt_id;

    IF NOT FOUND OR v_status = 'draft' THEN
        RETURN NULL;
    END IF;

    SELECT coalesce(sum(amount), 0) INTO v_lines
      FROM receipt_line WHERE receipt_id = v_receipt_id;

    IF v_lines <> v_amount THEN
        RAISE EXCEPTION
            'The revenue accounts on receipt % add up to %, but the receipt is for %.',
            v_or_no,
            to_char(v_lines,  'FM999,999,999,990.00'),
            to_char(v_amount, 'FM999,999,999,990.00')
            USING ERRCODE = 'check_violation',
                  HINT    = 'Adjust the amounts so they match the total collected.';
    END IF;

    RETURN NULL;
END
$$;

CREATE CONSTRAINT TRIGGER receipt_lines_total
    AFTER INSERT OR UPDATE OR DELETE ON receipt_line
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION assert_receipt_lines_total();


-- ── Report of collections and deposits ───────────────────────────────────────
-- A day's collections, and the single deposit that carries them to the bank.
-- The deposit is what the bank statement shows, so this is what reconciliation
-- matches against.
CREATE TABLE report_of_collections (
    id                uuid          PRIMARY KEY DEFAULT uuid_generate_v7(),
    tenant_id         uuid          NOT NULL REFERENCES tenant(id) ON DELETE RESTRICT,

    rcd_no            text          NOT NULL,
    fund_id           uuid          NOT NULL,
    fiscal_year       smallint      NOT NULL,
    report_date       date          NOT NULL,

    collecting_officer uuid         REFERENCES app_user(id) ON DELETE RESTRICT,

    -- Maintained by trigger from the receipts included.
    total_collections numeric(18,2) NOT NULL DEFAULT 0 CHECK (total_collections >= 0),

    -- The deposit, once made. Null until then, which is what makes a report a
    -- deposit-in-transit at reconciliation time.
    bank_account_id   uuid,
    deposit_date      date,
    deposit_slip_no   text,
    deposited_amount  numeric(18,2) CHECK (deposited_amount IS NULL OR deposited_amount > 0),

    created_at        timestamptz   NOT NULL DEFAULT now(),
    updated_at        timestamptz   NOT NULL DEFAULT now(),

    UNIQUE (tenant_id, fund_id, fiscal_year, rcd_no),

    FOREIGN KEY (tenant_id, fund_id)         REFERENCES fund         (tenant_id, id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, bank_account_id) REFERENCES bank_account (tenant_id, id) ON DELETE RESTRICT,

    CONSTRAINT rcd_deposit_is_complete
        CHECK (num_nonnulls(bank_account_id, deposit_date, deposited_amount) IN (0, 3))
);

CREATE INDEX rcd_register ON report_of_collections (tenant_id, fund_id, report_date DESC);
CREATE INDEX rcd_undeposited ON report_of_collections (tenant_id, bank_account_id)
    WHERE deposit_date IS NULL;

ALTER TABLE report_of_collections ADD CONSTRAINT rcd_tenant_key UNIQUE (tenant_id, id);

ALTER TABLE official_receipt
    ADD CONSTRAINT receipt_rcd_fk
    FOREIGN KEY (tenant_id, rcd_id) REFERENCES report_of_collections (tenant_id, id) ON DELETE RESTRICT;

/* Keeps the report's total equal to the receipts it covers. */
CREATE OR REPLACE FUNCTION refresh_rcd_total()
RETURNS trigger
LANGUAGE plpgsql
AS $$
DECLARE
    v_rcd_id uuid;
BEGIN
    FOREACH v_rcd_id IN ARRAY ARRAY[
        CASE TG_OP WHEN 'INSERT' THEN NULL ELSE OLD.rcd_id END,
        CASE TG_OP WHEN 'DELETE' THEN NULL ELSE NEW.rcd_id END
    ]
    LOOP
        IF v_rcd_id IS NOT NULL THEN
            UPDATE report_of_collections
               SET total_collections = coalesce((
                       SELECT sum(amount) FROM official_receipt
                        WHERE rcd_id = v_rcd_id AND status <> 'cancelled'), 0)
             WHERE id = v_rcd_id;
        END IF;
    END LOOP;

    RETURN NULL;
END
$$;

CREATE TRIGGER receipt_refreshes_rcd
    AFTER INSERT OR UPDATE OF rcd_id, amount, status OR DELETE
    ON official_receipt
    FOR EACH ROW EXECUTE FUNCTION refresh_rcd_total();

/*
 * A deposit may not exceed the collections it is made up of.
 *
 * Depositing more than was collected means either a receipt is missing or the
 * deposit is wrong, and both need a person to look rather than the books to
 * quietly absorb it.
 */
CREATE OR REPLACE FUNCTION assert_deposit_matches_collections()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    IF NEW.deposited_amount IS NULL THEN
        RETURN NEW;
    END IF;

    IF NEW.deposited_amount > NEW.total_collections THEN
        RAISE EXCEPTION
            'The deposit of % is more than the % collected under report %.',
            to_char(NEW.deposited_amount,  'FM999,999,999,990.00'),
            to_char(NEW.total_collections, 'FM999,999,999,990.00'),
            NEW.rcd_no
            USING ERRCODE = 'check_violation',
                  HINT    = 'Check the deposit slip against the receipts included in this report.';
    END IF;

    RETURN NEW;
END
$$;

CREATE TRIGGER rcd_deposit_within_collections
    BEFORE INSERT OR UPDATE OF deposited_amount, total_collections
    ON report_of_collections
    FOR EACH ROW EXECUTE FUNCTION assert_deposit_matches_collections();


CREATE TRIGGER receipt_touch BEFORE UPDATE ON official_receipt       FOR EACH ROW EXECUTE FUNCTION touch_updated_at();
CREATE TRIGGER rcd_touch     BEFORE UPDATE ON report_of_collections  FOR EACH ROW EXECUTE FUNCTION touch_updated_at();

SELECT apply_tenant_guards('official_receipt');
SELECT apply_tenant_guards('receipt_line');
SELECT apply_tenant_guards('report_of_collections');
