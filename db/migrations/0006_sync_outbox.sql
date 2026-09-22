-- ═══════════════════════════════════════════════════════════════════════════
--  0006 — Sync outbox
-- ═══════════════════════════════════════════════════════════════════════════
--  Backs the duplication-prevention guarantee in docs/ARCHITECTURE.md §3.
--
--  The load-bearing idea is the transactional outbox: the sync record is
--  written in the SAME transaction as the business change. There is no window
--  in which a journal entry exists but its sync record does not, or the
--  reverse. Either both commit or neither does.
--
--  Everything else is downstream of that:
--    * node_seq is strictly monotonic per node, so the receiver applies in
--      order and a GAP is detectable -- a missing voucher halts sync loudly
--      instead of being silently skipped
--    * batches carry an idempotency key, so a network timeout that hides a
--      successful response can be retried safely
--    * payload_hash lets the receiver detect a payload that changed in flight
-- ═══════════════════════════════════════════════════════════════════════════


-- ── Per-node sequence ────────────────────────────────────────────────────────
-- Deliberately not a PostgreSQL sequence, for the same reason as document
-- numbering: sequences leak values on rollback, and a gap here is
-- indistinguishable from a lost record.
CREATE TABLE sync_sequence (
    node_id     uuid        PRIMARY KEY REFERENCES node(id) ON DELETE RESTRICT,
    tenant_id   uuid        NOT NULL REFERENCES tenant(id) ON DELETE RESTRICT,
    next_seq    bigint      NOT NULL DEFAULT 1 CHECK (next_seq >= 1),
    updated_at  timestamptz NOT NULL DEFAULT now()
);


CREATE TABLE sync_outbox (
    id          uuid        PRIMARY KEY DEFAULT uuid_generate_v7(),
    tenant_id   uuid        NOT NULL REFERENCES tenant(id) ON DELETE RESTRICT,
    node_id     uuid        NOT NULL REFERENCES node(id) ON DELETE RESTRICT,

    -- Position in this node's stream. Contiguous by construction.
    node_seq    bigint      NOT NULL,

    entity      text        NOT NULL,
    entity_id   uuid        NOT NULL,
    op          text        NOT NULL CHECK (op IN ('insert','update','delete')),

    payload     jsonb       NOT NULL,
    payload_hash bytea      NOT NULL,

    created_at  timestamptz NOT NULL DEFAULT now(),

    -- Shipping state. NULL batch_id means "not yet claimed by a batch".
    batch_id    uuid,
    shipped_at  timestamptz,
    acked_at    timestamptz,

    UNIQUE (node_id, node_seq)
);

-- The shipper's hot path: the oldest unacknowledged records for a node.
CREATE INDEX sync_outbox_pending
    ON sync_outbox (node_id, node_seq)
    WHERE acked_at IS NULL;

CREATE INDEX sync_outbox_entity
    ON sync_outbox (tenant_id, entity, entity_id);


-- ── Batches ──────────────────────────────────────────────────────────────────
CREATE TABLE sync_batch (
    id          uuid        PRIMARY KEY DEFAULT uuid_generate_v7(),
    tenant_id   uuid        NOT NULL REFERENCES tenant(id) ON DELETE RESTRICT,
    node_id     uuid        NOT NULL REFERENCES node(id) ON DELETE RESTRICT,

    from_seq    bigint      NOT NULL,
    to_seq      bigint      NOT NULL,
    record_count integer    NOT NULL CHECK (record_count > 0),

    -- Ed25519 signature over the batch payload, made with the node's private
    -- key. The receiver verifies it against node.public_key, so a stolen API
    -- credential alone cannot inject forged ledger entries.
    signature   bytea,

    status      text        NOT NULL DEFAULT 'pending'
                            CHECK (status IN ('pending','sent','acknowledged','rejected')),
    attempts    smallint    NOT NULL DEFAULT 0,
    last_error  text,

    created_at  timestamptz NOT NULL DEFAULT now(),
    sent_at     timestamptz,
    acked_at    timestamptz,

    CHECK (to_seq >= from_seq)
);

CREATE INDEX sync_batch_pending
    ON sync_batch (tenant_id, node_id, created_at)
    WHERE status IN ('pending','sent');


