-- Close the composed path to every password hash.
--
-- Migration 0013 deliberately narrowed contracter_runtime's SELECT on
-- `users` to a column list that excludes `password_hash`, and its own
-- header calls the unnarrowed state "Critical". Migration 0015 then
-- granted that same role EXECUTE on `find_user_credential_by_login`, a
-- SECURITY DEFINER function that returns one login's hash so that login
-- can be verified -- which Argon2id cannot do inside PostgreSQL.
--
-- Separately each grant is defensible. Composed they are not: the role
-- can still read `users.login`, so it can drive the function over every
-- row at once. An independent security review demonstrated it:
--
--   SET LOCAL ROLE contracter_runtime;
--   SELECT u.login, c.password_hash
--     FROM users u
--     CROSS JOIN LATERAL find_user_credential_by_login(u.login) c;
--
-- returned every Argon2id PHC string in the database. Not reachable over
-- HTTP as the code stands -- only `auth::login` calls the function, with
-- a single login it was given -- but the boundary 0013 claims to have
-- established did not exist.
--
-- The fix is to remove the enumeration key rather than the verification
-- path. Nothing in `crates/db` selects `users.login` directly: `/me`
-- reads through `find_account_by_public_id`, which is SECURITY DEFINER
-- and returns one account by its public UUID. So the column grant buys
-- the runtime role nothing except the ability to enumerate.
DO $block$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'contracter_runtime') THEN
        REVOKE SELECT (login) ON users FROM contracter_runtime;
    END IF;
END;
$block$;
