-- ═══════════════════════════════════════════════════════════════════════════
--  0013 — Posting to the general ledger, and closing a period
-- ═══════════════════════════════════════════════════════════════════════════
--  §4.1: "The system shall automatically post approved journal entry vouchers
--         to the general ledger and maintain the corresponding subsidiary
--         ledgers."
--
--  A disbursement produces TWO entries, not one, because under PPSAS the
--  expense is recognised when the obligation becomes payable and the cash
--  moves later:
--
--    On approval of the voucher      Dr  Expense                  gross
--                                      Cr  Due to BIR / GSIS / …    deductions
--                                      Cr  Accounts Payable         net
--
--    On issue of the check           Dr  Accounts Payable         net
--                                      Cr  Cash in Bank             net
--
--  Collapsing these into one entry would recognise the expense only when the
--  cheque is signed, which is cash-basis accounting and not what the GAM asks
--  for. It would also leave the payable invisible between the two dates —
--  precisely the balance an auditor looks for.
-- ═══════════════════════════════════════════════════════════════════════════


-- ── Period balance snapshots ─────────────────────────────────────────────────
-- Written when a period closes, so the trial balance for an old month is a
-- lookup rather than a re-aggregation of every entry ever made.
CREATE TABLE account_period_balance (
    tenant_id      uuid          NOT NULL REFERENCES tenant(id) ON DELETE RESTRICT,
    period_id      uuid          NOT NULL,
    account_id     uuid          NOT NULL,
    fund_id        uuid          NOT NULL,

    -- Movement within the period.
    debit_total    numeric(18,4) NOT NULL DEFAULT 0,
    credit_total   numeric(18,4) NOT NULL DEFAULT 0,
    -- Running balance at the close, signed to the account's normal side.
    ending_balance numeric(18,4) NOT NULL DEFAULT 0,

    snapshot_at    timestamptz   NOT NULL DEFAULT now(),

    PRIMARY KEY (period_id, account_id),
    FOREIGN KEY (tenant_id, period_id)  REFERENCES posting_period (tenant_id, id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, account_id) REFERENCES account        (tenant_id, id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, fund_id)    REFERENCES fund           (tenant_id, id) ON DELETE RESTRICT
);

CREATE INDEX account_period_balance_lookup
    ON account_period_balance (tenant_id, fund_id, account_id);


-- ── Posting a voucher ────────────────────────────────────────────────────────
/*
 * Builds and posts the accrual entry for an approved disbursement voucher.
 *
 * Returns the journal entry id. Idempotent: a voucher already posted returns
 * its existing entry rather than creating a second one. Double-posting is one
 * of the few ways to corrupt a ledger that the balanced-entry rule cannot
 * catch, because each duplicate is internally balanced.
 */
CREATE OR REPLACE FUNCTION post_voucher_accrual(
    p_dv_id     uuid,
    p_posted_by uuid
)
RETURNS uuid
LANGUAGE plpgsql
AS $$
DECLARE
    v_dv          record;
    v_entry_id    uuid;
    v_jev_no      text;
    v_expense_acct uuid;
    v_payable_acct uuid;
    v_line        smallint := 0;
    d             record;
