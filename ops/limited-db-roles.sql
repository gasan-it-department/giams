-- ═══════════════════════════════════════════════════════════════════════════
--  Limited database accounts for an installation
-- ═══════════════════════════════════════════════════════════════════════════
--  Run once per installation, as the postgres superuser, connected to the
--  GIAMS database:
--
--      psql -U postgres -d giams -v app_pw="'...'" -v backup_pw="'...'" -f ops/limited-db-roles.sql
--
--  giams_app     what giams-node signs in as (DATABASE_URL). Owns the schema
--                so it can apply its own migrations, but is not a superuser
--                and does not bypass row level security: every table is
--                FORCE RLS, so even as owner it sees one municipality at a time.
--  giams_backup  read-only, bypasses RLS, used only by `giams-node backup`
--                (GIAMS_BACKUP_DATABASE_URL). pg_dump refuses to dump a table
--                RLS would filter, and a backup must hold every row.
--
--  Safe to run again: existing roles get the new passwords, ownership is
--  re-asserted.
-- ═══════════════════════════════════════════════════════════════════════════

\set ON_ERROR_STOP on

SELECT set_config('giams.app_pw', :app_pw, false), set_config('giams.backup_pw', :backup_pw, false) \gset

DO $do$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'giams_app') THEN
        CREATE ROLE giams_app LOGIN;
    END IF;
    EXECUTE format('ALTER ROLE giams_app LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOBYPASSRLS NOREPLICATION PASSWORD %L',
                   current_setting('giams.app_pw'));

    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'giams_backup') THEN
        CREATE ROLE giams_backup LOGIN;
    END IF;
    EXECUTE format('ALTER ROLE giams_backup LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE BYPASSRLS NOREPLICATION PASSWORD %L',
                   current_setting('giams.backup_pw'));
END
$do$;

SELECT set_config('giams.app_pw', '', false), set_config('giams.backup_pw', '', false) \gset

-- ── giams_app owns the database's own objects ─────────────────────────────
DO $do$
DECLARE
    r record;
BEGIN
    EXECUTE format('ALTER DATABASE %I OWNER TO giams_app', current_database());
    ALTER SCHEMA public OWNER TO giams_app;

    -- tables, views, sequences (sequences owned by a column follow their table)
    FOR r IN
        SELECT c.oid::regclass AS obj, c.relkind
          FROM pg_class c
         WHERE c.relnamespace = 'public'::regnamespace
           AND c.relkind IN ('r', 'p', 'v', 'm', 'S', 'f')
           AND NOT EXISTS (SELECT 1 FROM pg_depend d
                            WHERE d.objid = c.oid AND d.deptype IN ('e', 'a', 'i'))
    LOOP
        EXECUTE format('ALTER %s %s OWNER TO giams_app',
                       CASE r.relkind WHEN 'v' THEN 'VIEW' WHEN 'm' THEN 'MATERIALIZED VIEW'
                                      WHEN 'S' THEN 'SEQUENCE' WHEN 'f' THEN 'FOREIGN TABLE'
                                      ELSE 'TABLE' END,
                       r.obj);
    END LOOP;

    -- functions and procedures, except those that belong to an extension
    FOR r IN
        SELECT p.oid::regprocedure AS obj, p.prokind
          FROM pg_proc p
         WHERE p.pronamespace = 'public'::regnamespace
           AND NOT EXISTS (SELECT 1 FROM pg_depend d WHERE d.objid = p.oid AND d.deptype = 'e')
    LOOP
        EXECUTE format('ALTER %s %s OWNER TO giams_app',
                       CASE r.prokind WHEN 'p' THEN 'PROCEDURE' WHEN 'a' THEN 'AGGREGATE' ELSE 'FUNCTION' END,
                       r.obj);
    END LOOP;

    -- enums and domains (array types follow their element type)
    FOR r IN
        SELECT t.oid::regtype AS obj, t.typtype
          FROM pg_type t
         WHERE t.typnamespace = 'public'::regnamespace
           AND t.typtype IN ('e', 'd')
           AND NOT EXISTS (SELECT 1 FROM pg_depend d WHERE d.objid = t.oid AND d.deptype = 'e')
    LOOP
        EXECUTE format('ALTER %s %s OWNER TO giams_app',
                       CASE r.typtype WHEN 'd' THEN 'DOMAIN' ELSE 'TYPE' END, r.obj);
    END LOOP;
END
$do$;

-- ── giams_backup reads everything, writes nothing ─────────────────────────
GRANT pg_read_all_data TO giams_backup;
SELECT format('GRANT CONNECT ON DATABASE %I TO giams_backup', current_database()) \gexec

-- what came out of it
SELECT rolname, rolsuper, rolbypassrls, rolcreaterole, rolcreatedb
  FROM pg_roles WHERE rolname IN ('giams_app', 'giams_backup') ORDER BY 1;
SELECT count(*) AS objects_still_owned_by_postgres
  FROM pg_class c
 WHERE c.relnamespace = 'public'::regnamespace
   AND c.relowner = 'postgres'::regrole;
