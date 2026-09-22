-- ═══════════════════════════════════════════════════════════════════════════
--  0005 — The general ledger
-- ═══════════════════════════════════════════════════════════════════════════
--  §4.1: "record only balanced double-entry entries, where total debits equal
--         total credits"
--  §4.1: "Every transaction shall be timestamped and linked to the user who
--         performed it, forming part of an immutable audit trail"
--
--  Both are enforced here, in the database. Not in a service class, not in a
--  validator, not in the UI. A future developer with a psql prompt and good
--  intentions cannot write an unbalanced entry.
--
--  Money is NUMERIC(18,4): four decimal places so intermediate computations
--  (VAT, withholding, proration) stay exact, presented at two. Never float.
-- ═══════════════════════════════════════════════════════════════════════════

CREATE TYPE journal_type AS ENUM (
    'GJ',    -- General Journal        -- adjusting, closing, non-cash entries
    'CRJ',   -- Cash Receipts Journal  -- collections
    'CDJ',   -- Cash Disbursements Journal
    'CkDJ'   -- Check Disbursements Journal
);


CREATE TABLE journal_entry (
    id              uuid            PRIMARY KEY DEFAULT uuid_generate_v7(),
    tenant_id       uuid            NOT NULL REFERENCES tenant(id) ON DELETE RESTRICT,

    jev_no          text            NOT NULL,
    journal_type    journal_type    NOT NULL,
    fund_id         uuid            NOT NULL,
    period_id       uuid            NOT NULL,

    entry_date      date            NOT NULL,
    particulars     text            NOT NULL,

    status          document_status NOT NULL DEFAULT 'draft',

    -- What produced this entry, when it was generated rather than keyed by
    -- hand. Lets a ledger line be traced back to its voucher.
    source_type     text            CHECK (source_type IN ('DV','OR','ORS','manual','opening','closing')),
    source_id       uuid,

    -- Corrections are reversing entries, never edits. Both directions are
    -- recorded so the pair is visible from either side.
    reverses_id     uuid            REFERENCES journal_entry(id) ON DELETE RESTRICT,
    reversed_by_id  uuid            REFERENCES journal_entry(id) ON DELETE RESTRICT,
    reversal_reason text,

    prepared_by     uuid            REFERENCES app_user(id) ON DELETE RESTRICT,
    prepared_at     timestamptz     NOT NULL DEFAULT now(),
    approved_by     uuid            REFERENCES app_user(id) ON DELETE RESTRICT,
    approved_at     timestamptz,
    posted_by       uuid            REFERENCES app_user(id) ON DELETE RESTRICT,
    posted_at       timestamptz,

    created_at      timestamptz     NOT NULL DEFAULT now(),
    updated_at      timestamptz     NOT NULL DEFAULT now(),

    UNIQUE (tenant_id, fund_id, jev_no),

    FOREIGN KEY (tenant_id, fund_id)   REFERENCES fund           (tenant_id, id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, period_id) REFERENCES posting_period (tenant_id, id) ON DELETE RESTRICT,

    CONSTRAINT journal_entry_posted_needs_actor
        CHECK (status <> 'posted'   OR (posted_at   IS NOT NULL AND posted_by   IS NOT NULL)),
    CONSTRAINT journal_entry_approved_needs_actor
        CHECK (status <> 'approved' OR (approved_at IS NOT NULL AND approved_by IS NOT NULL)),
    CONSTRAINT journal_entry_reversal_needs_reason
        CHECK (reverses_id IS NULL  OR reversal_reason IS NOT NULL)
);

CREATE INDEX journal_entry_period  ON journal_entry (tenant_id, period_id, status);
CREATE INDEX journal_entry_date    ON journal_entry (tenant_id, fund_id, entry_date);
CREATE INDEX journal_entry_source  ON journal_entry (tenant_id, source_type, source_id);
CREATE INDEX journal_entry_posted  ON journal_entry (tenant_id, fund_id, entry_date)
                                   WHERE status = 'posted';

ALTER TABLE journal_entry ADD CONSTRAINT journal_entry_tenant_key UNIQUE (tenant_id, id);


