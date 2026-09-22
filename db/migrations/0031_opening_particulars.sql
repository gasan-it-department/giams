-- ════════════════════════════════════════════════════════════════════════════
-- 0031 — Trimming the opening entry's description
--
-- `to_char(date, 'DD Month YYYY')` pads the month name to nine characters, so
-- the particulars read "Opening balances brought forward as at 01 January
-- 2026" with three stray spaces before the year. That text is printed in the
-- General Journal and shown on screen, so it is worth the migration.
--
-- `FMMonth` suppresses the padding. Only the format string changes.
-- ════════════════════════════════════════════════════════════════════════════

CREATE OR REPLACE FUNCTION import_opening_balances(
    p_tenant_id   uuid,
    p_fund_id     uuid,
    p_fiscal_year smallint,
    p_lines       jsonb,
    p_user_id     uuid
)
RETURNS TABLE (entry_id uuid, jev_no text, lines integer, total numeric)
LANGUAGE plpgsql
VOLATILE
AS $$
DECLARE
    v_entry_id   uuid;
    v_jev_no     text;
    v_period_id  uuid;
    v_starts_on  date;
    v_debit      numeric(18,2) := 0;
    v_credit     numeric(18,2) := 0;
    v_count      integer := 0;
    v_line       jsonb;
    v_account_id uuid;
    v_code       text;
    v_d          numeric(18,2);
    v_c          numeric(18,2);
BEGIN
    PERFORM set_config('app.tenant_id', p_tenant_id::text, true);

    IF EXISTS (
        SELECT 1 FROM journal_entry
         WHERE tenant_id = p_tenant_id
           AND fund_id = p_fund_id
           AND fiscal_year_of(entry_date) = p_fiscal_year
           AND source_type = 'opening'
    ) THEN
        RAISE EXCEPTION
            'Opening balances have already been recorded for this fund for %.', p_fiscal_year
            USING HINT = 'To replace them, reverse the existing opening entry first.';
    END IF;

    SELECT id, starts_on INTO v_period_id, v_starts_on
      FROM posting_period
     WHERE tenant_id = p_tenant_id
       AND fund_id = p_fund_id
       AND fiscal_year = p_fiscal_year
       AND month = 1;

    IF v_period_id IS NULL THEN
        RAISE EXCEPTION
            'January % has not been opened for this fund, so opening balances cannot be recorded.',
            p_fiscal_year
            USING HINT = 'Open the accounting period first.';
    END IF;

    -- Everything is checked before anything is written: a half-applied import
    -- is worse than a refused one, because the office cannot tell which
    -- balances arrived without checking all of them by hand.
    FOR v_line IN SELECT * FROM jsonb_array_elements(p_lines)
    LOOP
        v_code := trim(v_line ->> 'account_code');
        v_d    := coalesce((v_line ->> 'debit')::numeric(18,2), 0);
        v_c    := coalesce((v_line ->> 'credit')::numeric(18,2), 0);

        IF v_d < 0 OR v_c < 0 THEN
            RAISE EXCEPTION
                'Account % has a negative amount. Opening balances are entered as debits or credits, never as negatives.',
                v_code;
        END IF;

        IF (v_d <> 0 AND v_c <> 0) THEN
            RAISE EXCEPTION
                'Account % has both a debit and a credit. Each account takes one or the other.',
                v_code;
        END IF;

        IF (v_d = 0 AND v_c = 0) THEN
            CONTINUE;
        END IF;

        SELECT id INTO v_account_id
          FROM account
         WHERE tenant_id = p_tenant_id AND code = v_code;

        IF v_account_id IS NULL THEN
            RAISE EXCEPTION
                'There is no account with code % in the chart of accounts.', v_code
                USING HINT = 'Check the code against the chart, or add the account first.';
        END IF;

        IF NOT EXISTS (
            SELECT 1 FROM account WHERE id = v_account_id AND is_postable AND is_active
        ) THEN
            RAISE EXCEPTION
                'Account % cannot be posted to. It is either a heading or has been deactivated.',
                v_code;
        END IF;

        v_debit  := v_debit + v_d;
        v_credit := v_credit + v_c;
        v_count  := v_count + 1;
    END LOOP;

    IF v_count = 0 THEN
        RAISE EXCEPTION 'The file contains no balances to record.';
    END IF;

    IF v_debit <> v_credit THEN
        RAISE EXCEPTION
            'The opening balances do not balance. Debits total %, credits total %, a difference of %.',
            to_char(v_debit,  'FM999,999,999,990.00'),
            to_char(v_credit, 'FM999,999,999,990.00'),
            to_char(abs(v_debit - v_credit), 'FM999,999,999,990.00')
            USING HINT = 'This is a difference in the legacy books. Correct it there first — '
                         'carrying it in would make it permanent here.';
    END IF;

    v_jev_no := next_document_number(p_tenant_id, 'JEV', p_fiscal_year, 1::smallint, p_fund_id);

    INSERT INTO journal_entry
        (tenant_id, jev_no, journal_type, fund_id, period_id, entry_date,
         particulars, status, source_type, prepared_by, prepared_at,
         posted_by, posted_at)
    VALUES
        (p_tenant_id, v_jev_no, 'GJ', p_fund_id, v_period_id, v_starts_on,
         format('Opening balances brought forward as at %s',
                to_char(v_starts_on, 'FMDD FMMonth YYYY')),
         'draft', 'opening', p_user_id, now(),
         p_user_id, now())
    RETURNING id INTO v_entry_id;

    INSERT INTO journal_entry_line (tenant_id, entry_id, line_no, account_id, debit, credit)
    SELECT p_tenant_id,
           v_entry_id,
           row_number() OVER (ORDER BY a.code),
           a.id,
           coalesce((l ->> 'debit')::numeric(18,2), 0),
           coalesce((l ->> 'credit')::numeric(18,2), 0)
      FROM jsonb_array_elements(p_lines) l
      JOIN account a
        ON a.tenant_id = p_tenant_id AND a.code = trim(l ->> 'account_code')
     WHERE coalesce((l ->> 'debit')::numeric(18,2), 0) <> 0
        OR coalesce((l ->> 'credit')::numeric(18,2), 0) <> 0;

    -- Posted last, so the balanced-entry constraint sees a complete entry.
    UPDATE journal_entry SET status = 'posted' WHERE id = v_entry_id;

    RETURN QUERY SELECT v_entry_id, v_jev_no, v_count, v_debit;
END
$$;
