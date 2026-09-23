#!/bin/sh
set -eu

repo_dir=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$repo_dir"

cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
sh -n db/verify.sh

if [ -n "${TEST_DATABASE_URL:-}" ]; then
    if ! command -v psql >/dev/null 2>&1; then
        if [ -x /opt/homebrew/opt/libpq/bin/psql ]; then
            PATH="/opt/homebrew/opt/libpq/bin:$PATH"
            export PATH
        else
            echo "TEST_DATABASE_URL is set, but psql is not available" >&2
            exit 1
        fi
    fi
    ./db/verify.sh
    # Whole workspace, not just `db`: the api/application suites carry the
    # HTTP and use-case integration tests, and they must run after
    # db/verify.sh, which expects a pristine database (the Rust suites
    # commit fixtures on purpose).
    # db/verify.sh seeds reference data and deliberately leaves it committed
    # for SQL assertions. Rust integration tests use rollback fixtures and
    # must run against a separate pristine database.
    if [ -n "${TEST_DATABASE_URL_INTEGRATION:-}" ]; then
        TEST_DATABASE_URL="$TEST_DATABASE_URL_INTEGRATION" \
            cargo test --workspace --tests -- --ignored
    else
        cargo test --workspace --tests -- --ignored
    fi
else
    echo "PostgreSQL integration skipped: TEST_DATABASE_URL is not set"
fi
