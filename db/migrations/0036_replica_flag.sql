-- ═══════════════════════════════════════════════════════════════════════════
--  0036 — Telling a migration it is running on the cloud copy
-- ═══════════════════════════════════════════════════════════════════════════
--  The cloud runs these same migrations. 0035 seeded the new accounts there
--  too, each with a new id, and the node's own copies were then refused as
--  duplicates (fixed by hand on 2026-11-23). The cloud's connections set
--  app.replica = 'on'; a migration that writes data skips it there:
--
--      IF NOT is_replica() THEN ... END IF;
--
--  The nodes make the change and upload it, so the cloud ends up with their
--  rows, ids and all.
-- ═══════════════════════════════════════════════════════════════════════════

CREATE OR REPLACE FUNCTION is_replica()
RETURNS boolean
LANGUAGE sql
STABLE
AS $$
    SELECT coalesce(current_setting('app.replica', true), '') = 'on'
$$;

COMMENT ON FUNCTION is_replica() IS
    'True on the cloud copy. Data steps in migrations skip it; the nodes upload their own result.';
