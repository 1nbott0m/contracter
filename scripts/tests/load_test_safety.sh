#!/usr/bin/env sh
set -eu
if BASE_URL=http://localhost:8080 ./scripts/run_load_test.sh smoke >/dev/null 2>&1; then exit 1; fi
if BASE_URL=https://example.invalid ./scripts/run_load_test.sh smoke >/dev/null 2>&1; then exit 1; fi
echo 'load-test safety checks: ok'
