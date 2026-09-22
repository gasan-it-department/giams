-- ════════════════════════════════════════════════════════════════════════════
-- 0024 — The first upload, and what it needs to carry
--
-- Two failures found by shipping a real journal entry to a real receiver.
--
-- ── 1. Replication only ever carried the future ─────────────────────────────
--
-- Capture is a trigger, so it fires on changes made after 0021 was applied and
-- on nothing before it. An office that has been running for a year would
-- upload only what it did today, and the cloud would hold a ledger that
-- referenced a chart of accounts it had never been sent.
--
-- The symptom was immediate and total: the very first journal entry was
-- refused, because its fund, its period, and the officer who prepared it did
-- not exist on the receiving side.
--
-- `sync_backfill` fixes it by enqueuing everything already on file, once, in
-- an order that satisfies the foreign keys on the way in.
--
-- ── 2. Three referenced tables were not replicated ──────────────────────────
--
-- `app_user`, `deduction_type`, and `deduction_rate` are pointed at by ledger
-- facts. Without them the facts cannot land.
--
-- `app_user` is the delicate one: those rows carry password hashes, and the
-- cloud has no business holding them. So its payload is **scrubbed** on the
-- way out — the name and position that a voucher must show are kept, and the
-- credential is replaced with a value no password can ever match. The cloud
-- learns who signed what, and gains no ability to sign anything.
-- ════════════════════════════════════════════════════════════════════════════


-- ── Order of application ─────────────────────────────────────────────────────
ALTER TABLE sync_replicated_table
    ADD COLUMN IF NOT EXISTS apply_order integer NOT NULL DEFAULT 100;

COMMENT ON COLUMN sync_replicated_table.apply_order IS
    'Lower goes first on a backfill, so a row never arrives before what it '
    'references. Within a table, oldest first handles self-references: a '
    'parent account exists before its children, and a reversing entry after '
    'the entry it reverses.';


CREATE OR REPLACE FUNCTION replicate_table(p_table text, p_order integer DEFAULT 100)
RETURNS void
LANGUAGE plpgsql
AS $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.tables
         WHERE table_schema = 'public' AND table_name = p_table
    ) THEN
        RAISE EXCEPTION 'there is no table called %', p_table;
    END IF;

    EXECUTE format('DROP TRIGGER IF EXISTS %I ON %I', p_table || '_sync_capture', p_table);
    EXECUTE format(
        'CREATE TRIGGER %I AFTER INSERT OR UPDATE OR DELETE ON %I
             FOR EACH ROW EXECUTE FUNCTION sync_capture()',
        p_table || '_sync_capture', p_table
    );

    INSERT INTO sync_replicated_table (table_name, apply_order)
         VALUES (p_table, p_order)
    ON CONFLICT (table_name) DO UPDATE SET apply_order = excluded.apply_order;
END
$$;


-- The three that were missing, plus the order for everything.
SELECT replicate_table('app_user',              10);
SELECT replicate_table('fund',                  20);
SELECT replicate_table('account',               30);
SELECT replicate_table('office',                40);
SELECT replicate_table('payee',                 50);
SELECT replicate_table('deduction_type',        60);
SELECT replicate_table('deduction_rate',        70);
SELECT replicate_table('bank_account',          80);
SELECT replicate_table('posting_period',        90);
SELECT replicate_table('appropriation',        100);
SELECT replicate_table('allotment',            110);
SELECT replicate_table('obligation',           120);
SELECT replicate_table('disbursement_voucher', 130);
SELECT replicate_table('dv_deduction',         140);
SELECT replicate_table('disbursement',         150);
SELECT replicate_table('report_of_collections',160);
SELECT replicate_table('official_receipt',     170);
SELECT replicate_table('journal_entry',        180);
SELECT replicate_table('journal_entry_line',   190);


-- ── What actually goes on the wire ───────────────────────────────────────────
CREATE OR REPLACE FUNCTION sync_payload(p_table text, p_row jsonb)
RETURNS jsonb
LANGUAGE sql
IMMUTABLE
AS $$
    SELECT CASE p_table
        WHEN 'app_user' THEN
            -- The name and position appear on printed vouchers, so they travel.
            -- The credential does not. '!' is not a valid Argon2 hash, so
            -- verification against it can only ever fail — the cloud copy of an
            -- account cannot be signed in to even if the cloud is breached.
            p_row
              || jsonb_build_object('password_hash', '!')
              || jsonb_build_object('is_active', false)
              - 'must_change_password'
              - 'failed_attempts'
              - 'locked_until'
        ELSE p_row
    END