BEGIN
    SELECT dv.*, ob.allotment_id
      INTO v_dv
      FROM disbursement_voucher dv
      JOIN obligation ob ON ob.id = dv.obligation_id
     WHERE dv.id = p_dv_id
       FOR UPDATE OF dv;

    IF NOT FOUND THEN
        RAISE EXCEPTION 'That voucher could not be found.'
            USING ERRCODE = 'no_data_found';
    END IF;

    -- Already posted: hand back what exists.
    SELECT id INTO v_entry_id
      FROM journal_entry
     WHERE source_type = 'DV' AND source_id = p_dv_id AND status = 'posted'
     LIMIT 1;

    IF v_entry_id IS NOT NULL THEN
        RETURN v_entry_id;
    END IF;

    IF v_dv.status <> 'approved' THEN
        RAISE EXCEPTION 'Voucher % has not been approved yet.', v_dv.dv_no
            USING ERRCODE = 'check_violation',
                  HINT    = 'It has to be approved before it can be posted to the general ledger.';
    END IF;

    -- The expense account comes from the appropriation the obligation charges,
    -- so the ledger and the budget registries always agree on what was spent
    -- on what.
    SELECT ap.account_id INTO v_expense_acct
      FROM allotment al
      JOIN appropriation ap ON ap.id = al.appropriation_id
     WHERE al.id = v_dv.allotment_id;

    SELECT id INTO v_payable_acct
      FROM account
     WHERE tenant_id = v_dv.tenant_id AND code = '2-01-01-010';

    IF v_payable_acct IS NULL THEN
        RAISE EXCEPTION 'The Accounts Payable account is not in the chart of accounts.'
            USING ERRCODE = 'no_data_found',
                  HINT    = 'Ask an administrator to add it before posting vouchers.';
    END IF;

    v_jev_no := next_document_number(v_dv.tenant_id, 'JEV', v_dv.fiscal_year,
                                     extract(month FROM v_dv.dv_date)::smallint, v_dv.fund_id);

    INSERT INTO journal_entry (tenant_id, jev_no, journal_type, fund_id, period_id,
                               entry_date, particulars, status, source_type, source_id,
                               prepared_by, prepared_at, approved_by, approved_at,
                               posted_by, posted_at)
         VALUES (v_dv.tenant_id, v_jev_no, 'GJ', v_dv.fund_id, v_dv.period_id,
                 v_dv.dv_date,
                 format('%s — %s', v_dv.dv_no, v_dv.particulars),
                 'draft', 'DV', p_dv_id,
                 v_dv.prepared_by, v_dv.prepared_at, v_dv.approved_by, v_dv.approved_at,
                 p_posted_by, now())
      RETURNING id INTO v_entry_id;

    -- Dr Expense, at gross.
    v_line := v_line + 1;
    INSERT INTO journal_entry_line (tenant_id, entry_id, line_no, account_id, debit, credit,
                                    particulars, payee_id, office_id)
         VALUES (v_dv.tenant_id, v_entry_id, v_line, v_expense_acct, v_dv.gross_amount, 0,
                 v_dv.particulars, v_dv.payee_id, v_dv.office_id);

    -- Cr each deduction to the agency it is owed to.
    FOR d IN
        SELECT dd.amount, dt.liability_account_id, dt.name
          FROM dv_deduction dd
          JOIN deduction_type dt ON dt.id = dd.deduction_type_id
         WHERE dd.dv_id = p_dv_id AND dd.amount > 0
         ORDER BY dt.code
    LOOP
        v_line := v_line + 1;
        INSERT INTO journal_entry_line (tenant_id, entry_id, line_no, account_id, debit, credit,
                                        particulars, payee_id, office_id)
             VALUES (v_dv.tenant_id, v_entry_id, v_line, d.liability_account_id, 0, d.amount,
                     d.name, v_dv.payee_id, v_dv.office_id);
    END LOOP;

    -- Cr Accounts Payable, at net.
    v_line := v_line + 1;
    INSERT INTO journal_entry_line (tenant_id, entry_id, line_no, account_id, debit, credit,
                                    particulars, payee_id, office_id)
         VALUES (v_dv.tenant_id, v_entry_id, v_line, v_payable_acct, 0, v_dv.net_amount,
                 'Payable to ' || coalesce((SELECT name FROM payee WHERE id = v_dv.payee_id), 'payee'),
                 v_dv.payee_id, v_dv.office_id);

    -- Posting last: the deferred balance trigger fires at COMMIT and will
    -- reject the whole transaction if these lines do not balance.
    UPDATE journal_entry SET status = 'posted' WHERE id = v_entry_id;
    UPDATE disbursement_voucher SET status = 'posted' WHERE id = p_dv_id;

    RETURN v_entry_id;
END
$$;

COMMENT ON FUNCTION post_voucher_accrual IS
    'Dr expense, Cr deductions, Cr payable. Idempotent — a voucher already '
    'posted returns its existing entry rather than duplicating it.';


/*
 * Posts the cash side when a check or ADA is issued.
 *
 *   Dr Accounts Payable   net
 *     Cr Cash in Bank       net
 */
