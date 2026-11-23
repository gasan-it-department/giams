-- ═══════════════════════════════════════════════════════════════════════════
--  0037 — A new installation starts on the official chart
-- ═══════════════════════════════════════════════════════════════════════════
--  0035 put the official titles and contra flags on the municipalities that
--  existed when it ran. A new installation creates its municipality after
--  the migrations, and startup then seeded the trial balance's 258 accounts
--  (0018) first: their old titles, and no contra flags, so 34 accounts such
--  as Accumulated Depreciation sat on the wrong side. Found rehearsing a
--  fresh office installation on 2026-11-23.
--
--  The startup seed now lays down the official chart first, then the six
--  semi-expendable accounts the annex doesn't have, under COA's titles.
--  An installation that already has accounts is untouched: everything here
--  only inserts what's missing.
-- ═══════════════════════════════════════════════════════════════════════════

CREATE OR REPLACE FUNCTION seed_gasan_chart_of_accounts(p_tenant_id uuid)
RETURNS integer
LANGUAGE plpgsql
AS $fn$
DECLARE
    v_inserted integer;
    v_semi     integer;
BEGIN
    v_inserted := seed_official_chart_of_accounts(p_tenant_id);

    INSERT INTO account (tenant_id, code, name, account_type, is_postable,
                         is_contra, requires_subsidiary, expense_class, is_cash)
    SELECT p_tenant_id, v.code, v.name, 'asset', true, false, false, NULL, false
      FROM (VALUES
        ('1-04-05-020','Semi-Expendable Office Equipment'),
        ('1-04-05-030','Semi-Expendable Information and Communications Technology Equipment'),
        ('1-04-05-080','Semi-Expendable Disaster Response and Rescue Equipment'),
        ('1-04-05-090','Semi-Expendable Military, Police and Security Equipment'),
        ('1-04-05-990','Semi-Expendable Other Machinery and Equipment'),
        ('1-04-06-010','Semi-Expendable Furniture and Fixtures')
      ) AS v(code, name)
    ON CONFLICT (tenant_id, code) DO NOTHING;

    GET DIAGNOSTICS v_semi = ROW_COUNT;

    UPDATE account child
       SET parent_id = parent.id
      FROM account parent
     WHERE child.tenant_id  = p_tenant_id
       AND parent.tenant_id = p_tenant_id
       AND parent.code      = left(child.code, 1) || '-00-00-000'
       AND child.code      <> parent.code
       AND child.parent_id IS NULL;

    RETURN v_inserted + v_semi;
END
$fn$;

COMMENT ON FUNCTION seed_gasan_chart_of_accounts(uuid) IS
    'Starting chart for a new installation: the official Annex B chart, plus '
    'the six semi-expendable accounts. Inserts only what is missing.';
