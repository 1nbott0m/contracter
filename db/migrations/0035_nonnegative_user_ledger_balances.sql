-- User credit accounts are never allowed to settle below zero. System and
-- treasury accounts remain unrestricted because balanced compensating entries
-- and administrative corrections may legitimately move them negative.
CREATE OR REPLACE FUNCTION enforce_nonnegative_user_ledger_balance()
RETURNS trigger
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog
AS $function$
DECLARE
    account_kind text;
BEGIN
    SELECT kind_code INTO account_kind
      FROM public.ledger_accounts
     WHERE id = NEW.account_id;
    IF account_kind = 'user_credit' AND NEW.balance_microcredits < 0 THEN
        RAISE EXCEPTION 'user credit balance cannot become negative'
            USING ERRCODE = '23514';
    END IF;
    RETURN NEW;
END;
$function$;

DROP TRIGGER IF EXISTS ledger_balances_nonnegative_user ON public.ledger_balances;
CREATE CONSTRAINT TRIGGER ledger_balances_nonnegative_user
AFTER INSERT OR UPDATE OF balance_microcredits ON public.ledger_balances
DEFERRABLE INITIALLY IMMEDIATE
FOR EACH ROW EXECUTE FUNCTION enforce_nonnegative_user_ledger_balance();

REVOKE ALL ON FUNCTION enforce_nonnegative_user_ledger_balance() FROM PUBLIC;
DO $block$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'contracter_runtime') THEN
        REVOKE ALL ON FUNCTION enforce_nonnegative_user_ledger_balance()
            FROM contracter_runtime;
    END IF;
END;
$block$;
