-- Auth/session access for the HTTP runtime role.
--
-- The identity schema (users, invitations, invitation_redemptions,
-- user_sessions, recovery_codes) already exists in
-- db/migrations/0002_identity_and_admin.sql; this migration adds no
-- tables. It adds the narrow SECURITY DEFINER functions the backend needs
-- and grants EXECUTE on exactly those, rather than widening table grants.
--
-- Why functions instead of table grants: 0013_security_hardening.sql
-- deliberately narrowed contracter_runtime's SELECT on `users` to a
-- column list that excludes password_hash. Login still needs the stored
-- hash (Argon2id verification happens in Rust -- PostgreSQL has no
-- Argon2id, and the private verification must not move into SQL), so
-- find_user_credential_by_login is a single purpose-built function that
-- returns a credential for one login and nothing else. The runtime role
-- gets EXECUTE on that, never a blanket read of password_hash.
--
-- Session tokens follow the same shape the schema already assumes for
-- invitations/recovery codes: the raw token exists only in the client's
-- possession, and only its SHA-256 (32-byte) hash is ever stored, so a
-- database read cannot reconstruct a usable session.

CREATE OR REPLACE FUNCTION find_user_credential_by_login(p_login text)
RETURNS TABLE (
    user_id bigint,
    user_public_id uuid,
    password_hash text,
    disabled_at timestamptz
)
LANGUAGE sql
SECURITY DEFINER
SET search_path = pg_catalog
STABLE
AS $function$
    SELECT app_user.id, app_user.public_id, app_user.password_hash, app_user.disabled_at
      FROM public.users AS app_user
     WHERE lower(app_user.login) = lower(p_login);
$function$;

-- Redeems an invitation and creates its user atomically. The caller
-- supplies an already-derived Argon2id hash; this function never sees a
-- plaintext password.
CREATE OR REPLACE FUNCTION register_invited_user(
    p_invitation_token_hash bytea,
    p_login text,
    p_password_hash text
) RETURNS uuid
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog
AS $function$
DECLARE
    v_invitation public.invitations%ROWTYPE;
    v_user_id bigint;
    v_user_public_id uuid;
BEGIN
    IF p_invitation_token_hash IS NULL OR octet_length(p_invitation_token_hash) <> 32 THEN
        RAISE EXCEPTION 'invitation token is required' USING ERRCODE = '23514';
    END IF;

    -- Locking the invitation first is what makes two concurrent
    -- redemptions of the same token resolve to exactly one winner with a
    -- clean error, rather than both racing into the UNIQUE constraint on
    -- invitation_redemptions.invitation_id.
    SELECT * INTO v_invitation
      FROM public.invitations
     WHERE token_hash = p_invitation_token_hash
     FOR UPDATE;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'invitation does not exist' USING ERRCODE = '23503';
    END IF;

    IF v_invitation.expires_at <= clock_timestamp() THEN
        RAISE EXCEPTION 'invitation has expired' USING ERRCODE = '23514';
    END IF;

    IF EXISTS (
        SELECT 1 FROM public.invitation_redemptions
         WHERE invitation_id = v_invitation.id
    ) THEN
        RAISE EXCEPTION 'invitation has already been redeemed' USING ERRCODE = '23514';
    END IF;

    INSERT INTO public.users (login, password_hash)
    VALUES (p_login, p_password_hash)
    RETURNING id, public_id INTO v_user_id, v_user_public_id;

    INSERT INTO public.invitation_redemptions (invitation_id, user_id)
    VALUES (v_invitation.id, v_user_id);

    RETURN v_user_public_id;
END;
$function$;

-- One clock reading feeds both created_at and expires_at: the table's
-- CHECK (expires_at > created_at) has no margin, and two independent
-- clock_timestamp() calls in one statement can land microseconds apart
-- (the same hazard already fixed elsewhere in this schema).
CREATE OR REPLACE FUNCTION create_user_session(
    p_user_id bigint,
    p_session_token_hash bytea,
    p_ttl interval
) RETURNS uuid
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog
AS $function$
DECLARE
    v_now timestamptz := clock_timestamp();
    v_session_public_id uuid;
BEGIN
    IF p_session_token_hash IS NULL OR octet_length(p_session_token_hash) <> 32 THEN
        RAISE EXCEPTION 'session token hash must be 32 bytes' USING ERRCODE = '23514';
    END IF;
    IF p_ttl IS NULL OR p_ttl <= interval '0' THEN
        RAISE EXCEPTION 'session lifetime must be positive' USING ERRCODE = '23514';
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM public.users
         WHERE id = p_user_id AND disabled_at IS NULL
    ) THEN
        RAISE EXCEPTION 'user is not eligible for a session' USING ERRCODE = '23514';
    END IF;

    INSERT INTO public.user_sessions (user_id, session_token_hash, created_at, expires_at)
    VALUES (p_user_id, p_session_token_hash, v_now, v_now + p_ttl)
    RETURNING public_id INTO v_session_public_id;

    RETURN v_session_public_id;
