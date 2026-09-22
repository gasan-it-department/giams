-- ═══════════════════════════════════════════════════════════════════════════
--  0016 — Bank reconciliation and record matching
-- ═══════════════════════════════════════════════════════════════════════════
--  §4.2: "The system shall automatically match disbursement and collection
--         records and flag unmatched or exception items for review."
--
--  Matching runs in three tiers, and the tier is recorded on every match. That
--  matters more than it looks: an auditor asking why two records were paired
--  gets an answer from the row, and an accountant can review the confident
--  matches quickly while spending attention on the uncertain ones.
--
--    1. exact        reference number + amount, within a few days
--    2. amount       a UNIQUE amount in a wider window, no reference to go on
--    3. candidate    ranked suggestions for a person to decide
--
--  Nothing in tier 3 is ever applied automatically. A wrong automatic match is
--  worse than no match: it looks reconciled, so nobody looks again.
-- ═══════════════════════════════════════════════════════════════════════════


CREATE TABLE bank_statement (
    id              uuid          PRIMARY KEY DEFAULT uuid_generate_v7(),
    tenant_id       uuid          NOT NULL REFERENCES tenant(id) ON DELETE RESTRICT,
    bank_account_id uuid          NOT NULL,

    statement_date  date          NOT NULL,
    period_start    date          NOT NULL,
    period_end      date          NOT NULL,

    opening_balance numeric(18,2) NOT NULL,
    closing_balance numeric(18,2) NOT NULL,

    imported_by     uuid          REFERENCES app_user(id) ON DELETE RESTRICT,
    imported_at     timestamptz   NOT NULL DEFAULT now(),
    source_filename text,

    UNIQUE (tenant_id, bank_account_id, period_start, period_end),
    FOREIGN KEY (tenant_id, bank_account_id) REFERENCES bank_account (tenant_id, id) ON DELETE RESTRICT,

    CONSTRAINT statement_period_is_ordered CHECK (period_end >= period_start)
);

ALTER TABLE bank_statement ADD CONSTRAINT bank_statement_tenant_key UNIQUE (tenant_id, id);


CREATE TABLE bank_statement_line (
    id           uuid          PRIMARY KEY DEFAULT uuid_generate_v7(),
    tenant_id    uuid          NOT NULL REFERENCES tenant(id) ON DELETE RESTRICT,
    statement_id uuid          NOT NULL,

    line_no      integer       NOT NULL,
    txn_date     date          NOT NULL,
    description  text          NOT NULL,
    -- Cheque number, deposit slip number, or whatever the bank prints. The
    -- single most useful field for matching, and often absent.
    reference    text,

    -- Money out of the account and money into it. One of the two, never both.
    withdrawal   numeric(18,2) NOT NULL DEFAULT 0 CHECK (withdrawal >= 0),
    deposit      numeric(18,2) NOT NULL DEFAULT 0 CHECK (deposit >= 0),

    -- What this line was matched to, once it has been.
    matched_kind text          CHECK (matched_kind IN ('disbursement', 'collection')),
    matched_id   uuid,
    match_tier   text          CHECK (match_tier IN ('exact', 'amount', 'manual')),
    match_score  numeric(5,4),
    matched_at   timestamptz,
    matched_by   uuid          REFERENCES app_user(id) ON DELETE RESTRICT,

    UNIQUE (statement_id, line_no),
    FOREIGN KEY (tenant_id, statement_id) REFERENCES bank_statement (tenant_id, id) ON DELETE CASCADE,

    CONSTRAINT statement_line_is_one_direction
        CHECK ((withdrawal = 0) <> (deposit = 0)),
    CONSTRAINT statement_line_match_is_complete
        CHECK (num_nonnulls(matched_kind, matched_id, match_tier, matched_at) IN (0, 4))
);

CREATE INDEX statement_line_by_statement ON bank_statement_line (statement_id, line_no);
CREATE INDEX statement_line_unmatched
    ON bank_statement_line (statement_id)
    WHERE matched_id IS NULL;
-- Matching searches by amount and date, in both directions.
CREATE INDEX statement_line_withdrawals
    ON bank_statement_line (tenant_id, withdrawal, txn_date)
    WHERE withdrawal > 0;
CREATE INDEX statement_line_deposits
    ON bank_statement_line (tenant_id, deposit, txn_date)
    WHERE deposit > 0;

