# Production reference deployment

Only Caddy publishes host ports. The application, PostgreSQL and Prometheus are
reachable only on the internal Compose network. Copy the example env files,
create `secrets/` files with restrictive permissions, and review the domain
before starting. Never commit real secrets or env files.

Required secret files are `database_url`, `quote_signing_key`,
`quote_seed_key`, and `postgres_owner_password`; create them with mode `0600`.
The application database URL must point at the restricted runtime role, not
the schema owner. Run migrations separately with an administrative role.

## Backup and restore drill

Backups use PostgreSQL custom format and are written with mode `0600`:

```bash
DATABASE_URL='postgres://admin@nonprod-db/contracter' \
BACKUP_DIR='./backups' ./deploy/backup.sh
```

Restore is fail-closed and must target a disposable non-production database:

```bash
DATABASE_URL='postgres://admin@nonprod-db/contracter_restore' \
BACKUP_FILE='./backups/contracter-<timestamp>.dump' \
ALLOW_RESTORE=YES_RESTORE_NONPROD ./deploy/restore.sh
```

After restore, run `./db/verify.sh` and the integration suite against the
restored database. Keep at least two dated backups and complete this drill
before changing production migrations. The scripts refuse ambiguous
production-like targets and never print database credentials.

## Rollback and incident procedure

1. Stop the rollout and keep the previous image/reference available.
2. Check `/health/live`, `/health/ready`, request IDs, and PostgreSQL health.
3. Roll back the application image first. Do not reverse an applied destructive
   migration; use a forward-compatible repair migration.
4. If data integrity is in doubt, disable market publication and preserve the
   append-only audit/ledger evidence before restoring a disposable copy.
5. Record the incident, affected commit, migration state, backup identifier, and
   verification results before reopening traffic.
