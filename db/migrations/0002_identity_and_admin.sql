\set ON_ERROR_STOP on

BEGIN;

CREATE TABLE IF NOT EXISTS users (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    public_id uuid NOT NULL DEFAULT gen_random_uuid() UNIQUE,
    login text COLLATE "C" NOT NULL,
    password_hash text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    disabled_at timestamptz,
    CHECK (login = btrim(login) AND length(login) BETWEEN 3 AND 64),
    CHECK (password_hash <> ''),
    CHECK (disabled_at IS NULL OR disabled_at >= created_at)
);

CREATE UNIQUE INDEX IF NOT EXISTS users_login_key ON users (lower(login));

CREATE TABLE IF NOT EXISTS invitations (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    public_id uuid NOT NULL DEFAULT gen_random_uuid() UNIQUE,
    token_hash bytea NOT NULL UNIQUE,
    created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    expires_at timestamptz NOT NULL,
    CHECK (octet_length(token_hash) = 32),
    CHECK (expires_at > created_at)
);

CREATE TABLE IF NOT EXISTS invitation_redemptions (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    invitation_id bigint NOT NULL UNIQUE REFERENCES invitations(id),
    user_id bigint NOT NULL UNIQUE REFERENCES users(id),
    redeemed_at timestamptz NOT NULL DEFAULT clock_timestamp()
);

CREATE TABLE IF NOT EXISTS user_sessions (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    public_id uuid NOT NULL DEFAULT gen_random_uuid() UNIQUE,
    user_id bigint NOT NULL REFERENCES users(id),
    session_token_hash bytea NOT NULL UNIQUE,
    created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    expires_at timestamptz NOT NULL,
    revoked_at timestamptz,
    totp_verified_at timestamptz,
    CHECK (octet_length(session_token_hash) = 32),
    CHECK (expires_at > created_at),
    CHECK (revoked_at IS NULL OR revoked_at >= created_at)
);

CREATE INDEX IF NOT EXISTS user_sessions_active_user_idx
    ON user_sessions (user_id, expires_at) WHERE revoked_at IS NULL;

CREATE TABLE IF NOT EXISTS recovery_codes (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    public_id uuid NOT NULL DEFAULT gen_random_uuid() UNIQUE,
    user_id bigint NOT NULL REFERENCES users(id),
    token_hash bytea NOT NULL UNIQUE,
    created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    used_at timestamptz,
    CHECK (octet_length(token_hash) = 32),
    CHECK (used_at IS NULL OR used_at >= created_at)
);

CREATE TABLE IF NOT EXISTS administrators (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    public_id uuid NOT NULL DEFAULT gen_random_uuid() UNIQUE,
    user_id bigint NOT NULL UNIQUE REFERENCES users(id),
    is_active boolean NOT NULL DEFAULT true,
    totp_secret_hash bytea,
    created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    deactivated_at timestamptz,
    CHECK (totp_secret_hash IS NULL OR octet_length(totp_secret_hash) = 32),
    CHECK ((is_active AND deactivated_at IS NULL) OR
           (NOT is_active AND deactivated_at IS NOT NULL))
);

CREATE TABLE IF NOT EXISTS critical_actions (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    public_id uuid NOT NULL DEFAULT gen_random_uuid() UNIQUE,
    action_type_code text NOT NULL REFERENCES critical_action_types(code),
    proposer_admin_id bigint NOT NULL REFERENCES administrators(id),
    payload jsonb NOT NULL,
    payload_hash bytea NOT NULL,
    requested_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    expires_at timestamptz NOT NULL,
    CHECK (jsonb_typeof(payload) = 'object'),
    CHECK (octet_length(payload_hash) = 32),
    CHECK (payload_hash = public.digest(convert_to(payload::text, 'UTF8'), 'sha256')),
    CHECK (expires_at > requested_at)
);

CREATE INDEX IF NOT EXISTS critical_actions_pending_idx
    ON critical_actions (expires_at, action_type_code);

CREATE TABLE IF NOT EXISTS critical_action_approval_events (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    public_id uuid NOT NULL DEFAULT gen_random_uuid() UNIQUE,
    critical_action_id bigint NOT NULL UNIQUE REFERENCES critical_actions(id),
    approver_admin_id bigint NOT NULL REFERENCES administrators(id),
    payload_hash bytea NOT NULL,
    approved_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    CHECK (octet_length(payload_hash) = 32)
);

