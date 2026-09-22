-- ═══════════════════════════════════════════════════════════════════════════
--  0002 — Tamper-evident audit trail
-- ═══════════════════════════════════════════════════════════════════════════
--  §4.1 of the project design requires an "immutable audit trail"; §4.2
--  requires it to be "tamper-evident". Those are two different guarantees:
--
--    immutable      -> nothing here can be updated or deleted (enforced below)
--    tamper-evident -> if someone edits the table out-of-band anyway, we can
--                      prove it and name the exact row
--
--  The second is what a hash chain buys. Each row commits to its predecessor,
--  so altering any historical row invalidates every hash after it.
--  verify_audit_chain() walks the chain and returns the first break.
-- ═══════════════════════════════════════════════════════════════════════════


-- ── Chain tip ────────────────────────────────────────────────────────────────
-- One row per tenant holding the running sequence and the latest hash. The
-- UPDATE in the append trigger takes a row lock on this, which serialises
-- concurrent audit writes -- that serialisation is what keeps the chain linear.
CREATE TABLE audit_chain (
    tenant_id   uuid        PRIMARY KEY REFERENCES tenant(id) ON DELETE RESTRICT,
    last_seq    bigint      NOT NULL DEFAULT 0,
    last_hash   bytea,
    updated_at  timestamptz NOT NULL DEFAULT now()
);


CREATE TABLE audit_log (
    id          bigint      GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    tenant_id   uuid        NOT NULL REFERENCES tenant(id) ON DELETE RESTRICT,
    node_id     uuid        REFERENCES node(id) ON DELETE SET NULL,

    -- Position in this tenant's chain. Gaps are impossible by construction;
    -- if one appears, the chain has been tampered with.
    seq         bigint      NOT NULL,
    at          timestamptz NOT NULL DEFAULT now(),

    -- The acting user, plus their name captured at the time. An audit trail
    -- must stay readable after a user record is renamed or deactivated.
    actor_id    uuid,
    actor_label text        NOT NULL DEFAULT 'system',

    action      text        NOT NULL,
    entity      text        NOT NULL,
    entity_id   uuid,

    before      jsonb,
    after       jsonb,

    -- Short code surfaced to the user in error messages so support can find
    -- this exact row without the user reading anything technical.
    reference   text,
    ip_address  inet,

    prev_hash   bytea,
    row_hash    bytea       NOT NULL,

    UNIQUE (tenant_id, seq)
);

CREATE INDEX audit_log_tenant_at      ON audit_log (tenant_id, at DESC);
CREATE INDEX audit_log_entity         ON audit_log (tenant_id, entity, entity_id);
CREATE INDEX audit_log_actor          ON audit_log (tenant_id, actor_id, at DESC);
CREATE INDEX audit_log_reference      ON audit_log (reference) WHERE reference IS NOT NULL;


