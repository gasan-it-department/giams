-- ════════════════════════════════════════════════════════════════════════════
-- 0023 — Applying a received batch, on the cloud side
--
-- The receiver's half of replication. These functions live in the shared
-- migrations because both sides must agree on the shape; on an office node
-- they are simply never called.
--
-- ── Why the invariants stand down during replay ─────────────────────────────
--
-- A journal entry and its lines are separate outbox records, so a batch
-- boundary can fall between them. The cloud would then commit an entry whose
-- lines have not all arrived, and `assert_entry_balanced` — which fires at
-- COMMIT — would refuse the batch. The next attempt would claim the same
-- records and fail identically. Replication would stop permanently, on
-- correct data.
--
-- The resolution is not to weaken the invariant. It is to recognise that the
-- **office node is the sole writer of ledger facts** (docs/ARCHITECTURE.md §2)
-- and has already enforced every one of these rules at the moment of posting.
-- A replica re-litigating a cross-row invariant against a partial view is not
-- checking anything; it is only guessing about records it has not been handed
-- yet. So during replay the cross-row assertions stand down, and integrity is
-- carried instead by what actually protects it across the wire: the per-node
-- sequence with gap detection, the payload hash, and the Ed25519 batch
-- signature.
--
-- Single-row constraints — types, NOT NULL, CHECK, foreign keys — are not
-- touched and still apply in full.
-- ════════════════════════════════════════════════════════════════════════════


-- ── The whitelist ────────────────────────────────────────────────────────────
-- `sync_apply_record` builds SQL from a table name that arrived over the
-- network. That is only safe against a fixed list, so the list is data rather
-- than a comment, and `replicate_table` is what maintains it.
CREATE TABLE IF NOT EXISTS sync_replicated_table (
    table_name  text        PRIMARY KEY,
    added_at    timestamptz NOT NULL DEFAULT now()
);

COMMENT ON TABLE sync_replicated_table IS
    'Tables that replicate node -> cloud. The receiver refuses any entity not '
    'named here, which is what stops a forged batch naming app_user.';


CREATE OR REPLACE FUNCTION replicate_table(p_table text)
RETURNS void
LANGUAGE plpgsql
AS $$
BEGIN
    -- Refuse a table that does not exist, rather than creating a trigger on
    -- nothing and reporting success.
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

    INSERT INTO sync_replicated_table (table_name) VALUES (p_table)
    ON CONFLICT (table_name) DO NOTHING;
END
$$;

-- Backfill the list from the triggers 0021 already created.
INSERT INTO sync_replicated_table (table_name)
SELECT c.relname
  FROM pg_trigger t
  JOIN pg_class c ON c.oid = t.tgrelid
 WHERE t.tgname LIKE '%\_sync\_capture'
ON CONFLICT (table_name) DO NOTHING;


-- ── Standing down during replay ──────────────────────────────────────────────
CREATE OR REPLACE FUNCTION sync_is_replaying()
RETURNS boolean
LANGUAGE sql
STABLE
AS $$
    SELECT coalesce(current_setting('app.replaying', true), '') = 'on'
$$;

COMMENT ON FUNCTION sync_is_replaying IS
    'True while a received batch is being applied. Set with SET LOCAL, so it '
    'cannot outlive the transaction that set it.';


-- The cross-row assertions, reissued from 0005 with only the replay guard
-- added. The bodies below are otherwise byte-identical to the originals:
-- rewriting one from memory while adding a guard is how a rule quietly
-- loses a clause, so they are reproduced rather than retyped.

CREATE OR REPLACE FUNCTION assert_entry_balanced()
RETURNS trigger
LANGUAGE plpgsql
AS $$
DECLARE
    v_entry_id   uuid;
    v_debit      numeric(18,4);
    v_credit     numeric(18,4);
    v_lines      integer;
    v_jev_no     text;
    v_status     document_status;
    v_difference numeric(18,4);
