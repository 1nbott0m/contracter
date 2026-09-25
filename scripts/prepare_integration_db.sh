#!/bin/sh
set -eu

: "${TEST_DATABASE_URL:?TEST_DATABASE_URL is required}"
repo_dir=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
export LC_ALL=C

if ! psql -XAt --dbname="$TEST_DATABASE_URL" --command="SELECT to_regclass('public._sqlx_migrations') IS NULL" | grep -qx t; then
    sqlx_managed=1
    echo "integration database already has SQLx migration history; refusing to replay raw migrations" >&2
else
    sqlx_managed=0
    for migration in "$repo_dir"/db/migrations/*.sql; do
        psql -X --dbname="$TEST_DATABASE_URL" --set=ON_ERROR_STOP=1 \
            --single-transaction --command='SET ROLE anonymous' --file="$migration"
    done
fi
for seed in "$repo_dir"/db/seeds/*.sql; do
    if [ "$sqlx_managed" -eq 1 ]; then
        psql -X --dbname="$TEST_DATABASE_URL" --set=ON_ERROR_STOP=1 \
            --single-transaction --file="$seed"
    else
        psql -X --dbname="$TEST_DATABASE_URL" --set=ON_ERROR_STOP=1 \
            --single-transaction --command='SET ROLE anonymous' --file="$seed"
    fi
done

# Keep reference catalog/risk data, but remove every mutable inventory/quote
# fixture: the ignored suites create their own deterministic state.  CASCADE
# is intentional here because quote reservations and transfer events reference
# inventory rows; leaving any of those rows behind makes tests depend on order.
if [ "$sqlx_managed" -eq 1 ]; then
    psql -X --dbname="$TEST_DATABASE_URL" --set=ON_ERROR_STOP=1 \
        --command='TRUNCATE TABLE
            inventory_transfer_events,
            inventory_item_locks,
            quote_candidate_reservations,
            quote_outcomes,
            quote_inputs,
            tradeup_quotes,
            price_halts,
            inventory_positions,
            inventory_items,
            warehouse_stock
            RESTART IDENTITY CASCADE;'
else
    psql -X --dbname="$TEST_DATABASE_URL" --set=ON_ERROR_STOP=1 \
        --command='SET ROLE anonymous' \
        --command='TRUNCATE TABLE
        inventory_transfer_events,
        inventory_item_locks,
        quote_candidate_reservations,
        quote_outcomes,
        quote_inputs,
        tradeup_quotes,
        price_halts,
        inventory_positions,
        inventory_items,
        warehouse_stock
        RESTART IDENTITY CASCADE;'
fi
