-- ═══════════════════════════════════════════════════════════════════════════
--  0007 — Default roles
-- ═══════════════════════════════════════════════════════════════════════════
--  Roles are per tenant, so they cannot be inserted by a migration that runs
--  before any municipality exists. This defines the seeding as a function the
--  server calls once it knows which municipality it serves.
--
--  The split follows how the Accounting Office actually works, and it is what
--  makes maker-checker real: the officer who certifies that an allotment is
--  available (Box B) is the Budget Officer, and the one who certifies the
--  obligation valid (Box C) is the Accountant. Two people, two roles, by
--  design and by COA practice.
-- ═══════════════════════════════════════════════════════════════════════════

CREATE OR REPLACE FUNCTION seed_default_roles(p_tenant_id uuid)
RETURNS void
LANGUAGE plpgsql
AS $$
DECLARE
    r           record;
    v_role_id   uuid;
BEGIN
    FOR r IN
        SELECT * FROM (VALUES
            ('ADMIN', 'Administrator',
             'Manages users, settings, and the audit trail. Does not record accounting entries.',
             ARRAY['user.manage','tenant.manage','audit.view','report.view',
                   'account.view','account.manage','budget.view']),

            ('ACCOUNTANT', 'Municipal Accountant',
             'Certifies obligations, approves vouchers, posts to the general ledger, and closes periods.',
             ARRAY['account.view','account.manage','budget.view',
                   'obligation.certify_accounting','dv.approve','jev.draft','jev.post',
                   'period.close','reconcile.perform','report.view','audit.view']),

            ('STAFF', 'Accounting Staff',
             'Prepares obligations, vouchers, and journal entries for certification and approval.',
             ARRAY['account.view','budget.view','obligation.draft','dv.draft',
                   'jev.draft','report.view']),

            ('BUDGET', 'Budget Officer',
             'Records appropriations and allotments, and certifies that an allotment is available.',
             ARRAY['account.view','budget.view','budget.manage',
                   'obligation.certify_budget','report.view']),

            ('TREASURY', 'Treasurer',
             'Issues official receipts, records collections, and records payments.',
             ARRAY['account.view','collection.record','disbursement.record','report.view']),

            ('AUDITOR', 'Auditor',
             'Reads everything, including the audit trail. Changes nothing.',
             ARRAY['account.view','budget.view','report.view','audit.view'])
        ) AS t(code, name, description, perms)
    LOOP
        INSERT INTO role (tenant_id, code, name, description, is_system)
             VALUES (p_tenant_id, r.code, r.name, r.description, true)
        ON CONFLICT (tenant_id, code) DO UPDATE
                SET name = excluded.name, description = excluded.description
          RETURNING id INTO v_role_id;

        -- Re-grant from the definition above so an upgrade that adds a
        -- permission reaches existing installations, while a permission an
        -- administrator deliberately removed is not silently restored.
        INSERT INTO role_permission (tenant_id, role_id, permission_code)
        SELECT p_tenant_id, v_role_id, unnest(r.perms)
        ON CONFLICT (role_id, permission_code) DO NOTHING;
    END LOOP;
END
$$;

COMMENT ON FUNCTION seed_default_roles(uuid) IS
    'Called by giams-node at startup once the municipality is known. Safe to '
    'run repeatedly.';
