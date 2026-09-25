#!/usr/bin/env sh
set -eu
: "${BASE_URL:?BASE_URL is required}"
case "$BASE_URL" in https://*) ;; *) echo 'BASE_URL must use HTTPS' >&2; exit 2;; esac
case "$BASE_URL" in *localhost*|*127.0.0.1*|*\[::1\]*) ;; *) [ "${ALLOW_REMOTE_LOAD:=}" = "YES_LOAD_NONPROD" ] || { echo 'remote load requires ALLOW_REMOTE_LOAD=YES_LOAD_NONPROD' >&2; exit 2; };; esac
scenario=${1:-smoke}
case "$scenario" in smoke) script=load/k6/smoke.js;; *) echo 'unsupported scenario' >&2; exit 2;; esac
command -v k6 >/dev/null 2>&1 || { echo 'NOT VERIFIED: k6 is unavailable'; exit 3; }
exec k6 run --vus "${VUS:-1}" --duration "${DURATION:-10s}" "$script"