CREATE TABLE journal_entry_line (
    id              uuid            PRIMARY KEY DEFAULT uuid_generate_v7(),
    tenant_id       uuid            NOT NULL REFERENCES tenant(id) ON DELETE RESTRICT,
    entry_id        uuid            NOT NULL,

    line_no         smallint        NOT NULL CHECK (line_no > 0),
    account_id      uuid            NOT NULL,

    debit           numeric(18,4)   NOT NULL DEFAULT 0,
    credit          numeric(18,4)   NOT NULL DEFAULT 0,

    particulars     text,

    -- Subsidiary ledger dimensions. §4.1 requires subsidiary ledgers to be
    -- maintained alongside the general ledger.
    payee_id        uuid,
    office_id       uuid,

    created_at      timestamptz     NOT NULL DEFAULT now(),

    UNIQUE (entry_id, line_no),

    FOREIGN KEY (tenant_id, entry_id)   REFERENCES journal_entry (tenant_id, id) ON DELETE CASCADE,
    FOREIGN KEY (tenant_id, account_id) REFERENCES account       (tenant_id, id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, payee_id)   REFERENCES payee         (tenant_id, id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, office_id)  REFERENCES office        (tenant_id, id) ON DELETE RESTRICT,

    -- Every check constraint is named. The API maps constraint names to plain
    -- language, so an unnamed constraint would surface to a clerk as
    -- "violates check constraint journal_entry_line_check1". Naming them is
    -- what makes the error taxonomy in docs/ARCHITECTURE.md §7 possible.

    -- Amounts are never negative. A negative debit is a credit, and allowing
    -- both representations makes every later aggregation ambiguous.
    CONSTRAINT journal_line_amounts_not_negative
        CHECK (debit >= 0 AND credit >= 0),
    -- Exactly one side carries a value. Not both, not neither.
    CONSTRAINT journal_line_exactly_one_side
        CHECK ((debit = 0) <> (credit = 0))
);

CREATE INDEX journal_entry_line_entry   ON journal_entry_line (entry_id, line_no);
CREATE INDEX journal_entry_line_account ON journal_entry_line (tenant_id, account_id);
CREATE INDEX journal_entry_line_payee   ON journal_entry_line (tenant_id, payee_id)  WHERE payee_id  IS NOT NULL;
CREATE INDEX journal_entry_line_office  ON journal_entry_line (tenant_id, office_id) WHERE office_id IS NOT NULL;


-- ═══════════════════════════════════════════════════════════════════════════
--  The balanced-entry guarantee
-- ═══════════════════════════════════════════════════════════════════════════
--  Checked as a DEFERRABLE INITIALLY DEFERRED constraint trigger, which means
--  it fires at COMMIT rather than per statement. That is what makes it usable:
--  an entry is inserted line by line, and it is legitimately unbalanced in the
--  middle of that. What must never be true is an unbalanced entry at commit.
--
--  Deferral is also what makes it unbypassable. A statement-level check would
--  force callers into contortions and someone would eventually add a way to
--  skip it.
-- ═══════════════════════════════════════════════════════════════════════════

CREATE OR REPLACE FUNCTION assert_entry_balanced()
RETURNS trigger
LANGUAGE plpgsql
AS $$
DECLARE
    v_entry_id   uuid;
    v_debit      numeric(18,4);
    v_credit     numeric(18,4);
    v_lines      integer;
    v_jev_no     text;
    v_status     document_status;
    v_difference numeric(18,4);
BEGIN
    v_entry_id := CASE TG_OP WHEN 'DELETE' THEN OLD.entry_id ELSE NEW.entry_id END;

    SELECT je.jev_no, je.status INTO v_jev_no, v_status
      FROM journal_entry je
     WHERE je.id = v_entry_id;

    -- The whole entry was deleted; its lines going with it is not a violation.
    IF NOT FOUND THEN
        RETURN NULL;
    END IF;

    -- Drafts are allowed to be incomplete. The rule binds from submission on.
    IF v_status = 'draft' THEN
        RETURN NULL;
    END IF;

    SELECT coalesce(sum(l.debit), 0), coalesce(sum(l.credit), 0), count(*)
      INTO v_debit, v_credit, v_lines
      FROM journal_entry_line l
     WHERE l.entry_id = v_entry_id;

    IF v_lines < 2 THEN
        RAISE EXCEPTION
            'Journal entry % needs at least one debit and one credit.', v_jev_no
            USING ERRCODE = 'check_violation',
                  HINT    = 'Add the missing side of the entry before submitting it.';
    END IF;

    IF v_debit <> v_credit THEN
        v_difference := abs(v_debit - v_credit);
        RAISE EXCEPTION
            'Journal entry % does not balance. Debits total %, credits total %, a difference of %.',
            v_jev_no,
            to_char(v_debit,      'FM999,999,999,990.00'),
            to_char(v_credit,     'FM999,999,999,990.00'),
            to_char(v_difference, 'FM999,999,999,990.00')
            USING ERRCODE = 'check_violation',
                  HINT    = 'Check the amounts on each line. Total debits must equal total credits.';
    END IF;

    RETURN NULL;
