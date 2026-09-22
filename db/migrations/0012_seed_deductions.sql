-- ═══════════════════════════════════════════════════════════════════════════
--  0012 — Mandatory deduction types and their starting rates
-- ═══════════════════════════════════════════════════════════════════════════
--  ⚠️  THE RATES BELOW MUST BE VERIFIED BEFORE LIVE USE.
--
--  They are the commonly applied figures and are seeded so the system has a
--  working starting point, each carrying the issuance it came from. They are
--  NOT a substitute for checking the current BIR, GSIS, PhilHealth, and HDMF
--  circulars — PhilHealth premiums in particular have changed almost yearly,
--  and BIR withholding rates move with each revenue regulation.
--
--  Because rates are effectivity-dated rows, correcting one is an INSERT with
--  a new `effective_from` and an `effective_to` closing the old row. Vouchers
--  already issued keep the figures they were computed with — that is the whole
--  reason this is a table and not a constant.
-- ═══════════════════════════════════════════════════════════════════════════

CREATE OR REPLACE FUNCTION seed_deduction_types(p_tenant_id uuid)
RETURNS integer
LANGUAGE plpgsql
AS $$
DECLARE
    r          record;
    v_type_id  uuid;
    v_account  uuid;
    v_inserted integer := 0;
BEGIN
    FOR r IN
        SELECT * FROM (VALUES
            -- code,        name,                                       remit to,     GL account,     basis,              applies to, rate,      fixed,  authority
            ('EWT-GOODS',  'Expanded withholding tax on goods',         'BIR',        '2-02-01-010', 'percent_of_base',  'supplier', 0.010000,  NULL::numeric, 'BIR RR 11-2018, 1% on gross payments for goods'),
            ('EWT-SVC',    'Expanded withholding tax on services',      'BIR',        '2-02-01-010', 'percent_of_base',  'supplier', 0.020000,  NULL,   'BIR RR 11-2018, 2% on gross payments for services'),
            ('EWT-PROF',   'Withholding tax on professional fees',      'BIR',        '2-02-01-010', 'percent_of_base',  'supplier', 0.100000,  NULL,   'BIR RR 11-2018, 10% on professional fees'),
            ('VAT-FINAL',  'Final withholding VAT on government money', 'BIR',        '2-02-01-010', 'percent_of_base',  'supplier', 0.050000,  NULL,   'NIRC sec. 114(C), 5% final VAT withheld by government'),
            ('GSIS-PS',    'GSIS personal share',                       'GSIS',       '2-02-01-020', 'percent_of_gross', 'employee', 0.090000,  NULL,   'RA 8291, 9% personal share of monthly compensation'),
            ('PHIC-PS',    'PhilHealth personal share',                 'PhilHealth', '2-02-01-040', 'percent_of_gross', 'employee', 0.025000,  NULL,   'RA 11223 premium schedule, employee half of the total premium'),
            ('HDMF-PS',    'Pag-IBIG personal share',                   'Pag-IBIG',   '2-02-01-030', 'percent_of_gross', 'employee', 0.020000,  NULL,   'HDMF Circular, 2% employee share'),
            ('RETENTION',  'Retention on contracts',                    'Municipality','2-05-01-010','percent_of_base',  'supplier', 0.100000,  NULL,   'RA 9184 IRR, 10% retention on progress billings')
        ) AS t(code, name, remit_to, account_code, basis, applies_to, rate, fixed, authority)
    LOOP
        SELECT id INTO v_account
          FROM account
         WHERE tenant_id = p_tenant_id AND code = r.account_code;

        -- The liability account has to exist first. If the chart has been
        -- replaced with the full Annex A list under different codes, skip
        -- rather than fail the migration — a missing deduction is visible and
        -- fixable, a failed startup is not.
        IF v_account IS NULL THEN
            RAISE WARNING 'deduction % skipped: account % not in the chart', r.code, r.account_code;
            CONTINUE;
        END IF;

        INSERT INTO deduction_type (tenant_id, code, name, remit_to,
                                    liability_account_id, basis, applies_to)
             VALUES (p_tenant_id, r.code, r.name, r.remit_to,
                     v_account, r.basis, r.applies_to)
        ON CONFLICT (tenant_id, code) DO UPDATE SET name = excluded.name
          RETURNING id INTO v_type_id;

        -- Seed the opening rate only if this deduction has none at all, so
        -- re-running never disturbs a schedule an accountant has corrected.
        IF NOT EXISTS (SELECT 1 FROM deduction_rate WHERE deduction_type_id = v_type_id) THEN
            INSERT INTO deduction_rate (tenant_id, deduction_type_id, rate, fixed_amount,
                                        effective_from, authority)
                 VALUES (p_tenant_id, v_type_id, r.rate, r.fixed, DATE '2024-01-01', r.authority);
            v_inserted := v_inserted + 1;
        END IF;
    END LOOP;

    RETURN v_inserted;
END
$$;

COMMENT ON FUNCTION seed_deduction_types(uuid) IS
    'Starting deduction types and rates. Rates MUST be verified against '
    'current BIR/GSIS/PhilHealth/HDMF issuances before live use.';
