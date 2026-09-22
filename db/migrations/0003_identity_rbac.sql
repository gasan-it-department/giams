-- ═══════════════════════════════════════════════════════════════════════════
--  0003 — Identity, roles, permissions, sessions
-- ═══════════════════════════════════════════════════════════════════════════
--  §4.1: "log in with certified credentials under role-based access control"
--  §4.2: "enforce role-based access for all Accounting office personnel"
--
--  Two rules that the schema enforces rather than trusting the API to:
--    * a user may never approve their own document (maker-checker)
--    * a refresh token may be used exactly once; reuse revokes the family
-- ═══════════════════════════════════════════════════════════════════════════


-- ── Permissions ──────────────────────────────────────────────────────────────
-- Global catalogue, not tenant-scoped: the vocabulary of what can be done is
-- the same for every LGU. Which role holds which permission is per tenant.
CREATE TABLE permission (
    code        text        PRIMARY KEY,
    module      text        NOT NULL,
    label       text        NOT NULL,   -- shown in the role editor, plain language
    description text        NOT NULL
);

INSERT INTO permission (code, module, label, description) VALUES
    ('account.view',        'ledger',   'View chart of accounts',      'See the chart of accounts and account balances.'),
    ('account.manage',      'ledger',   'Maintain chart of accounts',  'Add, edit, and deactivate accounts.'),
    ('budget.view',         'budget',   'View budget',                 'See appropriations, allotments, and registries.'),
    ('budget.manage',       'budget',   'Record appropriations',       'Record appropriations and release allotments.'),
    ('obligation.draft',    'budget',   'Prepare obligations',         'Prepare obligation requests for certification.'),
    ('obligation.certify_budget', 'budget', 'Certify allotment available', 'Sign Box B of the obligation request as Budget Officer.'),
    ('obligation.certify_accounting', 'budget', 'Certify obligation valid', 'Sign Box C of the obligation request as Accountant.'),
    ('dv.draft',            'disburse', 'Prepare vouchers',            'Prepare disbursement vouchers.'),
    ('dv.approve',          'disburse', 'Approve vouchers',            'Approve disbursement vouchers for payment.'),
    ('disbursement.record', 'disburse', 'Record payments',             'Record checks, ADA, and cash payments.'),
    ('collection.record',   'collect',  'Record collections',          'Issue official receipts and record collections.'),
    ('jev.draft',           'ledger',   'Prepare journal entries',     'Prepare journal entry vouchers.'),
    ('jev.post',            'ledger',   'Post to the general ledger',  'Post approved journal entries to the general ledger.'),
    ('period.close',        'ledger',   'Close accounting periods',    'Close a month or year and lock it against further posting.'),
    ('reconcile.perform',   'reconcile','Reconcile bank accounts',     'Match records and complete bank reconciliations.'),
    ('report.view',         'report',   'View reports',                'View and print registries and financial statements.'),
    ('audit.view',          'admin',    'View the audit trail',        'Read the full audit trail and verify its integrity.'),
    ('user.manage',         'admin',    'Manage users',                'Create users, assign roles, and revoke sessions.'),
    ('tenant.manage',       'admin',    'Manage system settings',      'Change municipality settings and system configuration.');


-- ── Roles ────────────────────────────────────────────────────────────────────
CREATE TABLE role (
    id          uuid        PRIMARY KEY DEFAULT uuid_generate_v7(),
    tenant_id   uuid        NOT NULL REFERENCES tenant(id) ON DELETE RESTRICT,
    code        text        NOT NULL,
    name        text        NOT NULL,
    description text        NOT NULL DEFAULT '',
    -- Built-in roles cannot be deleted; their permissions can still be tuned.
    is_system   boolean     NOT NULL DEFAULT false,
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now(),

    UNIQUE (tenant_id, code)
);

CREATE TABLE role_permission (
    tenant_id       uuid    NOT NULL REFERENCES tenant(id) ON DELETE RESTRICT,
    role_id         uuid    NOT NULL REFERENCES role(id) ON DELETE CASCADE,
    permission_code text    NOT NULL REFERENCES permission(code) ON DELETE RESTRICT,

    PRIMARY KEY (role_id, permission_code)
);


