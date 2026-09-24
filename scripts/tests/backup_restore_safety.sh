#!/usr/bin/env sh
set -eu
if DATABASE_URL=postgresql://localhost/contracter BACKUP_DIR="$(mktemp -d)" ./deploy/backup.sh >/dev/null 2>&1; then exit 1; fi
if DATABASE_URL=postgresql://localhost/contracter BACKUP_FILE=/tmp/missing.dump ./deploy/restore.sh >/dev/null 2>&1; then exit 1; fi
if DATABASE_URL=postgresql://remote/contracter BACKUP_FILE=/tmp/missing.dump ALLOW_RESTORE=YES_RESTORE_NONPROD ./deploy/restore.sh >/dev/null 2>&1; then exit 1; fi
echo 'backup/restore safety checks: ok'
