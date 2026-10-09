#!/usr/bin/env bash
# Regenerates the sqlx offline query cache (.sqlx/ at the workspace root).
#
# Run this after touching any `query!`/`query_as!` invocation or a migration
# in crates/taskboard-storage-sqlite, and commit the refreshed .sqlx/
# together with the change (never separately). CI runs
# `cargo sqlx prepare --check` against the committed cache, so a PR that
# forgot this step fails there.
#
# The sqlx-cli major/minor MUST match the sqlx crate (0.9); other versions
# write an incompatible offline data format:
#   cargo install sqlx-cli --version 0.9 --no-default-features --features sqlite
set -euo pipefail
cd "$(dirname "$0")/.."

CHECK=0
[ "${1:-}" = "--check" ] && CHECK=1

DB_URL="sqlite://.sqlx-prepare.db?mode=rwc"
MIGRATIONS="crates/taskboard-storage-sqlite/migrations"

cleanup() { rm -f .sqlx-prepare.db .sqlx-prepare.db-wal .sqlx-prepare.db-shm; }
trap cleanup EXIT
cleanup

sqlx database create --database-url "$DB_URL"
sqlx migrate run --source "$MIGRATIONS" --database-url "$DB_URL"

if [ "$CHECK" -eq 1 ]; then
    # Fails when a query!/query_as! would need new offline data that the
    # committed .sqlx/ does not carry.
    DATABASE_URL="$DB_URL" SQLX_OFFLINE=false cargo sqlx prepare --check --workspace -- --all-targets
    echo "Offline cache is fresh."
else
    DATABASE_URL="$DB_URL" cargo sqlx prepare --workspace -- --all-targets
    echo "Offline cache refreshed; commit .sqlx/ with your change."
fi