/*
 * A book record may be matched to at most one statement line.
 *
 * Without this, the same cheque could be ticked off against two lines and the
 * reconciliation would still appear to balance.
 */
CREATE UNIQUE INDEX statement_line_one_match_per_record
    ON bank_statement_line (tenant_id, matched_kind, matched_id)
    WHERE matched_id IS NOT NULL;


-- ═══════════════════════════════════════════════════════════════════════════
--  The matching engine
-- ═══════════════════════════════════════════════════════════════════════════

/*
 * Tier 1 — exact.
 *
 * The reference on the statement carries the cheque or slip number, and it
 * agrees with a record of the same amount within a few days. This is as
 * certain as matching gets, so it is applied automatically.
 *
 * The date window exists because a cheque clears days after it is issued; the
 * amount and the number are what actually identify it.
 */
CREATE OR REPLACE FUNCTION match_statement_exact(p_statement_id uuid)
RETURNS integer
LANGUAGE plpgsql
AS $$
DECLARE
    v_matched integer := 0;
BEGIN
    -- Withdrawals against cheques and ADA.
    WITH candidate AS (
        SELECT l.id AS line_id, d.id AS disbursement_id,
               row_number() OVER (PARTITION BY l.id ORDER BY abs(d.disbursement_date - l.txn_date)) AS rank
          FROM bank_statement_line l
          JOIN disbursement d
            ON d.tenant_id      = l.tenant_id
           AND d.status         = 'issued'
           AND d.amount         = l.withdrawal
           AND d.instrument_no IS NOT NULL
           AND l.reference      IS NOT NULL
           -- Banks pad and prefix cheque numbers inconsistently, so compare
           -- the digits rather than the printed string.
           AND regexp_replace(d.instrument_no, '\D', '', 'g')
             = regexp_replace(l.reference,      '\D', '', 'g')
           AND regexp_replace(l.reference, '\D', '', 'g') <> ''
           AND l.txn_date BETWEEN d.disbursement_date - 30 AND d.disbursement_date + 30
         WHERE l.statement_id = p_statement_id
           AND l.matched_id IS NULL
           AND l.withdrawal > 0
           AND NOT EXISTS (
                 SELECT 1 FROM bank_statement_line m
                  WHERE m.tenant_id = l.tenant_id
                    AND m.matched_kind = 'disbursement'
                    AND m.matched_id = d.id)
    )
    UPDATE bank_statement_line l
       SET matched_kind = 'disbursement', matched_id = c.disbursement_id,
           match_tier = 'exact', match_score = 1.0, matched_at = now()
      FROM candidate c
     WHERE l.id = c.line_id AND c.rank = 1;

    GET DIAGNOSTICS v_matched = ROW_COUNT;

    -- Deposits against reports of collections.
    WITH candidate AS (
        SELECT l.id AS line_id, r.id AS rcd_id,
               row_number() OVER (PARTITION BY l.id ORDER BY abs(r.deposit_date - l.txn_date)) AS rank
          FROM bank_statement_line l
          JOIN report_of_collections r
            ON r.tenant_id        = l.tenant_id
           AND r.deposited_amount = l.deposit
           AND r.deposit_slip_no IS NOT NULL
           AND l.reference        IS NOT NULL
           AND regexp_replace(r.deposit_slip_no, '\D', '', 'g')
             = regexp_replace(l.reference,       '\D', '', 'g')
           AND regexp_replace(l.reference, '\D', '', 'g') <> ''
           AND l.txn_date BETWEEN r.deposit_date - 7 AND r.deposit_date + 7
         WHERE l.statement_id = p_statement_id
           AND l.matched_id IS NULL
           AND l.deposit > 0
           AND NOT EXISTS (
                 SELECT 1 FROM bank_statement_line m
                  WHERE m.tenant_id = l.tenant_id
                    AND m.matched_kind = 'collection'
                    AND m.matched_id = r.id)
    )
    UPDATE bank_statement_line l
       SET matched_kind = 'collection', matched_id = c.rcd_id,
           match_tier = 'exact', match_score = 1.0, matched_at = now()
      FROM candidate c
     WHERE l.id = c.line_id AND c.rank = 1;

    v_matched := v_matched + (SELECT count(*)::integer FROM bank_statement_line
                               WHERE statement_id = p_statement_id
                                 AND match_tier = 'exact' AND matched_kind = 'collection');

    RETURN v_matched;
END
$$;


