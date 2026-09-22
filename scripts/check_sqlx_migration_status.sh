#!/usr/bin/env bash
set -euo pipefail

: "${DATABASE_URL:?DATABASE_URL is required}"

# Deliberately print only migration metadata, never the connection string.
psql -X --dbname="$DATABASE_URL" --set=ON_ERROR_STOP=1 <<'SQL'
SELECT version, success, installed_on
  FROM _sqlx_migrations
 ORDER BY version;
SQL
