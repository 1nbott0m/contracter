#!/usr/bin/env sh
set -eu
: "${BACKUP_DIR:=./backups}"
: "${DATABASE_URL:?DATABASE_URL is required}"
case "$DATABASE_URL" in *localhost*|*127.0.0.1*|*production*) echo 'refusing ambiguous production-like target' >&2; exit 2;; esac
mkdir -p "$BACKUP_DIR"
umask 077
file="$BACKUP_DIR/contracter-$(date -u +%Y%m%dT%H%M%SZ).dump"
pg_dump --format=custom --no-owner --file "$file" "$DATABASE_URL"
printf '%s\n' "$file"
