-- ════════════════════════════════════════════════════════════════════════════
-- 0022 — Correcting the batch claimer
--
-- `sync_claim_batch` in 0021 names its output columns after the outbox's own
-- (`batch_id`, `from_seq`, `to_seq`), so inside the function body every
-- unqualified reference is ambiguous and PostgreSQL refuses the query at run
-- time rather than at creation time. The first claim therefore failed with
-- `column reference "batch_id" is ambiguous`.
--
-- Rewritten with the table aliased and every column qualified, and with the
-- temporary table replaced by a CTE — the run is contiguous by the time it is
-- used, so a range is enough to bind it and there is nothing to keep.
-- ════════════════════════════════════════════════════════════════════════════

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

    SELECT n.tenant_id INTO v_tenant_id FROM node n WHERE n.id = p_node_id;
    IF v_tenant_id IS NULL THEN
        RAISE EXCEPTION 'unknown node %', p_node_id USING ERRCODE = 'foreign_key_violation';
    END IF;

    -- The candidate run: unshipped, oldest first. Locked, so two shippers
    -- running at once cannot both claim the same records.
    WITH candidate AS (
        SELECT o.node_seq
          FROM sync_outbox o
         WHERE o.node_id  = p_node_id
           AND o.acked_at IS NULL
           AND o.batch_id IS NULL
         ORDER BY o.node_seq
         LIMIT p_limit
           FOR UPDATE
    )
    SELECT min(c.node_seq), max(c.node_seq), count(*)
      INTO v_from, v_to, v_count
      FROM candidate c;

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
     WHERE o.node_id  = p_node_id
       AND o.node_seq BETWEEN v_from AND v_to
       AND o.batch_id IS NULL
       AND o.acked_at IS NULL;

    RETURN QUERY SELECT v_batch_id, v_from, v_to, v_count;
END
$$;

COMMENT ON FUNCTION sync_claim_batch IS
    'Binds the oldest unshipped records to a new batch, refusing to ship a '
    'run with a gap in it.';