/*
 * Tier 2 — amount within a window, but only when it is unambiguous.
 *
 * Used when the statement carries no usable reference. The rule that makes it
 * safe: the amount must match **exactly one** unmatched record in the window,
 * and that record must match **exactly one** line. Two payments of the same
 * amount in the same week are left for a person.
 *
 * This is the tier where an over-eager implementation does damage. A wrong
 * automatic match looks reconciled, so nobody looks at it again.
 */
CREATE OR REPLACE FUNCTION match_statement_by_amount(
    p_statement_id uuid,
    p_window_days  integer DEFAULT 7
)
RETURNS integer
LANGUAGE plpgsql
AS $$
DECLARE
    v_matched integer := 0;
BEGIN
    WITH pairing AS (
        SELECT l.id AS line_id, d.id AS disbursement_id
          FROM bank_statement_line l
          JOIN disbursement d
            ON d.tenant_id = l.tenant_id
           AND d.status    = 'issued'
           AND d.amount    = l.withdrawal
           AND l.txn_date BETWEEN d.disbursement_date - p_window_days
                              AND d.disbursement_date + p_window_days
         WHERE l.statement_id = p_statement_id
           AND l.matched_id IS NULL
           AND l.withdrawal > 0
           AND NOT EXISTS (
                 SELECT 1 FROM bank_statement_line m
                  WHERE m.tenant_id = l.tenant_id
                    AND m.matched_kind = 'disbursement'
                    AND m.matched_id = d.id)
    ),
    unambiguous AS (
        SELECT line_id, disbursement_id
          FROM pairing
         WHERE line_id IN (SELECT line_id FROM pairing GROUP BY line_id HAVING count(*) = 1)
           AND disbursement_id IN (
                 SELECT disbursement_id FROM pairing GROUP BY disbursement_id HAVING count(*) = 1)
    )
    UPDATE bank_statement_line l
       SET matched_kind = 'disbursement', matched_id = u.disbursement_id,
           match_tier = 'amount', match_score = 0.75, matched_at = now()
      FROM unambiguous u
     WHERE l.id = u.line_id;

    GET DIAGNOSTICS v_matched = ROW_COUNT;
    RETURN v_matched;
END
$$;


/*
 * Tier 3 — ranked candidates for a person to decide.
 *
 * Returns suggestions; changes nothing. The score combines how close the
 * amounts are, how close the dates are, and how similar the payee name is to
 * the statement description, using trigram similarity.
 */
CREATE OR REPLACE FUNCTION suggest_statement_matches(
    p_line_id uuid,
    p_limit   integer DEFAULT 5
)
RETURNS TABLE (
    kind            text,
    record_id       uuid,
    record_ref      text,
    record_date     date,
    record_amount   numeric,
    counterparty    text,
    score           numeric,
    why             text
)
LANGUAGE plpgsql
STABLE
AS $$
DECLARE
    v_line record;
BEGIN
    SELECT * INTO v_line FROM bank_statement_line WHERE id = p_line_id;

    IF NOT FOUND THEN
        RETURN;
    END IF;

    IF v_line.withdrawal > 0 THEN
        RETURN QUERY
        SELECT 'disbursement'::text,
               d.id,
               d.instrument_no,
               d.disbursement_date,
               d.amount,
               p.name,
               round((
                   -- Amounts within a peso count as equal; beyond that the
                   -- score falls away with the size of the difference.
                     0.50 * greatest(0, 1 - (abs(d.amount - v_line.withdrawal) / greatest(v_line.withdrawal, 1)))
                   + 0.25 * greatest(0, 1 - (abs(d.disbursement_date - v_line.txn_date)::numeric / 30))
                   + 0.25 * coalesce(similarity(p.name, v_line.description), 0)
               )::numeric, 4),
               concat_ws(', ',
                   CASE WHEN d.amount = v_line.withdrawal THEN 'same amount' END,
                   CASE WHEN abs(d.disbursement_date - v_line.txn_date) <= 3 THEN 'within three days' END,
                   CASE WHEN similarity(p.name, v_line.description) > 0.3 THEN 'payee name resembles the description' END
               )
          FROM disbursement d
          JOIN disbursement_voucher dv ON dv.id = d.dv_id
          JOIN payee p ON p.id = dv.payee_id
         WHERE d.tenant_id = v_line.tenant_id
           AND d.status = 'issued'
           AND NOT EXISTS (
                 SELECT 1 FROM bank_statement_line m
                  WHERE m.tenant_id = d.tenant_id
                    AND m.matched_kind = 'disbursement' AND m.matched_id = d.id)
           AND d.disbursement_date BETWEEN v_line.txn_date - 60 AND v_line.txn_date + 60
         ORDER BY 7 DESC
         LIMIT p_limit;
    ELSE
        RETURN QUERY
        SELECT 'collection'::text,
               r.id,
               r.deposit_slip_no,
               r.deposit_date,
               r.deposited_amount,
               r.rcd_no,
               round((
                     0.60 * greatest(0, 1 - (abs(r.deposited_amount - v_line.deposit) / greatest(v_line.deposit, 1)))
                   + 0.40 * greatest(0, 1 - (abs(r.deposit_date - v_line.txn_date)::numeric / 30))
               )::numeric, 4),
               concat_ws(', ',
                   CASE WHEN r.deposited_amount = v_line.deposit THEN 'same amount' END,
                   CASE WHEN abs(r.deposit_date - v_line.txn_date) <= 3 THEN 'within three days' END
               )
          FROM report_of_collections r
         WHERE r.tenant_id = v_line.tenant_id
           AND r.deposited_amount IS NOT NULL
           AND NOT EXISTS (
                 SELECT 1 FROM bank_statement_line m
                  WHERE m.tenant_id = r.tenant_id
                    AND m.matched_kind = 'collection' AND m.matched_id = r.id)
           AND r.deposit_date BETWEEN v_line.txn_date - 60 AND v_line.txn_date + 60
         ORDER BY 7 DESC
         LIMIT p_limit;
    END IF;
