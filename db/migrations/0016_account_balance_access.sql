-- Ledger-derived balance for the authenticated account's own /me/balance.
--
-- contracter_runtime has no SELECT on ledger_accounts or ledger_balances
-- (0005 granted those to contracter_admin_runtime only), which is correct:
-- a player-facing role must not be able to read the whole ledger. This
-- function is the one narrow exception, and it is keyed by user public id
-- so the HTTP layer can only ever ask for the caller's own balance -- the
-- session decides whose id that is, never the client.
--
-- A newly registered user has no ledger account yet (nothing creates one
-- at registration; accounts appear when the first economic operation
-- does). That is reported as a balance of 0, not as a missing resource.
-- An unknown or disabled user yields no row at all.
CREATE OR REPLACE FUNCTION find_user_credit_balance(p_user_public_id uuid)
RETURNS TABLE (balance_microcredits bigint)
LANGUAGE sql
SECURITY DEFINER
SET search_path = pg_catalog
STABLE
AS $function$
    SELECT COALESCE(balance.balance_microcredits, 0)::bigint
      FROM public.users AS app_user
      LEFT JOIN public.ledger_accounts AS account
        ON account.owner_user_id = app_user.id
       AND account.kind_code = 'user_credit'
       AND account.closed_at IS NULL
      LEFT JOIN public.ledger_balances AS balance
        ON balance.account_id = account.id
     WHERE app_user.public_id = p_user_public_id
       AND app_user.disabled_at IS NULL;
$function$;

REVOKE ALL ON FUNCTION find_user_credit_balance(uuid) FROM PUBLIC;

DO $block$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'contracter_runtime') THEN
        GRANT EXECUTE ON FUNCTION find_user_credit_balance(uuid) TO contracter_runtime;
    END IF;
END;
$block$;
