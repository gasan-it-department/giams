-- ════════════════════════════════════════════════════════════════════════════
-- 0021 — Capturing changes into the outbox
--
-- Migration 0006 built the outbox and the sequence allocator, but nothing ever
-- called `sync_enqueue`. An outbox with no writers is a replication system
-- that ships nothing, forever, while reporting zero pending — the most
-- comfortable kind of broken.
--
-- This closes it with a trigger rather than by calling `sync_enqueue` from
-- application code. The reason is the whole point of the outbox pattern: the
-- sync record must commit in the SAME transaction as the change it describes.
-- A trigger cannot be forgotten by a new route, cannot be skipped on a code
-- path somebody adds in a hurry, and fires for changes made by hand in psql
-- during a migration. Application-level enqueue would be all three of those
-- failures waiting to happen.
--
-- ── What is captured ────────────────────────────────────────────────────────
--
-- Ledger facts and the reference data needed to read them. Not sessions, not
-- audit records (the cloud keeps its own chain), not the outbox itself.
--
-- ── The replay guard ────────────────────────────────────────────────────────
--
-- These migrations run on the cloud too, so the same triggers exist there.
-- When the receiver applies a batch it sets `app.replaying`, and the trigger
-- stands down — otherwise the cloud would enqueue everything it received and
-- try to ship it back, which is a loop that ends in a full disk.
-- ════════════════════════════════════════════════════════════════════════════


-- ── The capture trigger ──────────────────────────────────────────────────────
CREATE OR REPLACE FUNCTION sync_capture()
RETURNS trigger
LANGUAGE plpgsql
AS $$
DECLARE
    v_node_id uuid;
    v_row     jsonb;
    v_op      text;
    v_id      uuid;
BEGIN
    -- Applying a received batch, not originating a change.
    IF coalesce(current_setting('app.replaying', true), '') = 'on' THEN
        RETURN coalesce(NEW, OLD);
    END IF;

    IF TG_OP = 'DELETE' THEN
        v_row := to_jsonb(OLD);
        v_op  := 'delete';
    ELSIF TG_OP = 'UPDATE' THEN
        v_row := to_jsonb(NEW);
        v_op  := 'update';
    ELSE
        v_row := to_jsonb(NEW);
        v_op  := 'insert';
    END IF;

    v_id := (v_row ->> 'id')::uuid;

    -- The node that holds the official records for this municipality. On a
    -- workstation-only database there is none, and nothing is captured —
    -- which is correct, because a workstation holds drafts, not facts.
    SELECT id INTO v_node_id
      FROM node
     WHERE tenant_id = (v_row ->> 'tenant_id')::uuid
       AND is_primary
     LIMIT 1;

    IF v_node_id IS NULL OR v_id IS NULL THEN
        RETURN coalesce(NEW, OLD);
    END IF;

    PERFORM sync_enqueue(v_node_id, TG_TABLE_NAME, v_id, v_op, v_row);

    RETURN coalesce(NEW, OLD);
END
$$;

COMMENT ON FUNCTION sync_capture IS
    'Enqueues a row change into the outbox, in the same transaction as the '
    'change itself. Stands down while a received batch is being applied.';


-- ── Attaching it ─────────────────────────────────────────────────────────────
-- A helper, so adding a replicated table later is one line rather than five
-- and a chance to get the timing or the FOR EACH wrong.
CREATE OR REPLACE FUNCTION replicate_table(p_table text)
RETURNS void
LANGUAGE plpgsql
AS $$
BEGIN
    EXECUTE format('DROP TRIGGER IF EXISTS %I ON %I', p_table || '_sync_capture', p_table);
    EXECUTE format(
        'CREATE TRIGGER %I AFTER INSERT OR UPDATE OR DELETE ON %I
             FOR EACH ROW EXECUTE FUNCTION sync_capture()',
        p_table || '_sync_capture', p_table
    );
END
$$;

COMMENT ON FUNCTION replicate_table IS
    'Marks a table as replicated node -> cloud. AFTER, so the row has already '
    'passed every constraint before it is queued to ship.';


-- Reference data: needed to make sense of a ledger line in the cloud.
SELECT replicate_table('fund');
SELECT replicate_table('account');
SELECT replicate_table('office');
SELECT replicate_table('payee');
SELECT replicate_table('bank_account');
SELECT replicate_table('posting_period');

-- Budget execution.
SELECT replicate_table('appropriation');
SELECT replicate_table('allotment');
SELECT replicate_table('obligation');

-- Disbursement.
SELECT replicate_table('disbursement_voucher');
SELECT replicate_table('disbursement');
SELECT replicate_table('dv_deduction');

-- Collections.
SELECT replicate_table('official_receipt');
SELECT replicate_table('report_of_collections');

-- The ledger itself.
SELECT replicate_table('journal_entry');
SELECT replicate_table('journal_entry_line');


