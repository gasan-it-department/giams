-- ════════════════════════════════════════════════════════════════════════════
-- 0027 — Two things the row-shaped upsert got wrong
--
-- ── 1. Generated columns ────────────────────────────────────────────────────
--
-- `INSERT INTO account SELECT * FROM jsonb_populate_record(...)` writes every
-- column positionally, including `normal_balance`, which is GENERATED. Postgres
-- refuses a non-default value for a generated column, so the very first batch
-- carrying an account was rejected in full.
--
-- The columns are now listed explicitly, taken from the receiving table and
-- filtered to those that can actually be written. A generated column is
-- recomputed on arrival, which is what it is for.
--
-- ── 2. Scrubbing by deletion ────────────────────────────────────────────────
--
-- `sync_payload` removed `must_change_password`, `failed_attempts`, and
-- `locked_until` from the `app_user` payload. `jsonb_populate_record` turns a
-- missing key into NULL, and all three are NOT NULL — so the scrub that was
-- meant to withhold information would instead have failed the batch.
--
-- They are set to harmless values rather than removed. The intent was never to
-- omit the columns; it was to make sure the cloud's copy of an account carries
-- nothing that helps anyone sign in as it.
-- ════════════════════════════════════════════════════════════════════════════

CREATE OR REPLACE FUNCTION sync_payload(p_table text, p_row jsonb)
RETURNS jsonb
LANGUAGE sql
IMMUTABLE
AS $$
    SELECT CASE p_table
        WHEN 'app_user' THEN
            p_row || jsonb_build_object(
                -- Not a valid Argon2 hash, so verification against it can only
                -- ever fail. The cloud's copy of an account cannot be signed
                -- in to even if the cloud is breached.
                'password_hash',        '!',
                'is_active',            false,
                'must_change_password', false,
                'failed_attempts',      0,
                'locked_until',         NULL,
                'last_login_at',        NULL
            )
        ELSE p_row
    END
$$;

COMMENT ON FUNCTION sync_payload IS
    'The outbound form of a row. Where a table holds something the cloud must '
    'not have, this is where it is neutralised -- by setting it, never by '
    'removing the key, because a missing key arrives as NULL.';


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
    v_columns text;
    v_set     text;
    v_id      uuid;
BEGIN
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

    -- Writable columns only: a generated column is recomputed on arrival, and
    -- an identity column is the receiving table's to allocate. Both are read
    -- from the receiving table rather than from the payload, so a payload
    -- carrying an unexpected field cannot reach a column at all.
    SELECT string_agg(quote_ident(column_name), ', ' ORDER BY ordinal_position),
           string_agg(
               format('%I = excluded.%I', column_name, column_name), ', '
           ) FILTER (WHERE column_name <> 'id')
      INTO v_columns, v_set
      FROM information_schema.columns
     WHERE table_schema = 'public'
       AND table_name   = p_entity
       AND is_generated = 'NEVER'
       AND is_identity  = 'NO';

    EXECUTE format(
        'INSERT INTO %I (%s) SELECT %s FROM jsonb_populate_record(NULL::%I, $1)
         ON CONFLICT (id) DO UPDATE SET %s',
        p_entity, v_columns, v_columns, p_entity, v_set
    ) USING p_payload;
END
$$;