-- ── Users ────────────────────────────────────────────────────────────────────
CREATE TABLE app_user (
    id                  uuid        PRIMARY KEY DEFAULT uuid_generate_v7(),
    tenant_id           uuid        NOT NULL REFERENCES tenant(id) ON DELETE RESTRICT,

    username            citext      NOT NULL,
    email               citext,
    full_name           text        NOT NULL,
    -- Printed on vouchers and certifications, e.g. 'Municipal Accountant'.
    position_title      text,

    -- Argon2id PHC string. The parameters live inside it, so tuning them later
    -- does not invalidate existing passwords.
    password_hash       text        NOT NULL,
    password_changed_at timestamptz NOT NULL DEFAULT now(),
    must_change_password boolean    NOT NULL DEFAULT true,

    -- Progressive lockout. Cleared on any successful authentication.
    failed_attempts     smallint    NOT NULL DEFAULT 0,
    locked_until        timestamptz,

    is_active           boolean     NOT NULL DEFAULT true,
    last_login_at       timestamptz,
    created_at          timestamptz NOT NULL DEFAULT now(),
    updated_at          timestamptz NOT NULL DEFAULT now(),

    UNIQUE (tenant_id, username)
);

CREATE UNIQUE INDEX app_user_email_unique
    ON app_user (tenant_id, email)
    WHERE email IS NOT NULL;

CREATE TABLE user_role (
    tenant_id   uuid        NOT NULL REFERENCES tenant(id) ON DELETE RESTRICT,
    user_id     uuid        NOT NULL REFERENCES app_user(id) ON DELETE CASCADE,
    role_id     uuid        NOT NULL REFERENCES role(id) ON DELETE RESTRICT,
    granted_at  timestamptz NOT NULL DEFAULT now(),
    granted_by  uuid        REFERENCES app_user(id) ON DELETE SET NULL,

    PRIMARY KEY (user_id, role_id)
);


-- ── Sessions and refresh-token rotation ──────────────────────────────────────
-- A session is a family of refresh tokens. Each refresh issues a new token and
-- retires the old one. If a retired token is presented again, that means it was
-- captured -- so the whole family is revoked immediately.
CREATE TABLE user_session (
    id              uuid        PRIMARY KEY DEFAULT uuid_generate_v7(),
    tenant_id       uuid        NOT NULL REFERENCES tenant(id) ON DELETE RESTRICT,
    user_id         uuid        NOT NULL REFERENCES app_user(id) ON DELETE CASCADE,

    -- SHA-256 of the refresh token. The token itself is never stored: a stolen
    -- database backup must not yield usable sessions.
    token_hash      bytea       NOT NULL UNIQUE,
    previous_id     uuid        REFERENCES user_session(id) ON DELETE SET NULL,

    workstation     text,
    ip_address      inet,
    user_agent      text,

    issued_at       timestamptz NOT NULL DEFAULT now(),
    expires_at      timestamptz NOT NULL,
    last_used_at    timestamptz,

    revoked_at      timestamptz,
    revoked_reason  text        CHECK (revoked_reason IN
                                ('logout','rotated','reuse_detected','expired',
                                 'revoked_by_admin','password_changed')),

    CHECK (revoked_at IS NULL OR revoked_reason IS NOT NULL)
);

CREATE INDEX user_session_active
    ON user_session (tenant_id, user_id)
    WHERE revoked_at IS NULL;

CREATE INDEX user_session_expiry
    ON user_session (expires_at)
    WHERE revoked_at IS NULL;

COMMENT ON COLUMN user_session.previous_id IS
    'Links a rotated token to the one it replaced, forming the family that '
    'gets revoked wholesale when a retired token is replayed.';


-- ── Maker-checker ────────────────────────────────────────────────────────────
-- Reused by every approvable document (obligations, DVs, JEVs). The rule that a
-- user may not approve their own work is enforced here so that no route, and no
-- future developer, can route around it.
CREATE TYPE document_status AS ENUM (
    'draft', 'submitted', 'certified', 'approved', 'posted', 'cancelled', 'reversed'
);

CREATE OR REPLACE FUNCTION assert_different_actor(
    p_preparer  uuid,
    p_actor     uuid,
    p_step      text
)
RETURNS void
LANGUAGE plpgsql
IMMUTABLE
AS $$
BEGIN
    IF p_preparer IS NOT NULL AND p_actor IS NOT NULL AND p_preparer = p_actor THEN
        RAISE EXCEPTION
            'the person who prepared this document cannot also % it', p_step
            USING ERRCODE = 'restrict_violation',
                  HINT    = 'Ask another authorised officer to take this step.';
    END IF;
END
$$;


CREATE TRIGGER role_touch      BEFORE UPDATE ON role      FOR EACH ROW EXECUTE FUNCTION touch_updated_at();
CREATE TRIGGER app_user_touch  BEFORE UPDATE ON app_user  FOR EACH ROW EXECUTE FUNCTION touch_updated_at();

SELECT apply_tenant_guards('role');
SELECT apply_tenant_guards('role_permission');
SELECT apply_tenant_guards('app_user');
SELECT apply_tenant_guards('user_role');
SELECT apply_tenant_guards('user_session');