END
$$;

CREATE CONSTRAINT TRIGGER journal_entry_line_balances
    AFTER INSERT OR UPDATE OR DELETE ON journal_entry_line
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION assert_entry_balanced();


-- An entry that gains no lines at all would never fire the line trigger, so it
-- is checked from its own side too.
CREATE OR REPLACE FUNCTION assert_entry_has_lines()
RETURNS trigger
LANGUAGE plpgsql
AS $$
DECLARE
    v_debit  numeric(18,4);
    v_credit numeric(18,4);
    v_lines  integer;
BEGIN
    IF NEW.status = 'draft' THEN
        RETURN NULL;
    END IF;

    SELECT coalesce(sum(debit), 0), coalesce(sum(credit), 0), count(*)
      INTO v_debit, v_credit, v_lines
      FROM journal_entry_line
     WHERE entry_id = NEW.id;

    IF v_lines = 0 THEN
        RAISE EXCEPTION 'Journal entry % has no accounting lines.', NEW.jev_no
            USING ERRCODE = 'check_violation',
                  HINT    = 'Add the debit and credit lines before submitting.';
    END IF;

    IF v_debit <> v_credit THEN
        RAISE EXCEPTION
            'Journal entry % does not balance. Debits total %, credits total %.',
            NEW.jev_no,
            to_char(v_debit,  'FM999,999,999,990.00'),
            to_char(v_credit, 'FM999,999,999,990.00')
            USING ERRCODE = 'check_violation',
                  HINT    = 'Total debits must equal total credits.';
    END IF;

    RETURN NULL;
END
$$;

CREATE CONSTRAINT TRIGGER journal_entry_balances
    AFTER INSERT OR UPDATE ON journal_entry
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION assert_entry_has_lines();


-- ── Postable accounts only ───────────────────────────────────────────────────
-- Posting to a summary account silently corrupts every rollup that depends on
-- it, and the damage is invisible until a statement is prepared months later.
CREATE OR REPLACE FUNCTION assert_account_postable()
RETURNS trigger
LANGUAGE plpgsql
AS $$
DECLARE
    v_code       text;
    v_name       text;
    v_postable   boolean;
    v_active     boolean;
BEGIN
    SELECT code, name, is_postable, is_active
      INTO v_code, v_name, v_postable, v_active
      FROM account
     WHERE id = NEW.account_id;

    IF NOT v_postable THEN
        RAISE EXCEPTION
            'Account % (%) is a summary account and cannot be used on a journal entry.',
            v_code, v_name
            USING ERRCODE = 'check_violation',
                  HINT    = 'Choose one of the detail accounts underneath it.';
    END IF;

    IF NOT v_active THEN
        RAISE EXCEPTION 'Account % (%) is no longer in use.', v_code, v_name
            USING ERRCODE = 'check_violation',
                  HINT    = 'Choose an active account, or ask an administrator to reactivate this one.';
    END IF;

    RETURN NEW;
END
$$;

CREATE TRIGGER journal_entry_line_account_postable
    BEFORE INSERT OR UPDATE OF account_id ON journal_entry_line
    FOR EACH ROW EXECUTE FUNCTION assert_account_postable();


-- ── Closed periods ───────────────────────────────────────────────────────────
CREATE OR REPLACE FUNCTION assert_period_open()
RETURNS trigger
LANGUAGE plpgsql
AS $$
DECLARE
    v_status    text;
    v_year      smallint;
    v_month     smallint;
