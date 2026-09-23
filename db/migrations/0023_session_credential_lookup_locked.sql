-- Close a TOCTOU between reading a credential and minting its session.
--
-- `create_user_session_for_credential` (0022) read the account's id with a
-- plain SELECT, then inserted a session using that cached id
-- unconditionally. Nothing locked the row in between, so a concurrent
-- `UPDATE users SET disabled_at = ...` -- an admin disabling a compromised
-- or banned account -- could commit inside that window, and the login
-- already in flight would still mint a valid session for the now-disabled
-- account. Confirmed directly: begin a transaction, run the function's
-- exact lookup, disable the account from a second connection and commit
-- it, then run the function's exact insert from the first transaction --
-- it succeeds and the disabled account ends up with a live session.
--
-- `SELECT ... FOR UPDATE` closes it under PostgreSQL's default READ
-- COMMITTED isolation via EvalPlanQual: if a concurrent transaction
-- commits a change to the row while this one is waiting on the lock,
-- PostgreSQL re-evaluates the WHERE clause against the newly committed
-- version before returning it. A row disabled in that window no longer
-- satisfies `disabled_at IS NULL` and is correctly not returned, so
-- `v_user_id` stays NULL and the function raises the same "the credential
-- does not identify an enabled account" it already raises for an unknown
-- login or a stale hash.
--
-- This is not a stronger guarantee than "a login serializes against a
-- disable somehow" -- if the disable commits strictly before this
-- function acquires the row lock, the login is correctly refused; if the
-- login's lock is acquired first, the session it mints is valid and the
-- disable simply takes effect afterward. That is the same ordering a
-- lock-free system would get by luck some of the time and not others;
-- this makes it deterministic instead.
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
       AND app_user.disabled_at IS NULL
       FOR UPDATE;

    IF v_user_id IS NULL THEN
        -- One outcome for an unknown login, a stale hash, and a disabled
        -- account alike -- including one disabled in the instant between
        -- this transaction starting and this lock being acquired. A
        -- caller that reaches this has already failed; telling it which
        -- of these happened would turn a bound session-creation call into
        -- the oracle 0019 removed.
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

DO $block$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'contracter_definer') THEN
        ALTER FUNCTION create_user_session_for_credential(text, text, bytea, interval)
            OWNER TO contracter_definer;
    END IF;
END;
$block$;
