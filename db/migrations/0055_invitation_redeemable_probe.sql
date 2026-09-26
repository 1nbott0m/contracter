-- A cheap redeemability probe, so registration does not pay for Argon2id
-- before it knows the invitation is worth anything.
--
-- `application::auth::register` derived the password hash first and only
-- then called `register_invited_user`. Argon2id costs ~19 MiB and tens of
-- milliseconds by design, and concurrency is capped by a semaphore sized
-- to the machine's parallelism. So an attacker holding no invitation at
-- all could keep that semaphore permanently full with garbage
-- registrations, and every legitimate login would then queue for five
-- seconds and receive 503. An independent security review identified this
-- as the sharpest consequence of the missing rate limiter.
--
-- This function answers only "would redeeming this token succeed right
-- now", and nothing else: no id, no expiry, no redemption count. It is
-- explicitly NOT the authorization step. Registration still goes through
-- `register_invited_user`, which locks the invitation FOR UPDATE and
-- redeems it atomically, so two callers racing one invitation still
-- resolve to a single winner. This probe only lets the loser stop before
-- spending the memory.
--
-- The timing difference it introduces is deliberate and acceptable: a
-- rejected token now fails fast instead of after a hash. That reveals
-- token validity, which is the one thing a legitimate holder needs to
-- know, and guessing a 32-byte token is not a search anyone completes.
CREATE OR REPLACE FUNCTION invitation_is_redeemable(
    p_invitation_token_hash bytea
) RETURNS boolean
LANGUAGE sql
STABLE
SECURITY DEFINER
SET search_path = pg_catalog
AS $function$
    SELECT EXISTS (
        SELECT 1
        FROM public.invitations AS invitation
        WHERE invitation.token_hash = p_invitation_token_hash
          AND invitation.expires_at > clock_timestamp()
          AND NOT EXISTS (
              SELECT 1
              FROM public.invitation_redemptions AS redemption
              WHERE redemption.invitation_id = invitation.id
          )
    );
$function$;

REVOKE ALL ON FUNCTION invitation_is_redeemable(bytea) FROM PUBLIC;

DO $block$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'contracter_runtime') THEN
        GRANT EXECUTE ON FUNCTION invitation_is_redeemable(bytea) TO contracter_runtime;
    END IF;
END;
$block$;
