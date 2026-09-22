-- ═══════════════════════════════════════════════════════════════════════════
--  0009 — Starter chart of accounts
-- ═══════════════════════════════════════════════════════════════════════════
--  ⚠️  THIS IS A STARTER SUBSET, NOT THE OFFICIAL CHART.
--
--  The Revised Chart of Accounts (COA Circular 2013-002) runs to several
--  hundred accounts. What follows is the commonly used core, enough to record
--  ordinary transactions and to exercise the system, structured exactly as the
--  full chart is so that loading the rest is an insert and not a migration.
--
--  Before this system records a single real transaction, the complete list
--  from **Annex A of the GAM for LGUs, Volume III** must be loaded and every
--  code below verified against it. That is a Phase 11 data task and is
--  tracked as such. Codes here follow the published structure but have not
--  been reconciled line by line against the current annex.
--
--  Structure of an 8-digit code:  M-GG-SS-DDD
--      M    account group   1 asset · 2 liability · 3 equity · 4 income · 5 expense
--      GG   major group
--      SS   sub-major group
--      DDD  general ledger account (990 = "Other …")
-- ═══════════════════════════════════════════════════════════════════════════

CREATE OR REPLACE FUNCTION seed_chart_of_accounts(p_tenant_id uuid)
RETURNS integer
LANGUAGE plpgsql
AS $$
DECLARE
    v_inserted integer;
