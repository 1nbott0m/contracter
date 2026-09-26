-- SECURITY DEFINER functions added after 0056 (Steam login, administrator
-- TOTP, admin audit and history readers) were created with
-- `SET search_path = public`. PostgreSQL searches the session's temporary
-- schema first for relations unless `pg_temp` is named explicitly, so any
-- caller with the default TEMP privilege could create `pg_temp.users`,
-- GRANT SELECT on it to the function owner, and have the function read the
-- caller's rows instead of the real ones. Reproduced as contracter_runtime:
-- find_user_by_steam_id('attacker-steam') returned a forged user_id.
--
-- Pin the search path for every definer function that is not already in
-- the safe form (pg_catalog first, pg_temp last), keeping `public` because these bodies use unqualified
-- names. pg_catalog first and pg_temp last is the documented safe order.
-- Data-driven, so functions added between 0056 and this migration are
-- covered, and idempotent so a re-run changes nothing.
DO $block$
DECLARE
    target record;
BEGIN
    FOR target IN
        SELECT p.oid::regprocedure AS signature
        FROM pg_proc AS p
        JOIN pg_namespace AS n ON n.oid = p.pronamespace
        WHERE n.nspname = 'public'
          AND p.prosecdef
          AND NOT EXISTS (
              SELECT 1
              FROM unnest(COALESCE(p.proconfig, ARRAY[]::text[])) AS setting
              WHERE setting ~ '^search_path=pg_catalog(, public)?, pg_temp$'
          )
    LOOP
        EXECUTE format(
            'ALTER FUNCTION %s SET search_path = pg_catalog, public, pg_temp',
            target.signature
        );
    END LOOP;
END;
$block$;