BEGIN
    -- Stands down during replay: see this migration's header. The node
    -- has already enforced this, and a batch boundary can legitimately
    -- separate an entry from some of its lines.
    IF sync_is_replaying() THEN
        RETURN NULL;
    END IF;
    v_entry_id := CASE TG_OP WHEN 'DELETE' THEN OLD.entry_id ELSE NEW.entry_id END;

    SELECT je.jev_no, je.status INTO v_jev_no, v_status
      FROM journal_entry je
     WHERE je.id = v_entry_id;

    -- The whole entry was deleted; its lines going with it is not a violation.
    IF NOT FOUND THEN
        RETURN NULL;
    END IF;

    -- Drafts are allowed to be incomplete. The rule binds from submission on.
    IF v_status = 'draft' THEN
        RETURN NULL;
    END IF;

    SELECT coalesce(sum(l.debit), 0), coalesce(sum(l.credit), 0), count(*)
      INTO v_debit, v_credit, v_lines
      FROM journal_entry_line l
     WHERE l.entry_id = v_entry_id;

    IF v_lines < 2 THEN
        RAISE EXCEPTION
            'Journal entry % needs at least one debit and one credit.', v_jev_no
            USING ERRCODE = 'check_violation',
                  HINT    = 'Add the missing side of the entry before submitting it.';
    END IF;

    IF v_debit <> v_credit THEN
        v_difference := abs(v_debit - v_credit);
        RAISE EXCEPTION
            'Journal entry % does not balance. Debits total %, credits total %, a difference of %.',
            v_jev_no,
            to_char(v_debit,      'FM999,999,999,990.00'),
            to_char(v_credit,     'FM999,999,999,990.00'),
            to_char(v_difference, 'FM999,999,999,990.00')
            USING ERRCODE = 'check_violation',
                  HINT    = 'Check the amounts on each line. Total debits must equal total credits.';
    END IF;

    RETURN NULL;
END
$$;


CREATE OR REPLACE FUNCTION assert_entry_has_lines()
RETURNS trigger
LANGUAGE plpgsql
AS $$
DECLARE
    v_debit  numeric(18,4);
    v_credit numeric(18,4);
    v_lines  integer;
BEGIN
    IF sync_is_replaying() THEN
        RETURN NULL;
    END IF;
    IF NEW.status = 'draft' THEN
        RETURN NULL;
    END IF;

    SELECT coalesce(sum(debit), 0), coalesce(sum(credit), 0), count(*)
      INTO v_debit, v_credit, v_lines
      FROM journal_entry_line
     WHERE entry_id = NEW.id;

    IF v_lines = 0 THEN
        RAISE EXCEPTION 'Journal entry % has no accounting lines.', NEW.jev_no
            USING ERRCODE = 'check_violation',
                  HINT    = 'Add the debit and credit lines before submitting.';
    END IF;

    IF v_debit <> v_credit THEN
        RAISE EXCEPTION
            'Journal entry % does not balance. Debits total %, credits total %.',
            NEW.jev_no,
            to_char(v_debit,  'FM999,999,999,990.00'),
            to_char(v_credit, 'FM999,999,999,990.00')
            USING ERRCODE = 'check_violation',
                  HINT    = 'Total debits must equal total credits.';
    END IF;

    RETURN NULL;
END
$$;


CREATE OR REPLACE FUNCTION assert_period_open()
RETURNS trigger
LANGUAGE plpgsql
AS $$
DECLARE
    v_status    text;
    v_year      smallint;
    v_month     smallint;
BEGIN
    -- A month is closed by a record that arrives in this same stream;
    -- re-checking here would refuse the entries that closed it.
    IF sync_is_replaying() THEN
        RETURN NEW;
    END IF;
    SELECT status, fiscal_year, month
      INTO v_status, v_year, v_month
      FROM posting_period
     WHERE id = NEW.period_id;

    IF v_status <> 'open' THEN
        RAISE EXCEPTION
            'The accounting period for % % is already closed.',
            to_char(to_date(v_month::text, 'MM'), 'FMMonth'), v_year
            USING ERRCODE = 'check_violation',
                  HINT    = 'Record this in the current open period, or ask an administrator to reopen the closed one.';
    END IF;

    -- The entry date must fall inside the period it claims.
    IF NOT EXISTS (
        SELECT 1 FROM posting_period
         WHERE id = NEW.period_id
           AND NEW.entry_date BETWEEN starts_on AND ends_on
    ) THEN
        RAISE EXCEPTION
            'The entry date does not fall within % %.',
            to_char(to_date(v_month::text, 'MM'), 'FMMonth'), v_year
            USING ERRCODE = 'check_violation',
                  HINT    = 'Check the date, or select the period that contains it.';
    END IF;

    RETURN NEW;
