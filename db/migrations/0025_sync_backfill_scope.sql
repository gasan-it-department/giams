-- ════════════════════════════════════════════════════════════════════════════
-- 0025 — Giving the backfill a tenant to see
--
-- `sync_backfill` reads every replicated table directly. Those tables are
-- under row level security, which resolves `app.tenant_id` from the session —
-- and the backfill was called from a plain connection that had never set it.
--
-- So the policies did exactly what they are for and returned nothing, table
-- after table. The backfill reported success, queued 324 records instead of
-- the several hundred it should have, and the first upload was refused because
-- the ledger referenced a chart of accounts that had been silently skipped.
--
-- This is the failure mode row level security is prone to: it does not raise,
-- it filters. A missing scope reads as an empty table, which reads as a
-- database with nothing in it.
--
-- The scope is now set inside the function, so it cannot be forgotten by a
-- caller. `true` makes it local to the transaction, so it is released on
-- commit and cannot leak to the next borrower of a pooled connection.
-- ════════════════════════════════════════════════════════════════════════════

CREATE OR REPLACE FUNCTION sync_backfill(p_tenant_id uuid)
RETURNS integer
LANGUAGE plpgsql
VOLATILE
AS $$
DECLARE
    v_node_id uuid;
    v_table   record;
    v_row     jsonb;
    v_total   integer := 0;
    v_seen    integer;
BEGIN
    -- Set here rather than relied upon from the caller. Every table below is
    -- under RLS, and an unset scope makes all of them look empty.
    PERFORM set_config('app.tenant_id', p_tenant_id::text, true);

    SELECT id INTO v_node_id
      FROM node
     WHERE tenant_id = p_tenant_id AND is_primary
     LIMIT 1;

    IF v_node_id IS NULL THEN
        RAISE EXCEPTION 'this database holds no primary node, so it has nothing to upload';
    END IF;

    IF EXISTS (SELECT 1 FROM sync_outbox WHERE node_id = v_node_id AND entity = 'backfill') THEN
        RETURN 0;
    END IF;

    FOR v_table IN
        SELECT table_name FROM sync_replicated_table ORDER BY apply_order, table_name
    LOOP
        v_seen := 0;

        FOR v_row IN EXECUTE format(
            'SELECT to_jsonb(t) FROM %I t WHERE t.tenant_id = $1 ORDER BY t.created_at, t.id',
            v_table.table_name
        ) USING p_tenant_id
        LOOP
            PERFORM sync_enqueue(
                v_node_id,
                v_table.table_name,
                (v_row ->> 'id')::uuid,
                'insert',
                sync_payload(v_table.table_name, v_row)
            );
            v_seen  := v_seen + 1;
            v_total := v_total + 1;
        END LOOP;

        -- Announced per table, because "queued 324 records" is not something
        -- anyone can check, and "0 accounts" is something anyone can.
        RAISE NOTICE 'backfill: % rows from %', v_seen, v_table.table_name;
    END LOOP;

    PERFORM sync_enqueue(
        v_node_id, 'backfill', v_node_id, 'insert',
        jsonb_build_object('records', v_total, 'at', now())
    );

    RETURN v_total;
END
$$;
