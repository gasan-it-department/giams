-- ═══════════════════════════════════════════════════════════════════════════
--  0001 — Foundation: extensions, id generation, tenancy, RLS plumbing
-- ═══════════════════════════════════════════════════════════════════════════
--  Read docs/ARCHITECTURE.md §2-§4 before changing anything in this file.
--
--  Two decisions are locked in here and are expensive to reverse:
--    1. Every synced row's id is a UUIDv7 assigned by its creator, never a
--       SERIAL. This is what makes re-sending a row idempotent instead of
--       duplicating it.
--    2. Every tenant-scoped table carries tenant_id and is protected by row
--       level security. A single-municipality deployment just has one tenant.
-- ═══════════════════════════════════════════════════════════════════════════

CREATE EXTENSION IF NOT EXISTS pgcrypto;   -- gen_random_bytes, digest
CREATE EXTENSION IF NOT EXISTS pg_trgm;    -- payee similarity in reconciliation
CREATE EXTENSION IF NOT EXISTS citext;     -- case-insensitive usernames/emails


-- ── Identity generation ──────────────────────────────────────────────────────
-- UUIDv7: a 48-bit millisecond timestamp followed by 74 random bits. Sorts in
-- creation order, so it indexes like a sequence without leaking a global
-- counter or colliding across offline nodes.
--
-- PostgreSQL 17 has no built-in uuidv7(); this is the reference layout from
-- RFC 9562 §5.7. When we move to PG 18 this can become a thin wrapper.
CREATE OR REPLACE FUNCTION uuid_generate_v7()
RETURNS uuid
LANGUAGE plpgsql
VOLATILE
AS $$
DECLARE
    unix_ts_ms bytea;
    buf        bytea;
