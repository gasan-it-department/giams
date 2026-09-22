-- ═══════════════════════════════════════════════════════════════════════════
--  0017 — Trial balance and the financial statements
-- ═══════════════════════════════════════════════════════════════════════════
--  §4.2: "The system shall produce a trial balance from posted general ledger
--         entries" and "generate the Statement of Financial Position,
--         Statement of Financial Performance, and a comparison of budget and
--         actual amounts, per fund."
--
--  Everything here is a function over posted journal lines. Nothing is stored
--  and maintained separately, because a stored statement can disagree with the
--  ledger it claims to summarise — and when it does, the ledger is right and
--  the statement has been wrong for however long nobody checked.
--
--  All four are computed **per fund**, as the GAM requires, with a NULL fund
--  meaning consolidated.
-- ═══════════════════════════════════════════════════════════════════════════


/*
 * The trial balance as at a date.
 *
 * The 0013 view covers a single period; this accumulates every posted entry up
 * to a date, which is what a statement needs. Balance-sheet accounts carry
 * forward across periods, so a Statement of Financial Position built from one
 * month's movement alone would be meaningless.
 */
CREATE OR REPLACE FUNCTION trial_balance_as_of(
    p_tenant_id uuid,
    p_as_of     date,
    p_fund_id   uuid DEFAULT NULL
)
RETURNS TABLE (
    account_id     uuid,
    account_code   text,
    account_name   text,
    account_type   text,
    normal_balance text,
    is_contra      boolean,
    debit_total    numeric(18,2),
    credit_total   numeric(18,2),
    balance        numeric(18,2)
)
LANGUAGE sql
STABLE
AS $$
    SELECT a.id, a.code, a.name, a.account_type::text, a.normal_balance, a.is_contra,
           sum(l.debit)::numeric(18,2),
           sum(l.credit)::numeric(18,2),
           -- Signed to the account's own normal side, so a positive figure
           -- always means "more of what this account is for".
           (CASE WHEN a.normal_balance = 'debit'
                 THEN sum(l.debit) - sum(l.credit)
                 ELSE sum(l.credit) - sum(l.debit)
            END)::numeric(18,2)
      FROM journal_entry_line l
      JOIN journal_entry je ON je.id = l.entry_id
      JOIN account a        ON a.id  = l.account_id
     WHERE je.tenant_id  = p_tenant_id
       AND je.status     = 'posted'
       AND je.entry_date <= p_as_of
       AND (p_fund_id IS NULL OR je.fund_id = p_fund_id)
     GROUP BY a.id, a.code, a.name, a.account_type, a.normal_balance, a.is_contra
    HAVING sum(l.debit) <> 0 OR sum(l.credit) <> 0
     ORDER BY a.code
$$;

COMMENT ON FUNCTION trial_balance_as_of IS
    'Cumulative trial balance to a date. NULL fund means consolidated.';


/*
 * Statement of Financial Position — the balance sheet.
 *
 * Assets, liabilities, and net assets/equity as at a date. The surplus for the
 * period is folded into equity: income and expense accounts are closed to
 * equity at year end, and a statement produced mid-year has to account for
 * them or it will not balance.
 */
CREATE OR REPLACE FUNCTION statement_financial_position(
    p_tenant_id uuid,
    p_as_of     date,
    p_fund_id   uuid DEFAULT NULL
)
RETURNS TABLE (
    section      text,
    account_code text,
    account_name text,
    amount       numeric(18,2),
    is_total     boolean
)
LANGUAGE plpgsql
STABLE
AS $$
DECLARE
    v_assets      numeric(18,2);
    v_liabilities numeric(18,2);
    v_equity      numeric(18,2);
    v_surplus     numeric(18,2);