BEGIN
    INSERT INTO account (tenant_id, code, name, account_type, is_postable,
                         is_contra, requires_subsidiary, expense_class, is_cash)
    SELECT p_tenant_id, t.code, t.name, t.kind::account_type, t.postable,
           t.contra, t.subsidiary, t.class::expense_class, t.cash
      FROM (VALUES
        -- ── Assets ──────────────────────────────────────────────────────────
        ('1-00-00-000','Assets',                                  'asset',    false,false,false,NULL,false),
        ('1-01-01-010','Cash - Local Treasury',                   'asset',    true, false,false,NULL,true ),
        ('1-01-02-010','Cash in Bank - Local Currency, Current',  'asset',    true, false,false,NULL,true ),
        ('1-01-02-020','Cash in Bank - Local Currency, Savings',  'asset',    true, false,false,NULL,true ),
        ('1-03-01-010','Accounts Receivable',                     'asset',    true, false,true, NULL,false),
        ('1-03-01-011','Allowance for Impairment - Accounts Receivable','asset',true,true, false,NULL,false),
        ('1-03-02-010','Real Property Tax Receivable',            'asset',    true, false,true, NULL,false),
        ('1-04-04-010','Office Supplies Inventory',               'asset',    true, false,false,NULL,false),
        ('1-04-04-020','Fuel, Oil and Lubricants Inventory',      'asset',    true, false,false,NULL,false),
        ('1-07-04-010','Buildings',                               'asset',    true, false,false,NULL,false),
        ('1-07-04-011','Accumulated Depreciation - Buildings',    'asset',    true, true, false,NULL,false),
        ('1-07-05-020','Office Equipment',                        'asset',    true, false,false,NULL,false),
        ('1-07-05-021','Accumulated Depreciation - Office Equipment','asset', true, true, false,NULL,false),
        ('1-07-06-010','Motor Vehicles',                          'asset',    true, false,false,NULL,false),
        ('1-07-99-010','Construction in Progress - Infrastructure','asset',   true, false,true, NULL,false),

        -- ── Liabilities ─────────────────────────────────────────────────────
        ('2-00-00-000','Liabilities',                             'liability',false,false,false,NULL,false),
        ('2-01-01-010','Accounts Payable',                        'liability',true, false,true, NULL,false),
        ('2-01-02-010','Due to Officers and Employees',           'liability',true, false,true, NULL,false),
        -- The mandatory deduction accounts. Every disbursement voucher with
        -- withholding credits one of these, and the balance is what is owed to
        -- the agency at remittance.
        ('2-02-01-010','Due to BIR',                              'liability',true, false,false,NULL,false),
        ('2-02-01-020','Due to GSIS',                             'liability',true, false,false,NULL,false),
        ('2-02-01-030','Due to Pag-IBIG',                         'liability',true, false,false,NULL,false),
        ('2-02-01-040','Due to PhilHealth',                       'liability',true, false,false,NULL,false),
        ('2-04-01-010','Trust Liabilities',                       'liability',true, false,true, NULL,false),
        ('2-05-01-010','Guaranty/Security Deposits Payable',      'liability',true, false,true, NULL,false),

        -- ── Equity ──────────────────────────────────────────────────────────
        ('3-00-00-000','Net Assets/Equity',                       'equity',   false,false,false,NULL,false),
        ('3-01-01-010','Government Equity',                       'equity',   true, false,false,NULL,false),
        ('3-01-01-020','Prior Period Adjustment',                 'equity',   true, false,false,NULL,false),

        -- ── Income ──────────────────────────────────────────────────────────
        ('4-00-00-000','Income',                                  'income',   false,false,false,NULL,false),
        ('4-01-02-040','Real Property Tax - Basic',               'income',   true, false,false,NULL,false),
        ('4-01-02-050','Special Education Tax',                   'income',   true, false,false,NULL,false),
        ('4-01-03-030','Business Tax',                            'income',   true, false,false,NULL,false),
        -- Renamed from Internal Revenue Allotment after Mandanas-Garcia.
        ('4-01-04-010','Share from National Tax Allotment',       'income',   true, false,false,NULL,false),
        ('4-02-01-040','Permit Fees',                             'income',   true, false,false,NULL,false),
        ('4-02-02-220','Clearance and Certification Fees',        'income',   true, false,false,NULL,false),
        ('4-06-01-010','Interest Income',                         'income',   true, false,false,NULL,false),

        -- ── Expenses: Personal Services ─────────────────────────────────────
        ('5-00-00-000','Expenses',                                'expense',  false,false,false,NULL,false),
        ('5-01-01-010','Salaries and Wages - Regular',            'expense',  true, false,false,'PS',  false),
        ('5-01-01-020','Salaries and Wages - Casual/Contractual', 'expense',  true, false,false,'PS',  false),
        ('5-01-02-010','Personal Economic Relief Allowance',      'expense',  true, false,false,'PS',  false),
        ('5-01-02-020','Representation Allowance',                'expense',  true, false,false,'PS',  false),
        ('5-01-02-100','Year End Bonus',                          'expense',  true, false,false,'PS',  false),
        ('5-01-03-010','Retirement and Life Insurance Premiums',  'expense',  true, false,false,'PS',  false),
        ('5-01-03-020','Pag-IBIG Contributions',                  'expense',  true, false,false,'PS',  false),
        ('5-01-03-030','PhilHealth Contributions',                'expense',  true, false,false,'PS',  false),

        -- ── Expenses: MOOE ──────────────────────────────────────────────────
        ('5-02-01-010','Traveling Expenses - Local',              'expense',  true, false,false,'MOOE',false),
        ('5-02-02-010','Training Expenses',                       'expense',  true, false,false,'MOOE',false),
        ('5-02-03-010','Office Supplies Expenses',                'expense',  true, false,false,'MOOE',false),
        ('5-02-03-090','Fuel, Oil and Lubricants Expenses',       'expense',  true, false,false,'MOOE',false),
        ('5-02-03-990','Other Supplies and Materials Expenses',   'expense',  true, false,false,'MOOE',false),
        ('5-02-04-010','Water Expenses',                          'expense',  true, false,false,'MOOE',false),
        ('5-02-04-020','Electricity Expenses',                    'expense',  true, false,false,'MOOE',false),
        ('5-02-05-020','Telephone Expenses',                      'expense',  true, false,false,'MOOE',false),
        ('5-02-05-030','Internet Subscription Expenses',          'expense',  true, false,false,'MOOE',false),
        ('5-02-11-990','Other General Services',                  'expense',  true, false,false,'MOOE',false),
        ('5-02-13-040','Repairs and Maintenance - Buildings',     'expense',  true, false,false,'MOOE',false),
        ('5-02-13-060','Repairs and Maintenance - Motor Vehicles','expense',  true, false,false,'MOOE',false),
        ('5-02-99-990','Other Maintenance and Operating Expenses','expense',  true, false,false,'MOOE',false),

        -- ── Expenses: Financial ─────────────────────────────────────────────
        ('5-03-01-020','Bank Charges',                            'expense',  true, false,false,'FE',  false),
        ('5-03-01-040','Interest Expenses',                       'expense',  true, false,false,'FE',  false),

        -- ── Expenses: non-cash ──────────────────────────────────────────────
        ('5-05-01-040','Depreciation - Buildings',                'expense',  true, false,false,'MOOE',false),
        ('5-05-01-050','Depreciation - Office Equipment',         'expense',  true, false,false,'MOOE',false)
      ) AS t(code, name, kind, postable, contra, subsidiary, class, cash)
    ON CONFLICT (tenant_id, code) DO NOTHING;

    GET DIAGNOSTICS v_inserted = ROW_COUNT;

    -- Link each account to its group heading, so the chart displays as a tree.
    UPDATE account child
       SET parent_id = parent.id
      FROM account parent
     WHERE child.tenant_id  = p_tenant_id
       AND parent.tenant_id = p_tenant_id
       AND parent.code      = left(child.code, 1) || '-00-00-000'
       AND child.code      <> parent.code
       AND child.parent_id IS NULL;

    RETURN v_inserted;
END
$$;

COMMENT ON FUNCTION seed_chart_of_accounts(uuid) IS
    'Starter subset of the Revised Chart of Accounts. Safe to run repeatedly. '
    'Must be replaced by the full Annex A list before live use.';
