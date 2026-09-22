#!/usr/bin/env bash
# Drops and rebuilds the development database from db/migrations.
#
# Each migration runs in a single transaction, so a failure leaves the database
# untouched rather than half-migrated. Never point this at anything but a
# development database.
set -euo pipefail

DB="${GIAMS_DB:-giams_dev}"
PGUSER_="${PGUSER:-postgres}"
PGHOST_="${PGHOST:-localhost}"
MIGRATIONS="$(cd "$(dirname "$0")/../db/migrations" && pwd)"

if [[ "$DB" != *dev* && "$DB" != *test* ]]; then
    echo "refusing to reset '$DB' -- name must contain 'dev' or 'test'" >&2
    exit 1
fi

echo "resetting $DB"
psql -U "$PGUSER_" -h "$PGHOST_" -q \
     -c "DROP DATABASE IF EXISTS $DB WITH (FORCE);" \
     -c "CREATE DATABASE $DB;" 2>&1 | grep -v '^NOTICE' || true

for f in "$MIGRATIONS"/*.sql; do
    name="$(basename "$f")"
    if psql -U "$PGUSER_" -h "$PGHOST_" -d "$DB" \
            --single-transaction -v ON_ERROR_STOP=1 -q -o /dev/null -f "$f"; then
        echo "  ok   $name"
    else
        echo "  FAIL $name" >&2
        exit 1
    fi
done

echo "done"