-- ── Receiver-side idempotency ────────────────────────────────────────────────
-- These live on the cloud, but the migrations are shared so both sides agree
-- on the shape. On an office node they simply stay empty.
CREATE TABLE sync_applied_batch (
    batch_id        uuid        PRIMARY KEY,
    tenant_id       uuid        NOT NULL,
    node_id         uuid        NOT NULL,
    from_seq        bigint      NOT NULL,
    to_seq          bigint      NOT NULL,
    record_count    integer     NOT NULL,
    applied_at      timestamptz NOT NULL DEFAULT now(),
    -- The original response, replayed verbatim when a retry arrives.
    result          jsonb       NOT NULL
);

CREATE TABLE sync_cursor (
    node_id             uuid        PRIMARY KEY,
    tenant_id           uuid        NOT NULL,
    last_applied_seq    bigint      NOT NULL DEFAULT 0,
    last_applied_at     timestamptz,
    updated_at          timestamptz NOT NULL DEFAULT now()
);


-- ── Enqueue ──────────────────────────────────────────────────────────────────
-- Allocates the next sequence number for the node under a row lock, so two
-- concurrent transactions can never be handed the same one.
CREATE OR REPLACE FUNCTION sync_enqueue(
    p_node_id   uuid,
    p_entity    text,
    p_entity_id uuid,
    p_op        text,
    p_payload   jsonb
)
RETURNS bigint
LANGUAGE plpgsql
VOLATILE
AS $$
DECLARE
    v_tenant_id uuid;
    v_seq       bigint;
BEGIN
    SELECT tenant_id INTO v_tenant_id FROM node WHERE id = p_node_id;
    IF v_tenant_id IS NULL THEN
        RAISE EXCEPTION 'unknown node %', p_node_id
            USING ERRCODE = 'foreign_key_violation';
    END IF;

    INSERT INTO sync_sequence (node_id, tenant_id) VALUES (p_node_id, v_tenant_id)
    ON CONFLICT (node_id) DO NOTHING;

    UPDATE sync_sequence
       SET next_seq   = next_seq + 1,
           updated_at = now()
     WHERE node_id    = p_node_id
    RETURNING next_seq - 1 INTO v_seq;

    INSERT INTO sync_outbox (tenant_id, node_id, node_seq, entity, entity_id, op, payload, payload_hash)
    VALUES (
        v_tenant_id, p_node_id, v_seq, p_entity, p_entity_id, p_op, p_payload,
        digest(convert_to(p_payload::text, 'UTF8'), 'sha256')
    );

    RETURN v_seq;
END
$$;

COMMENT ON FUNCTION sync_enqueue IS
    'Must be called inside the same transaction as the business change it '
    'describes. That co-commit is the entire point of the outbox pattern.';


-- ── Gap detection ────────────────────────────────────────────────────────────
-- Run on the receiver before applying a batch, and on the sender as a health
-- check. An empty result means the stream is contiguous.
CREATE OR REPLACE FUNCTION sync_find_gaps(p_node_id uuid)
RETURNS TABLE (missing_from bigint, missing_to bigint)
LANGUAGE sql
STABLE
AS $$
    SELECT prev_seq + 1, node_seq - 1
      FROM (
        SELECT node_seq,
               lag(node_seq) OVER (ORDER BY node_seq) AS prev_seq
          FROM sync_outbox
         WHERE node_id = p_node_id
      ) s
     WHERE prev_seq IS NOT NULL
       AND node_seq <> prev_seq + 1
$$;

COMMENT ON FUNCTION sync_find_gaps IS
    'A gap means a record was lost or removed between here and the receiver. '
    'Sync must stop and be investigated rather than skipping ahead -- silently '
    'continuing would leave a voucher permanently absent from the cloud books.';


CREATE TRIGGER sync_sequence_touch
    BEFORE UPDATE ON sync_sequence
    FOR EACH ROW EXECUTE FUNCTION touch_updated_at();

SELECT apply_tenant_guards('sync_outbox');
SELECT apply_tenant_guards('sync_batch');
SELECT apply_tenant_guards('sync_sequence');