END
$$;

COMMENT ON FUNCTION suggest_statement_matches IS
    'Tier 3. Suggests only — never applies. A wrong automatic match looks '
    'reconciled, so nobody looks at it again.';


/* Runs tiers 1 and 2 over a statement and reports what happened. */
CREATE OR REPLACE FUNCTION match_bank_statement(p_statement_id uuid)
RETURNS TABLE (matched_exact integer, matched_by_amount integer, still_unmatched integer)
LANGUAGE plpgsql
AS $$
BEGIN
    matched_exact := match_statement_exact(p_statement_id);
    matched_by_amount := match_statement_by_amount(p_statement_id);

    SELECT count(*)::integer INTO still_unmatched
      FROM bank_statement_line
     WHERE statement_id = p_statement_id AND matched_id IS NULL;

    RETURN NEXT;
END
$$;


-- ── The reconciliation itself ────────────────────────────────────────────────
CREATE TABLE reconciliation (
    id                uuid          PRIMARY KEY DEFAULT uuid_generate_v7(),
    tenant_id         uuid          NOT NULL REFERENCES tenant(id) ON DELETE RESTRICT,
    bank_account_id   uuid          NOT NULL,
    statement_id      uuid          NOT NULL,

    as_of             date          NOT NULL,

    -- The two starting points that have to be brought into agreement.
    book_balance      numeric(18,2) NOT NULL,
    bank_balance      numeric(18,2) NOT NULL,

    status            text          NOT NULL DEFAULT 'in_progress'
                                    CHECK (status IN ('in_progress', 'completed')),
    completed_by      uuid          REFERENCES app_user(id) ON DELETE RESTRICT,
    completed_at      timestamptz,

    created_at        timestamptz   NOT NULL DEFAULT now(),
    updated_at        timestamptz   NOT NULL DEFAULT now(),

    UNIQUE (tenant_id, bank_account_id, as_of),
    FOREIGN KEY (tenant_id, bank_account_id) REFERENCES bank_account   (tenant_id, id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, statement_id)    REFERENCES bank_statement (tenant_id, id) ON DELETE RESTRICT,

    CONSTRAINT reconciliation_completion_is_recorded
        CHECK (status <> 'completed' OR (completed_by IS NOT NULL AND completed_at IS NOT NULL))
);

ALTER TABLE reconciliation ADD CONSTRAINT reconciliation_tenant_key UNIQUE (tenant_id, id);

/*
 * The differences that explain the gap between the two balances.
 *
 * Each kind adjusts a specific side, which is what lets the adjusted balances
 * be computed rather than typed:
 *
 *   outstanding_check    bank side, less    — issued, not yet presented
 *   deposit_in_transit   bank side, more    — deposited, not yet credited
 *   bank_charge          book side, less    — the bank took a fee we had not booked
 *   bank_credit          book side, more    — interest we had not booked
 *   book_error           book side, either  — our mistake
 *   bank_error           bank side, either  — theirs
 */