BEGIN
    -- 6 bytes of big-endian unix milliseconds (drop the top 2 of int8send's 8)
    unix_ts_ms := substring(
        int8send(floor(extract(epoch FROM clock_timestamp()) * 1000)::bigint)
        FROM 3
    );
    buf := unix_ts_ms || gen_random_bytes(10);

    -- byte 6: high nibble = version 7, low nibble stays random
    buf := set_byte(buf, 6, (get_byte(buf, 6) & 15) | 112);
    -- byte 8: high two bits = variant 0b10, rest stays random
    buf := set_byte(buf, 8, (get_byte(buf, 8) & 63) | 128);

    RETURN encode(buf, 'hex')::uuid;
END
$$;

COMMENT ON FUNCTION uuid_generate_v7() IS
    'RFC 9562 UUIDv7. Time-ordered, collision-safe across offline nodes. '
    'Use for every id that can cross a sync boundary.';


-- ── Tenancy ──────────────────────────────────────────────────────────────────
-- One row per subscribing local government unit. Gasan is the first.
CREATE TABLE tenant (
    id              uuid        PRIMARY KEY DEFAULT uuid_generate_v7(),
    code            citext      NOT NULL UNIQUE,
    name            text        NOT NULL,
    lgu_type        text        NOT NULL DEFAULT 'municipality'
                                CHECK (lgu_type IN ('municipality','city','province','barangay')),
    province        text,
    region          text,
    -- Financial reporting context
    fiscal_year_start_month smallint NOT NULL DEFAULT 1
                                CHECK (fiscal_year_start_month BETWEEN 1 AND 12),
    currency_code   char(3)     NOT NULL DEFAULT 'PHP',
    timezone        text        NOT NULL DEFAULT 'Asia/Manila',
    -- Lifecycle. 'read_only' is what an expired licence degrades to; the books
    -- stay fully readable and printable. See ARCHITECTURE.md §5.
    status          text        NOT NULL DEFAULT 'active'
                                CHECK (status IN ('active','read_only','suspended')),
    created_at      timestamptz NOT NULL DEFAULT now(),
    updated_at      timestamptz NOT NULL DEFAULT now()
);

COMMENT ON TABLE tenant IS
    'One subscribing LGU. Present from migration 0001 even though subscription '
    'ships last -- retrofitting tenancy into a live ledger is not feasible.';


-- ── Nodes ────────────────────────────────────────────────────────────────────
-- A node is an office server that owns a tenant's ledger. Exactly one node per
-- tenant may create ledger facts (ARCHITECTURE.md §2). Workstations are not
-- nodes; they hold drafts and caches only.
CREATE TABLE node (
    id              uuid        PRIMARY KEY DEFAULT uuid_generate_v7(),
    tenant_id       uuid        NOT NULL REFERENCES tenant(id) ON DELETE RESTRICT,
    code            text        NOT NULL,
    name            text        NOT NULL,
    -- Ed25519 public key. The node signs every sync batch with the private
    -- half, so a stolen API credential still cannot forge ledger entries.
    public_key      bytea       NOT NULL,
    is_primary      boolean     NOT NULL DEFAULT true,
    last_seen_at    timestamptz,
    last_synced_at  timestamptz,
    created_at      timestamptz NOT NULL DEFAULT now(),
    updated_at      timestamptz NOT NULL DEFAULT now(),

    UNIQUE (tenant_id, code)
);

-- Enforce the single-writer rule in the schema, not in application code.
CREATE UNIQUE INDEX node_one_primary_per_tenant
    ON node (tenant_id)
    WHERE is_primary;

COMMENT ON INDEX node_one_primary_per_tenant IS
    'The single-writer guarantee. At most one primary node per tenant may '
    'create ledger facts, which is what makes duplication structurally '
    'impossible rather than something we clean up afterwards.';


-- ── Request context ──────────────────────────────────────────────────────────
-- The API opens each transaction with:
--     SET LOCAL app.tenant_id = '...';
--     SET LOCAL app.user_id   = '...';
-- Every RLS policy reads these. A query that forgets its tenant filter returns
-- nothing rather than another municipality's ledger.

CREATE OR REPLACE FUNCTION current_tenant_id()
RETURNS uuid
LANGUAGE sql
STABLE
AS $$
    SELECT nullif(current_setting('app.tenant_id', true), '')::uuid
$$;

CREATE OR REPLACE FUNCTION current_user_id()
RETURNS uuid
LANGUAGE sql
STABLE
AS $$
    SELECT nullif(current_setting('app.user_id', true), '')::uuid
$$;

-- Used by triggers that must refuse to run outside a properly scoped request.
CREATE OR REPLACE FUNCTION require_tenant_id()
RETURNS uuid
LANGUAGE plpgsql
STABLE
AS $$
DECLARE
    t uuid;
BEGIN
    t := current_tenant_id();
    IF t IS NULL THEN
        RAISE EXCEPTION 'no tenant in scope for this transaction'
            USING ERRCODE = 'insufficient_privilege',
                  HINT    = 'The API must SET LOCAL app.tenant_id before touching tenant data.';
    END IF;
    RETURN t;
END
$$;


-- ── Shared trigger helpers ───────────────────────────────────────────────────

CREATE OR REPLACE FUNCTION touch_updated_at()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    NEW.updated_at := now();
    RETURN NEW;
END
$$;

-- Defence in depth: tenant_id is never allowed to change, and never allowed to
-- be set to a tenant other than the one in scope.
CREATE OR REPLACE FUNCTION guard_tenant_id()
RETURNS trigger
LANGUAGE plpgsql
AS $$
DECLARE
    scoped uuid := current_tenant_id();
BEGIN
    IF TG_OP = 'UPDATE' AND NEW.tenant_id IS DISTINCT FROM OLD.tenant_id THEN
        RAISE EXCEPTION 'tenant_id is immutable'
            USING ERRCODE = 'restrict_violation';
    END IF;

    IF scoped IS NOT NULL AND NEW.tenant_id IS DISTINCT FROM scoped THEN
        RAISE EXCEPTION 'row tenant_id does not match the tenant in scope'
            USING ERRCODE = 'insufficient_privilege';
    END IF;

    RETURN NEW;
END
$$;

-- Applies the standard tenant protections to a table in one call, so no table
-- can be added later that quietly skips them.
CREATE OR REPLACE FUNCTION apply_tenant_guards(target regclass)
RETURNS void
LANGUAGE plpgsql
AS $$
DECLARE
    t text := target::text;
BEGIN
    EXECUTE format('ALTER TABLE %s ENABLE ROW LEVEL SECURITY', t);
    EXECUTE format('ALTER TABLE %s FORCE ROW LEVEL SECURITY', t);

    EXECUTE format($f$
        CREATE POLICY tenant_isolation ON %s
            USING       (tenant_id = current_tenant_id())
            WITH CHECK  (tenant_id = current_tenant_id())
    $f$, t);

    EXECUTE format($f$
        CREATE TRIGGER %s_guard_tenant
            BEFORE INSERT OR UPDATE ON %s
            FOR EACH ROW EXECUTE FUNCTION guard_tenant_id()
    $f$, replace(t, '.', '_'), t);
END
$$;

COMMENT ON FUNCTION apply_tenant_guards(regclass) IS
    'Call once per tenant-scoped table. Enables FORCE row level security, adds '
    'the isolation policy, and installs the immutability trigger. Every new '
    'tenant-scoped table must call this in its own migration.';


CREATE TRIGGER tenant_touch
    BEFORE UPDATE ON tenant
    FOR EACH ROW EXECUTE FUNCTION touch_updated_at();

CREATE TRIGGER node_touch
    BEFORE UPDATE ON node
    FOR EACH ROW EXECUTE FUNCTION touch_updated_at();

SELECT apply_tenant_guards('node');