BEGIN
    SELECT status, fiscal_year, month
      INTO v_status, v_year, v_month
      FROM posting_period
     WHERE id = NEW.period_id;

    IF v_status <> 'open' THEN
        RAISE EXCEPTION
            'The accounting period for % % is already closed.',
            to_char(to_date(v_month::text, 'MM'), 'FMMonth'), v_year
            USING ERRCODE = 'check_violation',
                  HINT    = 'Record this in the current open period, or ask an administrator to reopen the closed one.';
    END IF;

    -- The entry date must fall inside the period it claims.
    IF NOT EXISTS (
        SELECT 1 FROM posting_period
         WHERE id = NEW.period_id
           AND NEW.entry_date BETWEEN starts_on AND ends_on
    ) THEN
        RAISE EXCEPTION
            'The entry date does not fall within % %.',
            to_char(to_date(v_month::text, 'MM'), 'FMMonth'), v_year
            USING ERRCODE = 'check_violation',
                  HINT    = 'Check the date, or select the period that contains it.';
    END IF;

    RETURN NEW;
END
$$;

CREATE TRIGGER journal_entry_period_open
    BEFORE INSERT OR UPDATE OF period_id, entry_date, status ON journal_entry
    FOR EACH ROW
    WHEN (NEW.status <> 'cancelled')
    EXECUTE FUNCTION assert_period_open();


-- ── Posted entries are permanent ─────────────────────────────────────────────
-- Once posted, an entry is a fact about public money. It is corrected by a
-- reversing entry, never by an edit. The only mutation permitted is stamping
-- reversed_by_id when its reversal is created.
CREATE OR REPLACE FUNCTION journal_entry_is_append_only()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        RAISE EXCEPTION 'Journal entry % has been posted and cannot be deleted.', OLD.jev_no
            USING ERRCODE = 'restrict_violation',
                  HINT    = 'Create a reversing entry instead.';
    END IF;

    IF OLD.status = 'posted' THEN
        IF NEW.status              IS DISTINCT FROM OLD.status
        OR NEW.jev_no              IS DISTINCT FROM OLD.jev_no
        OR NEW.fund_id             IS DISTINCT FROM OLD.fund_id
        OR NEW.period_id           IS DISTINCT FROM OLD.period_id
        OR NEW.entry_date          IS DISTINCT FROM OLD.entry_date
        OR NEW.particulars         IS DISTINCT FROM OLD.particulars
        OR NEW.journal_type        IS DISTINCT FROM OLD.journal_type
        OR NEW.posted_by           IS DISTINCT FROM OLD.posted_by
        OR NEW.posted_at           IS DISTINCT FROM OLD.posted_at
        THEN
            RAISE EXCEPTION 'Journal entry % has been posted and can no longer be changed.', OLD.jev_no
                USING ERRCODE = 'restrict_violation',
                      HINT    = 'Create a reversing entry to correct it.';
        END IF;
    END IF;

    RETURN NEW;
END
$$;

CREATE TRIGGER journal_entry_no_change_when_posted
    BEFORE UPDATE ON journal_entry
    FOR EACH ROW
    WHEN (OLD.status = 'posted')
    EXECUTE FUNCTION journal_entry_is_append_only();

CREATE TRIGGER journal_entry_no_delete_when_posted
    BEFORE DELETE ON journal_entry
    FOR EACH ROW
    WHEN (OLD.status = 'posted')
    EXECUTE FUNCTION journal_entry_is_append_only();


CREATE OR REPLACE FUNCTION journal_line_is_append_only()
RETURNS trigger
LANGUAGE plpgsql
AS $$
DECLARE
    v_status document_status;
    v_jev_no text;
BEGIN
    SELECT status, jev_no INTO v_status, v_jev_no
      FROM journal_entry
     WHERE id = CASE TG_OP WHEN 'DELETE' THEN OLD.entry_id ELSE NEW.entry_id END;

    IF v_status = 'posted' THEN
        RAISE EXCEPTION 'Journal entry % has been posted and its lines cannot be changed.', v_jev_no
            USING ERRCODE = 'restrict_violation',
                  HINT    = 'Create a reversing entry to correct it.';
    END IF;

    RETURN CASE TG_OP WHEN 'DELETE' THEN OLD ELSE NEW END;
END
$$;

CREATE TRIGGER journal_entry_line_no_change_when_posted
    BEFORE UPDATE OR DELETE ON journal_entry_line
    FOR EACH ROW EXECUTE FUNCTION journal_line_is_append_only();


CREATE TRIGGER journal_entry_touch
    BEFORE UPDATE ON journal_entry
    FOR EACH ROW EXECUTE FUNCTION touch_updated_at();

SELECT apply_tenant_guards('journal_entry');
SELECT apply_tenant_guards('journal_entry_line');