CREATE TABLE reconciling_item (
    id                uuid          PRIMARY KEY DEFAULT uuid_generate_v7(),
    tenant_id         uuid          NOT NULL REFERENCES tenant(id) ON DELETE RESTRICT,
    reconciliation_id uuid          NOT NULL,

    kind              text          NOT NULL CHECK (kind IN (
                                        'outstanding_check', 'deposit_in_transit',
                                        'bank_charge', 'bank_credit',
                                        'book_error', 'bank_error')),
    description       text          NOT NULL,
    reference         text,
    item_date         date,
    -- Signed: negative reduces the side it applies to.
    amount            numeric(18,2) NOT NULL CHECK (amount <> 0),

    created_at        timestamptz   NOT NULL DEFAULT now(),

    FOREIGN KEY (tenant_id, reconciliation_id) REFERENCES reconciliation (tenant_id, id) ON DELETE CASCADE
);

CREATE INDEX reconciling_item_by_reconciliation ON reconciling_item (reconciliation_id);

/*
 * The reconciliation statement, computed rather than stored.
 *
 * Book balance adjusted for what the bank knew and we did not; bank balance
 * adjusted for what we knew and the bank did not. When the two agree, the
 * account reconciles.
 */
CREATE VIEW reconciliation_summary AS
SELECT r.id AS reconciliation_id,
       r.tenant_id,
       r.bank_account_id,
       r.as_of,
       r.status,
       r.book_balance,
       r.bank_balance,
       coalesce(sum(i.amount) FILTER (WHERE i.kind IN ('bank_charge','bank_credit','book_error')), 0)
           AS book_adjustments,
       coalesce(sum(i.amount) FILTER (WHERE i.kind IN ('outstanding_check','deposit_in_transit','bank_error')), 0)
           AS bank_adjustments,
       r.book_balance
         + coalesce(sum(i.amount) FILTER (WHERE i.kind IN ('bank_charge','bank_credit','book_error')), 0)
           AS adjusted_book_balance,
       r.bank_balance
         + coalesce(sum(i.amount) FILTER (WHERE i.kind IN ('outstanding_check','deposit_in_transit','bank_error')), 0)
           AS adjusted_bank_balance,
       (r.book_balance
          + coalesce(sum(i.amount) FILTER (WHERE i.kind IN ('bank_charge','bank_credit','book_error')), 0))
       - (r.bank_balance
          + coalesce(sum(i.amount) FILTER (WHERE i.kind IN ('outstanding_check','deposit_in_transit','bank_error')), 0))
           AS difference
  FROM reconciliation r
  LEFT JOIN reconciling_item i ON i.reconciliation_id = r.id
 GROUP BY r.id;

/*
 * A reconciliation may not be completed while the two sides disagree.
 *
 * Completing an unbalanced reconciliation records that the account was checked
 * when it was not. The remaining difference has to be explained by a
 * reconciling item first — which is the entire point of the exercise.
 */
CREATE OR REPLACE FUNCTION assert_reconciliation_balances()
RETURNS trigger
LANGUAGE plpgsql
AS $$
DECLARE
    v_difference numeric(18,2);
BEGIN
    IF NEW.status <> 'completed' THEN
        RETURN NEW;
    END IF;

    SELECT difference INTO v_difference
      FROM reconciliation_summary WHERE reconciliation_id = NEW.id;

    IF coalesce(v_difference, 0) <> 0 THEN
        RAISE EXCEPTION
            'The book and bank balances still differ by %.',
            to_char(abs(v_difference), 'FM999,999,999,990.00')
            USING ERRCODE = 'check_violation',
                  HINT    = 'Add the outstanding cheques, deposits in transit, or bank charges that account for the difference.';
    END IF;

    RETURN NEW;
END
$$;

CREATE CONSTRAINT TRIGGER reconciliation_must_balance
    AFTER UPDATE OF status ON reconciliation
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION assert_reconciliation_balances();


CREATE TRIGGER reconciliation_touch BEFORE UPDATE ON reconciliation FOR EACH ROW EXECUTE FUNCTION touch_updated_at();

SELECT apply_tenant_guards('bank_statement');
SELECT apply_tenant_guards('bank_statement_line');
SELECT apply_tenant_guards('reconciliation');
SELECT apply_tenant_guards('reconciling_item');