CREATE OR REPLACE FUNCTION post_disbursement_payment(
    p_disbursement_id uuid,
    p_posted_by       uuid
)
RETURNS uuid
LANGUAGE plpgsql
AS $$
DECLARE
    v_d           record;
    v_entry_id    uuid;
    v_jev_no      text;
    v_payable_acct uuid;
    v_cash_acct   uuid;
    v_journal     journal_type;
BEGIN
    SELECT d.*, dv.fund_id, dv.fiscal_year, dv.period_id, dv.tenant_id,
           dv.payee_id, dv.office_id, dv.dv_no, dv.particulars
      INTO v_d
      FROM disbursement d
      JOIN disbursement_voucher dv ON dv.id = d.dv_id
     WHERE d.id = p_disbursement_id
       FOR UPDATE OF d;

    IF NOT FOUND THEN
        RAISE EXCEPTION 'That payment could not be found.' USING ERRCODE = 'no_data_found';
    END IF;

    SELECT id INTO v_entry_id
      FROM journal_entry
     WHERE source_type = 'DV' AND source_id = p_disbursement_id AND status = 'posted'
     LIMIT 1;

    IF v_entry_id IS NOT NULL THEN
        RETURN v_entry_id;
    END IF;

    SELECT id INTO v_payable_acct
      FROM account WHERE tenant_id = v_d.tenant_id AND code = '2-01-01-010';

    -- Cash payments credit the treasury; checks and ADA credit the bank
    -- account they were drawn on.
    IF v_d.mode = 'cash' THEN
        SELECT id INTO v_cash_acct
          FROM account WHERE tenant_id = v_d.tenant_id AND code = '1-01-01-010';
        v_journal := 'CDJ';
    ELSE
        SELECT account_id INTO v_cash_acct
          FROM bank_account WHERE id = v_d.bank_account_id;
        v_journal := CASE WHEN v_d.mode = 'check' THEN 'CkDJ'::journal_type ELSE 'CDJ'::journal_type END;
    END IF;

    IF v_cash_acct IS NULL OR v_payable_acct IS NULL THEN
        RAISE EXCEPTION 'The cash or payable account for this payment is not in the chart of accounts.'
            USING ERRCODE = 'no_data_found';
    END IF;

    v_jev_no := next_document_number(v_d.tenant_id, 'JEV', v_d.fiscal_year,
                                     extract(month FROM v_d.disbursement_date)::smallint, v_d.fund_id);

    INSERT INTO journal_entry (tenant_id, jev_no, journal_type, fund_id, period_id,
                               entry_date, particulars, status, source_type, source_id,
                               posted_by, posted_at)
         VALUES (v_d.tenant_id, v_jev_no, v_journal, v_d.fund_id, v_d.period_id,
                 v_d.disbursement_date,
                 format('Payment of %s — %s', v_d.dv_no, coalesce(v_d.instrument_no, 'cash')),
                 'draft', 'DV', p_disbursement_id, p_posted_by, now())
      RETURNING id INTO v_entry_id;

    INSERT INTO journal_entry_line (tenant_id, entry_id, line_no, account_id, debit, credit,
                                    particulars, payee_id, office_id)
         VALUES (v_d.tenant_id, v_entry_id, 1, v_payable_acct, v_d.amount, 0,
                 'Settlement of ' || v_d.dv_no, v_d.payee_id, v_d.office_id),
                (v_d.tenant_id, v_entry_id, 2, v_cash_acct, 0, v_d.amount,
                 coalesce(v_d.instrument_no, 'Cash payment'), v_d.payee_id, v_d.office_id);

    UPDATE journal_entry SET status = 'posted' WHERE id = v_entry_id;

    RETURN v_entry_id;
END
$$;


-- ── Closing a period ─────────────────────────────────────────────────────────
/*
 * Closes a month.
 *
 * Refuses while anything in the period is still unposted — closing over a
 * draft voucher silently excludes it from the trial balance, and the
 * difference surfaces months later with no obvious cause. Better to name the
 * documents and let someone deal with them.
 */
CREATE OR REPLACE FUNCTION close_posting_period(
    p_period_id uuid,
    p_closed_by uuid
)
RETURNS integer
LANGUAGE plpgsql
AS $$
DECLARE
    v_period    record;
    v_pending   integer;
    v_documents text;
    v_accounts  integer;