BEGIN
    -- Detail lines. Contra accounts are shown as negatives within their group,
    -- which is how accumulated depreciation appears on a real balance sheet.
    RETURN QUERY
    SELECT CASE tb.account_type
               WHEN 'asset'     THEN 'Assets'
               WHEN 'liability' THEN 'Liabilities'
               ELSE 'Net assets and equity'
           END,
           tb.account_code, tb.account_name,
           CASE WHEN tb.is_contra THEN -tb.balance ELSE tb.balance END,
           false
      FROM trial_balance_as_of(p_tenant_id, p_as_of, p_fund_id) tb
     WHERE tb.account_type IN ('asset', 'liability', 'equity')
     ORDER BY tb.account_code;

    SELECT coalesce(sum(CASE WHEN tb.is_contra THEN -tb.balance ELSE tb.balance END), 0)
      INTO v_assets
      FROM trial_balance_as_of(p_tenant_id, p_as_of, p_fund_id) tb
     WHERE tb.account_type = 'asset';

    SELECT coalesce(sum(CASE WHEN tb.is_contra THEN -tb.balance ELSE tb.balance END), 0)
      INTO v_liabilities
      FROM trial_balance_as_of(p_tenant_id, p_as_of, p_fund_id) tb
     WHERE tb.account_type = 'liability';

    SELECT coalesce(sum(tb.balance), 0) INTO v_equity
      FROM trial_balance_as_of(p_tenant_id, p_as_of, p_fund_id) tb
     WHERE tb.account_type = 'equity';

    -- Income less expenses to date: the result not yet closed to equity.
    SELECT coalesce(sum(CASE WHEN tb.account_type = 'income'
                             THEN tb.balance ELSE -tb.balance END), 0)
      INTO v_surplus
      FROM trial_balance_as_of(p_tenant_id, p_as_of, p_fund_id) tb
     WHERE tb.account_type IN ('income', 'expense');

    RETURN QUERY VALUES
        ('Assets',                'TOTAL', 'Total assets',                      v_assets,      true),
        ('Liabilities',           'TOTAL', 'Total liabilities',                 v_liabilities, true),
        ('Net assets and equity', 'RESULT','Surplus (deficit) for the period',  v_surplus,     false),
        ('Net assets and equity', 'TOTAL', 'Total net assets and equity',       v_equity + v_surplus, true),
        ('Check',                 'CHECK', 'Assets less liabilities and equity',
             v_assets - v_liabilities - (v_equity + v_surplus), true);
END
$$;

COMMENT ON FUNCTION statement_financial_position IS
    'SFP. The final Check row must be zero; if it is not, the statement does '
    'not balance and should not be issued.';


/*
 * Statement of Financial Performance — income and expenses for a period.
 *
 * Unlike the balance sheet this covers a window rather than a point, because
 * income and expense accounts measure a period's activity.
 */
CREATE OR REPLACE FUNCTION statement_financial_performance(
    p_tenant_id uuid,
    p_from      date,
    p_to        date,
    p_fund_id   uuid DEFAULT NULL
)
RETURNS TABLE (
    section       text,
    account_code  text,
    account_name  text,
    expense_class text,
    amount        numeric(18,2),
    is_total      boolean
)
LANGUAGE plpgsql
STABLE
AS $$
DECLARE
    v_income  numeric(18,2);
    v_expense numeric(18,2);
BEGIN
    RETURN QUERY
    SELECT CASE a.account_type WHEN 'income' THEN 'Income' ELSE 'Expenses' END,
           a.code, a.name, a.expense_class::text,
           (CASE WHEN a.account_type = 'income'
                 THEN sum(l.credit) - sum(l.debit)
                 ELSE sum(l.debit)  - sum(l.credit)
            END)::numeric(18,2),
           false
      FROM journal_entry_line l
      JOIN journal_entry je ON je.id = l.entry_id
      JOIN account a        ON a.id  = l.account_id
     WHERE je.tenant_id  = p_tenant_id
       AND je.status     = 'posted'
       AND je.entry_date BETWEEN p_from AND p_to
       AND (p_fund_id IS NULL OR je.fund_id = p_fund_id)
       AND a.account_type IN ('income', 'expense')
     GROUP BY a.id, a.code, a.name, a.account_type, a.expense_class
    HAVING sum(l.debit) <> 0 OR sum(l.credit) <> 0
     ORDER BY 1 DESC, a.code;

    SELECT coalesce(sum(l.credit - l.debit), 0) INTO v_income
      FROM journal_entry_line l
      JOIN journal_entry je ON je.id = l.entry_id
      JOIN account a        ON a.id  = l.account_id
     WHERE je.tenant_id = p_tenant_id AND je.status = 'posted'
       AND je.entry_date BETWEEN p_from AND p_to
       AND (p_fund_id IS NULL OR je.fund_id = p_fund_id)
       AND a.account_type = 'income';

    SELECT coalesce(sum(l.debit - l.credit), 0) INTO v_expense
      FROM journal_entry_line l
      JOIN journal_entry je ON je.id = l.entry_id
      JOIN account a        ON a.id  = l.account_id
     WHERE je.tenant_id = p_tenant_id AND je.status = 'posted'
       AND je.entry_date BETWEEN p_from AND p_to
       AND (p_fund_id IS NULL OR je.fund_id = p_fund_id)
       AND a.account_type = 'expense';

    -- The NULL is `expense_class`: totals span every class, so naming one
    -- would be wrong. It is spelled out because omitting it silently shifts
    -- every following column by one.
    RETURN QUERY VALUES
        ('Income',   'TOTAL',  'Total income',                     NULL::text, v_income,             true),
        ('Expenses', 'TOTAL',  'Total expenses',                   NULL::text, v_expense,            true),
        ('Result',   'RESULT', 'Surplus (deficit) for the period', NULL::text, v_income - v_expense, true);
