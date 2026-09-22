-- ════════════════════════════════════════════════════════════════════════════
-- 0026 — Letting a batch settle its own foreign keys
--
-- A batch carries records in the order they happened, which is not the order
-- the foreign keys need them in. A deduction type updated this morning is
-- earlier in the stream than the chart of accounts it points at, because the
-- chart was created a year ago and only entered the stream when the node was
-- first connected to a cloud.
--
-- Applying that batch row by row fails on the first record whose reference has
-- not arrived yet — even though every record needed is in the same batch, a
-- few rows further down.
--
-- The fix is the one Postgres provides for exactly this: make the foreign keys
-- DEFERRABLE, and defer them for the duration of a replay. Every reference is
-- then checked once, at COMMIT, against the batch as a whole. A batch that is
-- internally consistent applies; one that genuinely references something it
-- was never sent is still refused, in full, with nothing half-applied.
--
-- Note what this does not do. It defers *when* the keys are checked, never
-- *whether*. And it is deferred only inside `sync_apply_batch`; ordinary work
-- on a node still fails immediately on a bad reference, which is what a clerk
-- needs.
-- ════════════════════════════════════════════════════════════════════════════

DO $$
DECLARE
    v_fk      record;
    v_changed integer := 0;
BEGIN
    FOR v_fk IN
        SELECT c.conname,
               c.conrelid::regclass::text AS child,
               pg_get_constraintdef(c.oid) AS definition
          FROM pg_constraint c
         WHERE c.contype = 'f'
           AND NOT c.condeferrable
           AND c.conrelid::regclass::text IN (
                 SELECT table_name FROM sync_replicated_table
               )
    LOOP
        -- Dropped and re-added rather than altered: PostgreSQL has no
        -- ALTER CONSTRAINT that can turn a non-deferrable key deferrable.
        -- The definition is taken from the catalogue, so the re-added key is
        -- the same key — same columns, same ON DELETE behaviour — with only
        -- the deferrability changed.
        EXECUTE format('ALTER TABLE %s DROP CONSTRAINT %I', v_fk.child, v_fk.conname);
        EXECUTE format(
            'ALTER TABLE %s ADD CONSTRAINT %I %s DEFERRABLE INITIALLY IMMEDIATE',
            v_fk.child, v_fk.conname, v_fk.definition
        );

        v_changed := v_changed + 1;
    END LOOP;

    RAISE NOTICE 'made % foreign keys deferrable for replication', v_changed;
END
$$;


CREATE OR REPLACE FUNCTION sync_apply_batch(
    p_batch_id  uuid,
    p_tenant_id uuid,
    p_node_id   uuid,
    p_from_seq  bigint,
    p_to_seq    bigint,
    p_records   jsonb
)
RETURNS jsonb
LANGUAGE plpgsql
VOLATILE
AS $$
DECLARE
    v_previous  jsonb;
    v_cursor    bigint;
    v_record    jsonb;
    v_count     integer := 0;
    v_expected  bigint;
BEGIN
    SELECT result INTO v_previous FROM sync_applied_batch WHERE batch_id = p_batch_id;
    IF v_previous IS NOT NULL THEN
        RETURN v_previous;
    END IF;

    PERFORM set_config('app.replaying', 'on', true);
    PERFORM set_config('app.tenant_id', p_tenant_id::text, true);

    -- Checked at COMMIT instead of per row, so a batch may carry a record and
    -- the record it references in any order. Scoped to this transaction.
    SET CONSTRAINTS ALL DEFERRED;

    SELECT last_applied_seq INTO v_cursor FROM sync_cursor WHERE node_id = p_node_id;
    v_cursor := coalesce(v_cursor, 0);

    IF p_from_seq <> v_cursor + 1 THEN
        RAISE EXCEPTION
            'this batch starts at % but the next expected record is %',
            p_from_seq, v_cursor + 1
            USING HINT = 'The batch has been rejected. The node should resend '
                         'from the expected position.';
    END IF;

    v_expected := p_from_seq;

    FOR v_record IN SELECT * FROM jsonb_array_elements(p_records)
    LOOP
        IF (v_record ->> 'node_seq')::bigint <> v_expected THEN
            RAISE EXCEPTION
                'the batch is missing record %', v_expected
                USING HINT = 'The batch has been rejected.';
        END IF;

        PERFORM sync_apply_record(
            v_record ->> 'entity',
            v_record ->> 'op',
            v_record -> 'payload'
        );

        v_expected := v_expected + 1;
        v_count    := v_count + 1;
    END LOOP;

    IF v_expected - 1 <> p_to_seq THEN
        RAISE EXCEPTION
            'the batch claims to end at % but carries records up to %',
            p_to_seq, v_expected - 1;
    END IF;

    INSERT INTO sync_cursor (node_id, tenant_id, last_applied_seq, last_applied_at)
         VALUES (p_node_id, p_tenant_id, p_to_seq, now())
    ON CONFLICT (node_id) DO UPDATE
            SET last_applied_seq = excluded.last_applied_seq,
                last_applied_at  = excluded.last_applied_at,
                updated_at       = now();

    v_previous := jsonb_build_object(
        'batch_id', p_batch_id,
        'applied',  v_count,
        'through',  p_to_seq
    );

    INSERT INTO sync_applied_batch
        (batch_id, tenant_id, node_id, from_seq, to_seq, record_count, result)
    VALUES
        (p_batch_id, p_tenant_id, p_node_id, p_from_seq, p_to_seq, v_count, v_previous);

    RETURN v_previous;
END
$$;
