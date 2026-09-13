#!/bin/sh
set -eu

: "${TEST_DATABASE_URL:?TEST_DATABASE_URL is required}"

repo_dir=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
migration_dir="$repo_dir/db/migrations"
test_file="$repo_dir/db/tests/001_invariants.sql"
migration_found=0

# POSIX glob expansion is sorted according to the active locale.  Pinning the
# locale makes the numbered migration order deterministic across machines.
LC_ALL=C
export LC_ALL

for migration in "$migration_dir"/*.sql; do
    if [ ! -f "$migration" ]; then
        continue
    fi

    migration_found=1
    psql -X --dbname="$TEST_DATABASE_URL" --set=ON_ERROR_STOP=1 --file="$migration"
done

if [ "$migration_found" -ne 1 ]; then
    echo "No SQL migrations found in $migration_dir" >&2
    exit 1
fi

psql -X --dbname="$TEST_DATABASE_URL" --set=ON_ERROR_STOP=1 --file="$test_file"
