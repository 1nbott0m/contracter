#!/usr/bin/env sh
set -eu
: "${BACKUP_FILE:?BACKUP_FILE is required}"
: "${DATABASE_URL:?DATABASE_URL is required}"
: "${ALLOW_RESTORE:=}"
[ "$ALLOW_RESTORE" = "YES_RESTORE_NONPROD" ] || { echo 'set ALLOW_RESTORE=YES_RESTORE_NONPROD for a non-production restore' >&2; exit 2; }
test -f "$BACKUP_FILE" || { echo 'backup file not found' >&2; exit 2; }
case "$DATABASE_URL" in *localhost*|*127.0.0.1*|*production*) echo 'refusing production-like target' >&2; exit 2;; esac
pg_restore --clean --if-exists --no-owner --dbname "$DATABASE_URL" "$BACKUP_FILE"
