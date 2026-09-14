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
    cargo test -p db --test postgres -- --ignored
else
    echo "PostgreSQL integration skipped: TEST_DATABASE_URL is not set"
fi
