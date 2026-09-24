#!/usr/bin/env sh
set -eu
: "${RUNTIME_DATABASE_URL:?RUNTIME_DATABASE_URL is required}"
role_flags=$(psql -XAt --dbname="$RUNTIME_DATABASE_URL" -c "SELECT rolsuper OR rolbypassrls OR rolcreaterole OR rolcreatedb FROM pg_roles WHERE rolname=current_user")
[ "$role_flags" = f ] || { echo 'runtime role is overprivileged' >&2; exit 1; }
owner=$(psql -XAt --dbname="$RUNTIME_DATABASE_URL" -c "SELECT nspowner=(SELECT oid FROM pg_roles WHERE rolname=current_user) FROM pg_namespace WHERE nspname='public'")
[ "$owner" = f ] || { echo 'runtime role owns public schema' >&2; exit 1; }
echo 'runtime role checks: ok'
