-- ════════════════════════════════════════════════════════════════════════════
-- 0020 — The offices of the municipal government
--
-- An appropriation is made to an office and an obligation is charged against
-- one, so with no offices on file the whole budget chain cannot be started.
-- Nothing seeded them, which meant a fresh installation looked complete and
-- then refused the first thing anyone tried to do with it.
--
-- The list is the mandatory and optional offices of a municipality under the
-- Local Government Code (RA 7160, Book III Title V), plus the offices Gasan
-- keeps in practice. Codes are the abbreviations the office already uses on
-- its own vouchers, because those are what a clerk will type.
--
-- A starting point, not a fixed list: offices are created and merged by
-- ordinance, so this seeds only an empty tenant and never touches an existing
-- roster.
-- ════════════════════════════════════════════════════════════════════════════

CREATE OR REPLACE FUNCTION seed_offices(p_tenant_id uuid)
RETURNS integer
LANGUAGE plpgsql
AS $$
DECLARE
    v_inserted integer;
BEGIN
    INSERT INTO office (tenant_id, code, name)
    VALUES
        (p_tenant_id, 'SB',     'Sangguniang Bayan'),
        (p_tenant_id, 'OMM',    'Office of the Municipal Mayor'),
        (p_tenant_id, 'OMVM',   'Office of the Municipal Vice Mayor'),
        (p_tenant_id, 'MPDO',   'Municipal Planning and Development Office'),
        (p_tenant_id, 'MBO',    'Municipal Budget Office'),
        (p_tenant_id, 'MACCTG', 'Office of the Municipal Accountant'),
        (p_tenant_id, 'MTO',    'Office of the Municipal Treasurer'),
        (p_tenant_id, 'MASSO',  'Office of the Municipal Assessor'),
        (p_tenant_id, 'MCR',    'Office of the Municipal Civil Registrar'),
        (p_tenant_id, 'MEO',    'Municipal Engineering Office'),
        (p_tenant_id, 'MHO',    'Municipal Health Office'),
        (p_tenant_id, 'MSWDO',  'Municipal Social Welfare and Development Office'),
        (p_tenant_id, 'MAGRO',  'Municipal Agriculture Office'),
        (p_tenant_id, 'MENRO',  'Municipal Environment and Natural Resources Office'),
        (p_tenant_id, 'HRMO',   'Human Resource Management Office'),
        (p_tenant_id, 'GSO',    'General Services Office'),
        (p_tenant_id, 'MDRRMO', 'Municipal Disaster Risk Reduction and Management Office'),
        (p_tenant_id, 'MTCO',   'Municipal Tourism and Cultural Affairs Office'),
        (p_tenant_id, 'MCTC',   'Municipal Trial Court'),
        (p_tenant_id, 'PNP',    'Philippine National Police — Gasan Station'),
        (p_tenant_id, 'BFP',    'Bureau of Fire Protection — Gasan Station')
    ON CONFLICT (tenant_id, code) DO NOTHING;

    GET DIAGNOSTICS v_inserted = ROW_COUNT;
    RETURN v_inserted;
END
$$;

COMMENT ON FUNCTION seed_offices(uuid) IS
    'The offices of a municipal government, as a starting roster. Offices are '
    'created and merged by ordinance, so this is a starting point rather than '
    'a complete list. Safe to run repeatedly.';
