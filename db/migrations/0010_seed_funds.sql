-- ═══════════════════════════════════════════════════════════════════════════
--  0010 — Standard LGU funds
-- ═══════════════════════════════════════════════════════════════════════════
--  LGU accounting is fund-based: every transaction, budget line, and financial
--  statement belongs to a fund, and the books are kept per fund before being
--  consolidated. Unlike the chart of accounts, this list is short, fixed by
--  law, and identical for every municipality — so it is a complete seed rather
--  than a starter subset.
--
--  RA 7160 establishes the three funds. The sub-funds are statutory carve-outs
--  of the General Fund that must be reported separately:
--
--    20% Development Fund  §287  — 20% of the NTA for development projects
--    LDRRMF                RA 10121 — 5% for disaster risk reduction
--    GAD                   RA 9710  — at least 5% of the total budget
--
--  They roll up into the General Fund's statements while carrying their own
--  appropriations and registries.
-- ═══════════════════════════════════════════════════════════════════════════

CREATE OR REPLACE FUNCTION seed_funds(p_tenant_id uuid)
RETURNS integer
LANGUAGE plpgsql
AS $$
DECLARE
    v_general_id uuid;
    v_inserted   integer;
BEGIN
    INSERT INTO fund (tenant_id, code, name, kind)
         VALUES (p_tenant_id, '01', 'General Fund',           'general'),
                (p_tenant_id, '02', 'Special Education Fund', 'special_education'),
                (p_tenant_id, '03', 'Trust Fund',             'trust')
    ON CONFLICT (tenant_id, code) DO NOTHING;

    GET DIAGNOSTICS v_inserted = ROW_COUNT;

    SELECT id INTO v_general_id
      FROM fund
     WHERE tenant_id = p_tenant_id AND code = '01';

    INSERT INTO fund (tenant_id, code, name, kind, parent_fund_id, is_statutory_subfund)
         VALUES (p_tenant_id, '01-DF',    '20% Development Fund',                         'general', v_general_id, true),
                (p_tenant_id, '01-LDRRMF','Local Disaster Risk Reduction and Management Fund','general', v_general_id, true),
                (p_tenant_id, '01-GAD',   'Gender and Development Fund',                  'general', v_general_id, true)
    ON CONFLICT (tenant_id, code) DO NOTHING;

    RETURN v_inserted;
END
$$;

COMMENT ON FUNCTION seed_funds(uuid) IS
    'The three statutory LGU funds and the General Fund sub-funds. Complete, '
    'not a subset. Safe to run repeatedly.';