END
$$;


/*
 * Statement of Comparison of Budget and Actual Amounts — the SCBAA.
 *
 * Unique to the public sector and the one COA scrutinises hardest for LGUs,
 * because it is what ties the appropriation ordinance to what was actually
 * spent. It is also the statement this system can produce almost for free:
 * appropriations, allotments, obligations, and payments all live in the same
 * database, which is precisely what the manual process cannot do quickly.
 *
 * Reported per expense class, which is how the budget is authorised and how
 * the RAOD registries are kept.
 */
CREATE OR REPLACE FUNCTION statement_budget_vs_actual(
    p_tenant_id   uuid,
    p_fiscal_year smallint,
    p_fund_id     uuid DEFAULT NULL
)
RETURNS TABLE (
    expense_class    text,
    office_name      text,
    account_code     text,
    account_name     text,
    original_budget  numeric(18,2),
    final_budget     numeric(18,2),
    obligated        numeric(18,2),
    disbursed        numeric(18,2),
    unused_budget    numeric(18,2),
    utilisation_rate numeric(5,2)
)
LANGUAGE sql
STABLE
AS $$
    WITH budget AS (
        SELECT ap.expense_class, ap.office_id, ap.account_id,
               -- The annual appropriation is the original authority; the final
               -- budget includes supplementals passed during the year.
               sum(ap.amount) FILTER (WHERE ap.appropriation_type = 'annual') AS original,
               sum(ap.amount)                                                  AS final
          FROM appropriation ap
         WHERE ap.tenant_id = p_tenant_id
           AND ap.fiscal_year = p_fiscal_year
           AND (p_fund_id IS NULL OR ap.fund_id = p_fund_id)
         GROUP BY ap.expense_class, ap.office_id, ap.account_id
    ),
    actual AS (
        SELECT ap.expense_class, ap.office_id, ap.account_id,
               sum(ob.amount)      AS obligated,
               sum(ob.paid_amount) AS disbursed
          FROM obligation ob
          JOIN allotment al     ON al.id = ob.allotment_id
          JOIN appropriation ap ON ap.id = al.appropriation_id
         WHERE ob.tenant_id = p_tenant_id
           AND ob.fiscal_year = p_fiscal_year
           AND ob.status <> 'cancelled'
           AND (p_fund_id IS NULL OR ob.fund_id = p_fund_id)
         GROUP BY ap.expense_class, ap.office_id, ap.account_id
    )
    SELECT b.expense_class::text,
           o.name,
           a.code,
           a.name,
           coalesce(b.original, 0)::numeric(18,2),
           coalesce(b.final, 0)::numeric(18,2),
           coalesce(act.obligated, 0)::numeric(18,2),
           coalesce(act.disbursed, 0)::numeric(18,2),
           (coalesce(b.final, 0) - coalesce(act.obligated, 0))::numeric(18,2),
           CASE WHEN coalesce(b.final, 0) = 0 THEN 0
                ELSE round(100 * coalesce(act.obligated, 0) / b.final, 2)
           END
      FROM budget b
      JOIN office  o ON o.id = b.office_id
      JOIN account a ON a.id = b.account_id
      LEFT JOIN actual act
             ON act.expense_class = b.expense_class
            AND act.office_id     = b.office_id
            AND act.account_id    = b.account_id
     ORDER BY b.expense_class, o.name, a.code
$$;

COMMENT ON FUNCTION statement_budget_vs_actual IS
    'SCBAA. Ties the appropriation ordinance to what was obligated and paid, '
    'per expense class. The statement COA looks at first for an LGU.';