CREATE TABLE IF NOT EXISTS critical_action_execution_events (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    public_id uuid NOT NULL DEFAULT gen_random_uuid() UNIQUE,
    critical_action_id bigint NOT NULL UNIQUE REFERENCES critical_actions(id),
    execution_key uuid NOT NULL UNIQUE,
    payload_hash bytea NOT NULL,
    executed_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    result_reference text,
    CHECK (octet_length(payload_hash) = 32)
);

CREATE TABLE IF NOT EXISTS admin_recovery_events (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    public_id uuid NOT NULL DEFAULT gen_random_uuid() UNIQUE,
    recovered_admin_id bigint NOT NULL REFERENCES administrators(id),
    surviving_admin_id bigint NOT NULL REFERENCES administrators(id),
    custodian_reference text NOT NULL,
    external_audit_reference text NOT NULL,
    occurred_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    freeze_until timestamptz NOT NULL,
    CHECK (recovered_admin_id <> surviving_admin_id),
    CHECK (freeze_until >= occurred_at + interval '24 hours')
);

CREATE TABLE IF NOT EXISTS admin_recovery_shares (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    administrator_id bigint REFERENCES administrators(id),
    custodian_reference text,
    share_hash bytea NOT NULL UNIQUE,
    created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    revoked_at timestamptz,
    CHECK ((administrator_id IS NOT NULL)::integer +
           (custodian_reference IS NOT NULL)::integer = 1),
    CHECK (octet_length(share_hash) = 32)
);

CREATE OR REPLACE FUNCTION reject_critical_action_payload_change()
RETURNS trigger
LANGUAGE plpgsql
SET search_path = pg_catalog
AS $function$
BEGIN
    RAISE EXCEPTION 'critical action payload and hash are immutable'
        USING ERRCODE = '55000';
END;
$function$;

DROP TRIGGER IF EXISTS critical_actions_payload_immutable ON critical_actions;
CREATE TRIGGER critical_actions_payload_immutable
BEFORE UPDATE OF payload, payload_hash, action_type_code, proposer_admin_id,
                 requested_at, expires_at
ON critical_actions
FOR EACH STATEMENT
EXECUTE FUNCTION reject_critical_action_payload_change();

CREATE OR REPLACE FUNCTION validate_critical_action_approval()
RETURNS trigger
LANGUAGE plpgsql
SET search_path = pg_catalog
AS $function$
DECLARE
    action_row public.critical_actions%ROWTYPE;
    approver_active boolean;
BEGIN
    SELECT * INTO action_row
      FROM public.critical_actions
     WHERE id = NEW.critical_action_id
     FOR UPDATE;

    IF NOT FOUND THEN
        RAISE EXCEPTION 'critical action does not exist' USING ERRCODE = '23503';
    END IF;

    SELECT is_active INTO approver_active
      FROM public.administrators
     WHERE id = NEW.approver_admin_id
     FOR SHARE;

    IF approver_active IS DISTINCT FROM true THEN
        RAISE EXCEPTION 'approver must be an active administrator'
            USING ERRCODE = '23514';
    END IF;

    IF action_row.proposer_admin_id = NEW.approver_admin_id THEN
        RAISE EXCEPTION 'proposer cannot approve their own critical action'
            USING ERRCODE = '23514';
    END IF;

    IF NEW.payload_hash <> action_row.payload_hash THEN
        RAISE EXCEPTION 'approval payload hash does not match proposal'
            USING ERRCODE = '23514';
    END IF;

    IF NEW.approved_at < action_row.requested_at OR
       NEW.approved_at > action_row.expires_at OR
       NEW.approved_at > action_row.requested_at + interval '24 hours' THEN
        RAISE EXCEPTION 'critical-action approval window has expired'
            USING ERRCODE = '23514';
    END IF;

    IF EXISTS (
        SELECT 1 FROM public.admin_recovery_events AS recovery
         WHERE recovery.freeze_until > NEW.approved_at
    ) THEN
        RAISE EXCEPTION 'critical administration is frozen after recovery'
            USING ERRCODE = '23514';
    END IF;

    RETURN NEW;
END;
$function$;

DROP TRIGGER IF EXISTS critical_action_approval_validate
    ON critical_action_approval_events;
CREATE TRIGGER critical_action_approval_validate
BEFORE INSERT ON critical_action_approval_events
FOR EACH ROW
EXECUTE FUNCTION validate_critical_action_approval();

COMMIT;
