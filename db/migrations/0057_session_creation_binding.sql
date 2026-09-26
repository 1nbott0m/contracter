-- Session creation must name the credential it is acting on, not an
-- arbitrary internal id.
--
-- `create_user_session(p_user_id bigint, ...)` is granted to
-- `contracter_runtime` and trusts whatever id it is handed. That is the
-- pattern migration 0013 declared unacceptable for `finalize_contract`
-- and replaced with an identity-checked wrapper, and it was skipped here
-- for the two functions where it matters most: an independent security
-- review demonstrated, as `contracter_runtime`, minting a valid session
-- for another account by id alone and resolving it back to that account.
--
-- Not reachable over HTTP -- `auth::login` passes an id it has just
-- derived from a verified credential -- so this is a broken boundary
-- rather than live data loss. But it means any SQL injection anywhere in
-- the runtime path, or any leak of the runtime credential, converts
-- directly into "mint a session for any account", which is full takeover
-- with no password involved.
--
-- The fix binds the call to the stored credential. Argon2id verification
-- cannot happen in PostgreSQL and is not being moved there: the caller
-- still verifies the password itself. What changes is that it must now
-- also present the password hash it read, and the function refuses unless
-- that matches what is stored for that login. An attacker holding only
-- the runtime credential cannot supply it, because 0019 removed their
-- ability to enumerate logins and the hash is reachable only one login at
-- a time through a function that requires knowing the login already.
--
-- The comparison is a plain equality on a stored PHC string, not a
-- secret-dependent branch on attacker-supplied data: the value is already
-- in the database, and a caller that can present it has already read it.
CREATE OR REPLACE FUNCTION create_user_session_for_credential(
    p_login text,
    p_password_hash text,
    p_session_token_hash bytea,
    p_ttl interval
) RETURNS uuid
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, pg_temp
AS $function$
DECLARE
    v_user_id bigint;
    v_now timestamptz;
    v_public_id uuid;
BEGIN
    IF p_login IS NULL OR p_password_hash IS NULL
       OR p_session_token_hash IS NULL OR p_ttl IS NULL THEN
        RAISE EXCEPTION 'session creation requires a login, its stored hash, a token hash and a ttl'
            USING ERRCODE = '23514';
    END IF;

    SELECT app_user.id
      INTO v_user_id
      FROM public.users AS app_user
     WHERE lower(app_user.login) = lower(p_login)
       AND app_user.password_hash = p_password_hash
       AND app_user.disabled_at IS NULL;

    IF v_user_id IS NULL THEN
        -- One outcome for an unknown login, a stale hash, and a disabled
        -- account alike. A caller that reaches this has already failed;
        -- telling it which of the three happened would turn a bound
        -- session-creation call into the oracle this migration removes.
        RAISE EXCEPTION 'the credential does not identify an enabled account'
            USING ERRCODE = '42501';
    END IF;

    -- One clock reading feeds both columns. `user_sessions` carries
    -- `CHECK (expires_at > created_at)` with no margin, and
    -- `clock_timestamp()` is volatile, so two readings in one statement
    -- can land microseconds apart and violate it.
    v_now := clock_timestamp();

    INSERT INTO public.user_sessions (user_id, session_token_hash, created_at, expires_at)
    VALUES (v_user_id, p_session_token_hash, v_now, v_now + p_ttl)
    RETURNING public_id INTO v_public_id;

    RETURN v_public_id;
END;
$function$;

REVOKE ALL ON FUNCTION
    create_user_session_for_credential(text, text, bytea, interval)
    FROM PUBLIC;

DO $block$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'contracter_runtime') THEN
        GRANT EXECUTE ON FUNCTION
            create_user_session_for_credential(text, text, bytea, interval)
            TO contracter_runtime;
        -- The unbound version stops being reachable by the runtime role.
        -- It stays defined, because administrative and recovery paths in
        -- later milestones legitimately create sessions without a password
        -- in hand; those run as an admin role, not this one.
        REVOKE EXECUTE ON FUNCTION create_user_session(bigint, bytea, interval)
            FROM contracter_runtime;
    END IF;

    IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'contracter_definer') THEN
        -- ALTER FUNCTION OWNER needs the new owner to hold CREATE on the
        -- schema for a non-superuser migrator; granted only for this step.
        GRANT CREATE ON SCHEMA public TO contracter_definer;
        ALTER FUNCTION create_user_session_for_credential(text, text, bytea, interval)
            OWNER TO contracter_definer;
        REVOKE CREATE ON SCHEMA public FROM contracter_definer;
    END IF;
END;
$block$;