$$;

COMMENT ON FUNCTION sync_payload IS
    'The outbound form of a row. Where a table holds something the cloud must '
    'not have, this is where it is removed -- once, so capture and backfill '
    'cannot disagree about it.';


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
    IF sync_is_replaying() THEN
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

    SELECT id INTO v_node_id
      FROM node
     WHERE tenant_id = (v_row ->> 'tenant_id')::uuid
       AND is_primary
     LIMIT 1;

    IF v_node_id IS NULL OR v_id IS NULL THEN
        RETURN coalesce(NEW, OLD);
    END IF;

    PERFORM sync_enqueue(v_node_id, TG_TABLE_NAME, v_id, v_op,
                         sync_payload(TG_TABLE_NAME, v_row));

    RETURN coalesce(NEW, OLD);
END
$$;


-- ── The first upload ─────────────────────────────────────────────────────────
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
BEGIN
    SELECT id INTO v_node_id
      FROM node
     WHERE tenant_id = p_tenant_id AND is_primary
     LIMIT 1;

    IF v_node_id IS NULL THEN
        RAISE EXCEPTION 'this database holds no primary node, so it has nothing to upload';
    END IF;

    -- Once only. Running it twice would queue a second copy of the entire
    -- ledger, which the receiver would apply harmlessly as upserts but which
    -- would take hours over a provincial connection for no gain.
    IF EXISTS (SELECT 1 FROM sync_outbox WHERE node_id = v_node_id AND entity = 'backfill') THEN
        RETURN 0;
    END IF;

    FOR v_table IN
        SELECT table_name FROM sync_replicated_table ORDER BY apply_order, table_name
    LOOP
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
            v_total := v_total + 1;
        END LOOP;
    END LOOP;

    -- A marker, so this is never done twice. It carries the count so an
    -- operator reading the outbox can see what the first upload consisted of.
    PERFORM sync_enqueue(
        v_node_id, 'backfill', v_node_id, 'insert',
        jsonb_build_object('records', v_total, 'at', now())
    );

    RETURN v_total;
END
$$;

COMMENT ON FUNCTION sync_backfill IS
    'Queues everything already on file, once, in foreign key order. Run when a '
    'node is first connected to a cloud service; does nothing on later runs.';


-- The marker is not a table and must never be applied as one. `sync_apply_record`
-- already refuses anything absent from the whitelist, so this only needs the
-- receiver to skip it rather than fail on it.
CREATE OR REPLACE FUNCTION sync_apply_record(
    p_entity  text,
    p_op      text,
    p_payload jsonb
)
RETURNS void
LANGUAGE plpgsql
VOLATILE
AS $$
DECLARE
    v_set text;
    v_id  uuid;
BEGIN
    -- Bookkeeping about the stream itself, not a row in it.
    IF p_entity = 'backfill' THEN
        RETURN;
    END IF;

    IF NOT EXISTS (SELECT 1 FROM sync_replicated_table WHERE table_name = p_entity) THEN
        RAISE EXCEPTION 'the batch names a table that is not replicated: %', p_entity
            USING HINT = 'The batch has been rejected. This should be reported.';
    END IF;

    v_id := (p_payload ->> 'id')::uuid;
    IF v_id IS NULL THEN
        RAISE EXCEPTION 'a record in the batch has no id';
    END IF;

    IF p_op = 'delete' THEN
        EXECUTE format('DELETE FROM %I WHERE id = $1', p_entity) USING v_id;
        RETURN;
    END IF;

    SELECT string_agg(format('%I = excluded.%I', column_name, column_name), ', ')
      INTO v_set
      FROM information_schema.columns
     WHERE table_schema = 'public'
       AND table_name   = p_entity
       AND column_name <> 'id';

    EXECUTE format(
        'INSERT INTO %I SELECT * FROM jsonb_populate_record(NULL::%I, $1)
         ON CONFLICT (id) DO UPDATE SET %s',
        p_entity, p_entity, v_set
    ) USING p_payload;
END
$$;