END
$$;


CREATE OR REPLACE FUNCTION journal_entry_is_append_only()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    -- The posted state arriving after the draft state is precisely what
    -- replication must do, and precisely what this rule forbids locally.
    IF sync_is_replaying() THEN
        RETURN CASE TG_OP WHEN 'DELETE' THEN OLD ELSE NEW END;
    END IF;
    IF TG_OP = 'DELETE' THEN
        RAISE EXCEPTION 'Journal entry % has been posted and cannot be deleted.', OLD.jev_no
            USING ERRCODE = 'restrict_violation',
                  HINT    = 'Create a reversing entry instead.';
    END IF;

    IF OLD.status = 'posted' THEN
        IF NEW.status              IS DISTINCT FROM OLD.status
        OR NEW.jev_no              IS DISTINCT FROM OLD.jev_no
        OR NEW.fund_id             IS DISTINCT FROM OLD.fund_id
        OR NEW.period_id           IS DISTINCT FROM OLD.period_id
        OR NEW.entry_date          IS DISTINCT FROM OLD.entry_date
        OR NEW.particulars         IS DISTINCT FROM OLD.particulars
        OR NEW.journal_type        IS DISTINCT FROM OLD.journal_type
        OR NEW.posted_by           IS DISTINCT FROM OLD.posted_by
        OR NEW.posted_at           IS DISTINCT FROM OLD.posted_at
        THEN
            RAISE EXCEPTION 'Journal entry % has been posted and can no longer be changed.', OLD.jev_no
                USING ERRCODE = 'restrict_violation',
                      HINT    = 'Create a reversing entry to correct it.';
        END IF;
    END IF;

    RETURN NEW;
END
$$;


CREATE OR REPLACE FUNCTION journal_line_is_append_only()
RETURNS trigger
LANGUAGE plpgsql
AS $$
DECLARE
    v_status document_status;
    v_jev_no text;
BEGIN
    IF sync_is_replaying() THEN
        RETURN CASE TG_OP WHEN 'DELETE' THEN OLD ELSE NEW END;
    END IF;
    SELECT status, jev_no INTO v_status, v_jev_no
      FROM journal_entry
     WHERE id = CASE TG_OP WHEN 'DELETE' THEN OLD.entry_id ELSE NEW.entry_id END;

    IF v_status = 'posted' THEN
        RAISE EXCEPTION 'Journal entry % has been posted and its lines cannot be changed.', v_jev_no
            USING ERRCODE = 'restrict_violation',
                  HINT    = 'Create a reversing entry to correct it.';
    END IF;

    RETURN CASE TG_OP WHEN 'DELETE' THEN OLD ELSE NEW END;
END
$$;


-- ── Applying one record ──────────────────────────────────────────────────────
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

    -- Every column but the key, taken from the receiving table's own
    -- definition rather than from the payload — so a payload carrying an
    -- extra field cannot write to a column this side does not expect.
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

COMMENT ON FUNCTION sync_apply_record IS
    'Upserts one replicated row. Refuses any table not on the whitelist, '
    'because the table name arrived over the network.';


-- ── Applying a batch ─────────────────────────────────────────────────────────
-- Idempotent by batch id. A retry after a lost response replays the original
-- answer instead of applying anything twice, which is the case the shipper
-- produces every time a connection drops at the wrong moment.
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

    SELECT last_applied_seq INTO v_cursor FROM sync_cursor WHERE node_id = p_node_id;
    v_cursor := coalesce(v_cursor, 0);

    -- Out of order, or with a hole before it. Refused rather than applied:
    -- accepting seq 40 when 39 never arrived would leave a voucher missing
    -- from the cloud books with nothing downstream ever indicating it.
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

COMMENT ON FUNCTION sync_apply_batch IS
    'Applies a received batch exactly once. Refuses anything out of order, '
    'and replays the original answer for a batch already applied.';