-- ── Canonical payload ────────────────────────────────────────────────────────
-- What gets hashed. Two properties matter:
--
--   * jsonb renders its keys in a deterministic order in PostgreSQL, so the
--     text form is stable for a given value.
--   * the timestamp is rendered explicitly in UTC. Casting a timestamptz
--     through jsonb would format it using the *session* TimeZone, which would
--     make the same row hash differently for a clerk in Manila and a verifier
--     running in UTC. That would look exactly like tampering.
CREATE OR REPLACE FUNCTION audit_canonical_payload(
    p_seq       bigint,
    p_at        timestamptz,
    p_actor_id  uuid,
    p_action    text,
    p_entity    text,
    p_entity_id uuid,
    p_before    jsonb,
    p_after     jsonb
)
RETURNS text
LANGUAGE sql
IMMUTABLE
AS $$
    SELECT jsonb_build_object(
        'seq',       p_seq,
        'at',        to_char(p_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.US"Z"'),
        'actor_id',  p_actor_id,
        'action',    p_action,
        'entity',    p_entity,
        'entity_id', p_entity_id,
        'before',    coalesce(p_before, 'null'::jsonb),
        'after',     coalesce(p_after,  'null'::jsonb)
    )::text
$$;


CREATE OR REPLACE FUNCTION audit_link_row()
RETURNS trigger
LANGUAGE plpgsql
AS $$
DECLARE
    v_prev_hash bytea;
    v_seq       bigint;
BEGIN
    INSERT INTO audit_chain (tenant_id, last_seq, last_hash)
         VALUES (NEW.tenant_id, 0, NULL)
    ON CONFLICT (tenant_id) DO NOTHING;

    -- Row-locks this tenant's tip; concurrent audit writes queue here.
    UPDATE audit_chain
       SET last_seq   = last_seq + 1,
           updated_at = now()
     WHERE tenant_id  = NEW.tenant_id
    RETURNING last_seq, last_hash INTO v_seq, v_prev_hash;

    NEW.seq       := v_seq;
    NEW.prev_hash := v_prev_hash;
    NEW.row_hash  := digest(
        coalesce(v_prev_hash, '\x'::bytea) ||
        convert_to(
            audit_canonical_payload(
                v_seq, NEW.at, NEW.actor_id, NEW.action,
                NEW.entity, NEW.entity_id, NEW.before, NEW.after
            ),
            'UTF8'
        ),
        'sha256'
    );

    UPDATE audit_chain
       SET last_hash = NEW.row_hash
     WHERE tenant_id = NEW.tenant_id;

    RETURN NEW;
END
$$;

CREATE TRIGGER audit_log_link
    BEFORE INSERT ON audit_log
    FOR EACH ROW EXECUTE FUNCTION audit_link_row();


-- ── Immutability ─────────────────────────────────────────────────────────────
-- Not a convention, not an ORM setting. The database refuses.
CREATE OR REPLACE FUNCTION audit_log_is_append_only()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'the audit trail is append-only; % is not permitted', TG_OP
        USING ERRCODE = 'restrict_violation',
              HINT    = 'Audit records are permanent by law and by design.';
END
$$;

CREATE TRIGGER audit_log_no_update
    BEFORE UPDATE ON audit_log
    FOR EACH ROW EXECUTE FUNCTION audit_log_is_append_only();

CREATE TRIGGER audit_log_no_delete
    BEFORE DELETE ON audit_log
    FOR EACH ROW EXECUTE FUNCTION audit_log_is_append_only();

CREATE TRIGGER audit_log_no_truncate
    BEFORE TRUNCATE ON audit_log
    FOR EACH STATEMENT EXECUTE FUNCTION audit_log_is_append_only();


-- ── Verification ─────────────────────────────────────────────────────────────
-- Walks a tenant's chain in order and recomputes every hash. Returns one row
-- per problem found; an empty result means the trail is intact.
--
-- This is the COA demonstration: edit any audit row directly in psql, run this,
-- and it names the row and says exactly what is wrong.
CREATE OR REPLACE FUNCTION verify_audit_chain(p_tenant_id uuid)
RETURNS TABLE (
    audit_id    bigint,
    seq         bigint,
    at          timestamptz,
    problem     text
)
LANGUAGE plpgsql
STABLE
AS $$
DECLARE
    r             record;
    v_expect_seq  bigint := 1;
    v_prev_hash   bytea  := NULL;
    v_recomputed  bytea;
BEGIN
    FOR r IN
        SELECT * FROM audit_log
         WHERE tenant_id = p_tenant_id
         ORDER BY seq
    LOOP
        IF r.seq <> v_expect_seq THEN
            audit_id := r.id; seq := r.seq; at := r.at;
            problem  := format('sequence break: expected %s, found %s -- %s record(s) removed',
                               v_expect_seq, r.seq, r.seq - v_expect_seq);
            RETURN NEXT;
            v_expect_seq := r.seq;
        END IF;

        IF r.prev_hash IS DISTINCT FROM v_prev_hash THEN
            audit_id := r.id; seq := r.seq; at := r.at;
            problem  := 'broken link: this record does not point at its predecessor';
            RETURN NEXT;
        END IF;

        v_recomputed := digest(
            coalesce(r.prev_hash, '\x'::bytea) ||
            convert_to(
                audit_canonical_payload(
                    r.seq, r.at, r.actor_id, r.action,
                    r.entity, r.entity_id, r.before, r.after
                ),
                'UTF8'
            ),
            'sha256'
        );

        IF v_recomputed IS DISTINCT FROM r.row_hash THEN
            audit_id := r.id; seq := r.seq; at := r.at;
            problem  := 'contents altered: the stored hash does not match this record';
            RETURN NEXT;
        END IF;

        v_prev_hash  := r.row_hash;
        v_expect_seq := r.seq + 1;
    END LOOP;

    RETURN;
END
$$;

COMMENT ON FUNCTION verify_audit_chain(uuid) IS
    'Returns one row per detected tampering. Empty result = trail intact. '
    'Run before any COA submission and on a schedule.';

SELECT apply_tenant_guards('audit_log');