-- ── Claiming a batch ─────────────────────────────────────────────────────────
-- Takes the oldest unshipped records for a node, in sequence order, up to a
-- limit, and binds them to a new batch.
--
-- Contiguity is not assumed — it is checked. If the claimed records are not a
-- run of consecutive sequence numbers, something removed a row from the outbox
-- and the batch is refused rather than shipped with a hole in it.
CREATE OR REPLACE FUNCTION sync_claim_batch(
    p_node_id uuid,
    p_limit   integer DEFAULT 500
)
RETURNS TABLE (batch_id uuid, from_seq bigint, to_seq bigint, record_count integer)
LANGUAGE plpgsql
VOLATILE
AS $$
DECLARE
    v_tenant_id uuid;
    v_from      bigint;
    v_to        bigint;
    v_count     integer;
    v_batch_id  uuid;
BEGIN
    IF p_limit < 1 THEN
        RAISE EXCEPTION 'a batch must contain at least one record';
    END IF;

    SELECT tenant_id INTO v_tenant_id FROM node WHERE id = p_node_id;
    IF v_tenant_id IS NULL THEN
        RAISE EXCEPTION 'unknown node %', p_node_id USING ERRCODE = 'foreign_key_violation';
    END IF;

    -- The candidate run: unshipped, oldest first. Locked so two shippers
    -- running at once cannot both claim the same records.
    CREATE TEMP TABLE IF NOT EXISTS _claim (node_seq bigint) ON COMMIT DROP;
    DELETE FROM _claim;

    INSERT INTO _claim (node_seq)
    SELECT node_seq
      FROM sync_outbox
     WHERE node_id  = p_node_id
       AND acked_at IS NULL
       AND batch_id IS NULL
     ORDER BY node_seq
     LIMIT p_limit
       FOR UPDATE;

    SELECT min(node_seq), max(node_seq), count(*) INTO v_from, v_to, v_count FROM _claim;

    IF v_count = 0 THEN
        RETURN;
    END IF;

    -- A run of n consecutive numbers spans exactly n - 1. Anything else means
    -- a record went missing, and shipping around it would leave a voucher
    -- permanently absent from the cloud books.
    IF v_to - v_from + 1 <> v_count THEN
        RAISE EXCEPTION
            'the upload queue is missing records between % and %', v_from, v_to
            USING HINT = 'Sync has been stopped. Report this to the system administrator '
                         'before uploading again.';
    END IF;

    INSERT INTO sync_batch (tenant_id, node_id, from_seq, to_seq, record_count)
         VALUES (v_tenant_id, p_node_id, v_from, v_to, v_count)
      RETURNING id INTO v_batch_id;

    UPDATE sync_outbox o
       SET batch_id = v_batch_id
      FROM _claim c
     WHERE o.node_id = p_node_id AND o.node_seq = c.node_seq;

    RETURN QUERY SELECT v_batch_id, v_from, v_to, v_count;
END
$$;

COMMENT ON FUNCTION sync_claim_batch IS
    'Binds the oldest unshipped records to a new batch, refusing to ship a '
    'run with a gap in it.';


-- ── Acknowledging ────────────────────────────────────────────────────────────
-- Called once the cloud has confirmed the batch. Idempotent: a duplicate
-- acknowledgement, which is exactly what a retried request produces, changes
-- nothing.
CREATE OR REPLACE FUNCTION sync_acknowledge_batch(p_batch_id uuid)
RETURNS integer
LANGUAGE plpgsql
VOLATILE
AS $$
DECLARE
    v_acked integer;
BEGIN
    UPDATE sync_batch
       SET status = 'acknowledged', acked_at = coalesce(acked_at, now())
     WHERE id = p_batch_id AND status <> 'acknowledged';

    UPDATE sync_outbox
       SET acked_at = coalesce(acked_at, now())
     WHERE batch_id = p_batch_id AND acked_at IS NULL;

    GET DIAGNOSTICS v_acked = ROW_COUNT;
    RETURN v_acked;
END
$$;


-- ── Releasing a failed batch ─────────────────────────────────────────────────
-- A batch the cloud never confirmed goes back on the queue so the next attempt
-- picks it up. The records keep their sequence numbers, so the stream stays
-- contiguous and the receiver's idempotency check catches the case where the
-- cloud actually did apply it and only the response was lost.
CREATE OR REPLACE FUNCTION sync_release_batch(p_batch_id uuid, p_error text)
RETURNS void
LANGUAGE sql
VOLATILE
AS $$
    UPDATE sync_batch
       SET status     = 'pending',
           attempts   = attempts + 1,
           last_error = p_error
     WHERE id = p_batch_id AND status <> 'acknowledged';

    UPDATE sync_outbox
       SET batch_id = NULL
     WHERE batch_id = p_batch_id AND acked_at IS NULL;
$$;