END;
$function$;

-- Resolves a presented session token to its owner. Returns no row for a
-- token that is unknown, expired, revoked, or whose user has since been
-- disabled -- the caller cannot distinguish those cases, by design.
CREATE OR REPLACE FUNCTION find_active_user_session(p_session_token_hash bytea)
RETURNS TABLE (
    session_public_id uuid,
    user_id bigint,
    user_public_id uuid,
    expires_at timestamptz
)
LANGUAGE sql
SECURITY DEFINER
SET search_path = pg_catalog
STABLE
AS $function$
    SELECT session.public_id, app_user.id, app_user.public_id, session.expires_at
      FROM public.user_sessions AS session
      JOIN public.users AS app_user ON app_user.id = session.user_id
     WHERE session.session_token_hash = p_session_token_hash
       AND session.revoked_at IS NULL
       AND session.expires_at > clock_timestamp()
       AND app_user.disabled_at IS NULL;
$function$;

-- Idempotent: revoking an already-revoked, expired, or unknown session
-- reports false rather than failing, so a repeated logout is safe.
CREATE OR REPLACE FUNCTION revoke_user_session(p_session_token_hash bytea)
RETURNS boolean
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog
AS $function$
DECLARE
    v_revoked integer;
BEGIN
    UPDATE public.user_sessions
       SET revoked_at = clock_timestamp()
     WHERE session_token_hash = p_session_token_hash
       AND revoked_at IS NULL;
    GET DIAGNOSTICS v_revoked = ROW_COUNT;
    RETURN v_revoked > 0;
END;
$function$;

-- Revokes every live session for one user (logout-everywhere, and the
-- building block a future credential change or recovery flow needs).
CREATE OR REPLACE FUNCTION revoke_all_user_sessions(p_user_id bigint)
RETURNS integer
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog
AS $function$
DECLARE
    v_revoked integer;
BEGIN
    UPDATE public.user_sessions
       SET revoked_at = clock_timestamp()
     WHERE user_id = p_user_id
       AND revoked_at IS NULL;
    GET DIAGNOSTICS v_revoked = ROW_COUNT;
    RETURN v_revoked;
END;
$function$;

-- The account view the /me endpoint needs, by public id only: the HTTP
-- layer never handles an internal sequential user id.
CREATE OR REPLACE FUNCTION find_account_by_public_id(p_user_public_id uuid)
RETURNS TABLE (
    user_public_id uuid,
    login text,
    created_at timestamptz
)
LANGUAGE sql
SECURITY DEFINER
SET search_path = pg_catalog
STABLE
AS $function$
    SELECT app_user.public_id, app_user.login, app_user.created_at
      FROM public.users AS app_user
     WHERE app_user.public_id = p_user_public_id
       AND app_user.disabled_at IS NULL;
$function$;

REVOKE ALL ON FUNCTION find_user_credential_by_login(text) FROM PUBLIC;
REVOKE ALL ON FUNCTION register_invited_user(bytea, text, text) FROM PUBLIC;
REVOKE ALL ON FUNCTION create_user_session(bigint, bytea, interval) FROM PUBLIC;
REVOKE ALL ON FUNCTION find_active_user_session(bytea) FROM PUBLIC;
REVOKE ALL ON FUNCTION revoke_user_session(bytea) FROM PUBLIC;
REVOKE ALL ON FUNCTION revoke_all_user_sessions(bigint) FROM PUBLIC;
REVOKE ALL ON FUNCTION find_account_by_public_id(uuid) FROM PUBLIC;

DO $block$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'contracter_runtime') THEN
        GRANT EXECUTE ON FUNCTION find_user_credential_by_login(text)
            TO contracter_runtime;
        GRANT EXECUTE ON FUNCTION register_invited_user(bytea, text, text)
            TO contracter_runtime;
        GRANT EXECUTE ON FUNCTION create_user_session(bigint, bytea, interval)
            TO contracter_runtime;
        GRANT EXECUTE ON FUNCTION find_active_user_session(bytea)
            TO contracter_runtime;
        GRANT EXECUTE ON FUNCTION revoke_user_session(bytea)
            TO contracter_runtime;
        GRANT EXECUTE ON FUNCTION revoke_all_user_sessions(bigint)
            TO contracter_runtime;
        GRANT EXECUTE ON FUNCTION find_account_by_public_id(uuid)
            TO contracter_runtime;
    END IF;
END;
$block$;
