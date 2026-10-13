-- ═══════════════════════════════════════════════════════════════════════════
--  0033 — Licence profiles, for the admin console
-- ═══════════════════════════════════════════════════════════════════════════
--  Issuing a licence used to be a command typed at the licensing authority's
--  console, and nothing remembered who a key had been given to. This records
--  it: a profile per municipality — who its Municipal Accountant and Accounting
--  Staff are — and every key issued against it.
--
--  The people on a profile are not accounts on *this* server. They are signed
--  into the licence key itself, and become accounts on the municipality's own
--  server when the key is installed there. The key is the only thing that
--  travels from the authority to a municipality, so it has to carry them.
--
--  Issuing still needs the authority's private key on disk. A municipality's
--  server never has it, so an administrator there can see this section but
--  cannot mint a key.
-- ═══════════════════════════════════════════════════════════════════════════

INSERT INTO permission (code, module, label, description) VALUES
    ('licence.issue', 'admin', 'Issue licence keys',
     'Create licence profiles for municipalities and issue their keys.')
ON CONFLICT (code) DO NOTHING;


-- Same definition as 0007, with licence.issue added to ADMIN. The server calls
-- this at every start, so an existing installation picks the permission up
-- without anyone having to grant it by hand.
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
                   'account.view','account.manage','budget.view','licence.issue']),

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


-- ── Profiles ─────────────────────────────────────────────────────────────────
CREATE TABLE licensee (
    id                  uuid        PRIMARY KEY DEFAULT uuid_generate_v7(),
    tenant_id           uuid        NOT NULL REFERENCES tenant(id) ON DELETE RESTRICT,

    municipality_name   text        NOT NULL CHECK (btrim(municipality_name) <> ''),

    -- The municipality the key is bound to. For a profile made for this very
    -- server it is this server's own municipality; otherwise it is minted here
    -- and handed over as part of the server's settings.
    target_tenant_id    uuid        NOT NULL,
    target_tenant_code  text        NOT NULL CHECK (target_tenant_code ~ '^[A-Z0-9-]{2,40}$'),
    for_this_server     boolean     NOT NULL DEFAULT false,

    notes               text,

    created_by          uuid        REFERENCES app_user(id) ON DELETE RESTRICT,
    created_at          timestamptz NOT NULL DEFAULT now(),
    updated_at          timestamptz NOT NULL DEFAULT now(),

    CONSTRAINT licensee_one_per_code   UNIQUE (tenant_id, target_tenant_code),
    CONSTRAINT licensee_one_per_tenant UNIQUE (tenant_id, target_tenant_id),
    CONSTRAINT licensee_tenant_key     UNIQUE (tenant_id, id)
);


-- ── The people a municipality starts with ────────────────────────────────────
CREATE TABLE licensee_person (
    id              uuid        PRIMARY KEY DEFAULT uuid_generate_v7(),
    tenant_id       uuid        NOT NULL REFERENCES tenant(id) ON DELETE RESTRICT,
    licensee_id     uuid        NOT NULL,

    full_name       text        NOT NULL CHECK (btrim(full_name) <> ''),
    username        citext      NOT NULL CHECK (username ~ '^[a-z0-9._-]{3,40}$'),
    role_code       text        NOT NULL CHECK (role_code IN ('ACCOUNTANT','STAFF')),
    position_title  text,
    sort_order      smallint    NOT NULL DEFAULT 0,

    CONSTRAINT licensee_person_username_once UNIQUE (licensee_id, username),
    FOREIGN KEY (tenant_id, licensee_id) REFERENCES licensee (tenant_id, id) ON DELETE RESTRICT
);

CREATE INDEX licensee_person_by_licensee ON licensee_person (licensee_id, sort_order);


-- ── Keys issued ──────────────────────────────────────────────────────────────
-- Append-only. A key that has been handed to a municipality exists whether or
-- not this table remembers it, so the record of it must not be editable.
CREATE TABLE licence_issue (
    id           uuid        PRIMARY KEY DEFAULT uuid_generate_v7(),
    tenant_id    uuid        NOT NULL REFERENCES tenant(id) ON DELETE RESTRICT,
    licensee_id  uuid        NOT NULL,

    token        text        NOT NULL,
    plan         text        NOT NULL,
    seats        integer     NOT NULL CHECK (seats > 0),
    not_before   timestamptz NOT NULL,
    expires_at   timestamptz NOT NULL,
    grace_days   integer     NOT NULL CHECK (grace_days >= 0),

    issued_by    uuid        REFERENCES app_user(id) ON DELETE RESTRICT,
    issued_at    timestamptz NOT NULL DEFAULT now(),

    CONSTRAINT licence_issue_runs_forward CHECK (expires_at > not_before),
    FOREIGN KEY (tenant_id, licensee_id) REFERENCES licensee (tenant_id, id) ON DELETE RESTRICT
);

CREATE INDEX licence_issue_by_licensee ON licence_issue (licensee_id, issued_at DESC);

CREATE OR REPLACE FUNCTION forbid_licence_issue_change()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'An issued licence key is a permanent record and cannot be changed or removed.'
        USING ERRCODE = 'restrict_violation',
              HINT    = 'Issue a new key instead.';
END
$$;

CREATE TRIGGER licence_issue_append_only
    BEFORE UPDATE OR DELETE ON licence_issue
    FOR EACH ROW EXECUTE FUNCTION forbid_licence_issue_change();


CREATE TRIGGER licensee_touch BEFORE UPDATE ON licensee FOR EACH ROW EXECUTE FUNCTION touch_updated_at();

SELECT apply_tenant_guards('licensee');
SELECT apply_tenant_guards('licensee_person');
SELECT apply_tenant_guards('licence_issue');
