#!/bin/sh
set -eu

: "${TEST_DATABASE_URL:?TEST_DATABASE_URL is required}"

repo_dir=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
migration_dir="$repo_dir/db/migrations"
test_dir="$repo_dir/db/tests"
seed_dir="$repo_dir/db/seeds"
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
    psql -X --dbname="$TEST_DATABASE_URL" --set=ON_ERROR_STOP=1 \
        --single-transaction --file="$migration"
done

if [ "$migration_found" -ne 1 ]; then
    echo "No SQL migrations found in $migration_dir" >&2
    exit 1
fi

for seed in "$seed_dir"/*.sql; do
    if [ -f "$seed" ]; then
        psql -X --dbname="$TEST_DATABASE_URL" --set=ON_ERROR_STOP=1 \
            --single-transaction --file="$seed"
    fi
done

for test_file in "$test_dir"/*.sql; do
    if [ -f "$test_file" ]; then
        # TEST_RUNTIME_ROLE_REQUIRED reaches the SQL suite too, so a missing
        # runtime role fails the run rather than printing a warning a green
        # run can ignore.
        psql -X --dbname="$TEST_DATABASE_URL" --set=ON_ERROR_STOP=1 \
            --set=runtime_role_required="${TEST_RUNTIME_ROLE_REQUIRED:-}" \
            --file="$test_file"
    fi
done
