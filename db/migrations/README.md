# Migrations

Applied in order by `giams-node` and `giams-cloud` at startup. Never edit one
that has run anywhere: the recorded checksum stops the server. Fix forward
with a new file.

## Data steps skip the cloud

Both the office servers and the cloud copy run these files. Anything that
writes rows (seeding, back-filling, renaming) must not run on the cloud, or
the cloud gets its own copies with their own ids and the office's uploads are
then refused as duplicates. That happened with 0035.

```sql
DO $do$
BEGIN
    IF is_replica() THEN RETURN; END IF;
    -- the data step
END
$do$;
```

Schema changes (tables, columns, functions, constraints) run everywhere.

## Run as postgres the first time

0001 installs PostgreSQL extensions, which needs a superuser. After that the
server runs as `giams_app` (`ops/limited-db-roles.sql`); re-run that script
after `ops/reset-dev-db.sh` or a restore, which leave `postgres` as owner.