BEGIN
    SELECT * INTO v_period FROM posting_period WHERE id = p_period_id FOR UPDATE;

    IF NOT FOUND THEN
        RAISE EXCEPTION 'That accounting period could not be found.'
            USING ERRCODE = 'no_data_found';
    END IF;

    IF v_period.status <> 'open' THEN
        RAISE EXCEPTION 'The period for % % is already closed.',
            to_char(to_date(v_period.month::text, 'MM'), 'FMMonth'), v_period.fiscal_year
            USING ERRCODE = 'check_violation';
    END IF;

    -- Unposted journal entries.
    SELECT count(*) INTO v_pending
      FROM journal_entry
     WHERE period_id = p_period_id
       AND status NOT IN ('posted', 'cancelled', 'reversed');

    IF v_pending > 0 THEN
        RAISE EXCEPTION
            'There % still % journal entr% in this period that % not been posted.',
            CASE WHEN v_pending = 1 THEN 'is' ELSE 'are' END,
            v_pending,
            CASE WHEN v_pending = 1 THEN 'y' ELSE 'ies' END,
            CASE WHEN v_pending = 1 THEN 'has' ELSE 'have' END
            USING ERRCODE = 'check_violation',
                  HINT    = 'Post or cancel them before closing the month.';
    END IF;

    -- Source documents that have moved past draft but never reached the
    -- ledger. Checking only for journal entries misses these entirely: an
    -- approved voucher with no entry yet has nothing to find, so the month
    -- would close over it and leave it permanently out of the books.
    -- The list is named rather than counted, because "which ones?" is the
    -- first thing anyone asks.
    SELECT count(*), string_agg(dv_no, ', ' ORDER BY dv_no)
      INTO v_pending, v_documents
      FROM disbursement_voucher
     WHERE period_id = p_period_id
       AND status IN ('submitted', 'certified', 'approved');

    IF v_pending > 0 THEN
        RAISE EXCEPTION
            'Voucher% % % not been posted to the general ledger yet.',
            CASE WHEN v_pending = 1 THEN '' ELSE 's' END,
            v_documents,
            CASE WHEN v_pending = 1 THEN 'has' ELSE 'have' END
            USING ERRCODE = 'check_violation',
                  HINT    = 'Post or cancel them before closing the month, or the amounts will be left out of the books.';
    END IF;

    -- Snapshot every account that moved, plus its running balance.
    INSERT INTO account_period_balance
        (tenant_id, period_id, account_id, fund_id, debit_total, credit_total, ending_balance)
    SELECT v_period.tenant_id, p_period_id, l.account_id, je.fund_id,
           sum(l.debit), sum(l.credit),
           -- Signed to the account's normal side, so a positive balance always
           -- means "more of what this account is for".
           CASE WHEN a.normal_balance = 'debit'
                THEN sum(l.debit) - sum(l.credit)
                ELSE sum(l.credit) - sum(l.debit)
           END
      FROM journal_entry_line l
      JOIN journal_entry je ON je.id = l.entry_id
      JOIN account a        ON a.id  = l.account_id
     WHERE je.period_id = p_period_id
       AND je.status    = 'posted'
     GROUP BY l.account_id, je.fund_id, a.normal_balance
    ON CONFLICT (period_id, account_id) DO UPDATE
        SET debit_total    = excluded.debit_total,
            credit_total   = excluded.credit_total,
            ending_balance = excluded.ending_balance,
            snapshot_at    = now();

    GET DIAGNOSTICS v_accounts = ROW_COUNT;

    UPDATE posting_period
       SET status = 'closed', closed_at = now(), closed_by = p_closed_by
     WHERE id = p_period_id;

    RETURN v_accounts;
END
$$;

COMMENT ON FUNCTION close_posting_period IS
    'Closes a month and snapshots account balances. Refuses while any entry '
    'in the period is still unposted.';


-- ── Trial balance ────────────────────────────────────────────────────────────
-- Straight from posted lines, so it is always the ledger's own arithmetic and
-- never a separately maintained figure that can disagree with it.
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

SELECT apply_tenant_guards('account_period_balance');
