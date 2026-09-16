-- Fixes from an independent adversarial security review of the DB layer.
-- Two gaps fixed here; a third, related gap is deliberately not fixed by
-- this migration (see docs/database/BLOCKED_DECISIONS.md):
--
-- 1. contracter_runtime and contracter_admin_runtime both received a bare
--    `GRANT SELECT ON users` in db/migrations/0005_append_only_guards.sql.
--    An unqualified table grant covers every column, including
--    password_hash -- so any bug in a future player- or admin-facing read
--    path that touches `users` (a missing WHERE clause, a forgotten scope,
--    a later SQL injection elsewhere) could return every user's password
--    hash, not just public identity fields. Narrowed to a safe column list
--    for both roles; migrations 0001-0005 are frozen (SQLx checksums them),
--    so this is a REVOKE+re-GRANT in a new migration rather than an edit.
--
-- 2. finalize_contract and approve_critical_action trust their bigint
--    arguments completely: finalize_contract has no caller-identity
--    parameter at all (it never compares the calling session to
--    quote.user_id), and approve_critical_action's dual-control check
--    (validate_critical_action_approval: proposer <> approver) never
--    verifies the caller actually *is* p_approver_admin_id. Since every
--    player shares one contracter_runtime credential and every admin shares
--    one contracter_admin_runtime credential, nothing in the DB stops a
--    caller from passing someone else's quote_id or another admin's id --
--    the entire dual-control guarantee that post_credit_adjustment's
--    approval gate exists for depends on this identity binding holding.
--
--    This does not attempt to solve authentication in the database: a
--    shared connection-pool role fundamentally cannot verify identity on
--    its own, and building real per-request identity (per-user DB roles,
--    or row-level security keyed off a verified session claim) is an
--    application/authentication-architecture decision outside DATABASE
--    ONLY scope (see docs/database/BLOCKED_DECISIONS.md). What it does do
--    is close the "a handler simply forgot to check ownership" IDOR gap:
--    thin wrapper functions now require an explicit p_calling_user_id /
--    p_calling_admin_id argument and reject a mismatch, so identity has to
--    be an explicit, auditable argument at the one place all such calls
--    must pass through, instead of a silent, easy-to-forget check left
--    entirely to future application code. The wrappers still trust that the
--    caller passes a genuinely authenticated id -- that half of the
--    trust boundary is inherent to the shared-role model and is called out
--    explicitly in BLOCKED_DECISIONS, not silently assumed away here.
DO $block$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'contracter_runtime') THEN
        REVOKE SELECT ON users FROM contracter_runtime;
        GRANT SELECT (id, public_id, login, created_at, disabled_at)
            ON users TO contracter_runtime;
    END IF;

    IF EXISTS (
        SELECT 1 FROM pg_roles WHERE rolname = 'contracter_admin_runtime'
    ) THEN
        REVOKE SELECT ON users FROM contracter_admin_runtime;
        GRANT SELECT (id, public_id, login, created_at, disabled_at)
            ON users TO contracter_admin_runtime;
    END IF;
END;
$block$;

CREATE OR REPLACE FUNCTION finalize_contract_for_user(
    p_calling_user_id bigint,
    p_quote_id bigint,
    p_locked_input_ids bigint[],
    p_idempotency_key uuid
) RETURNS bigint
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog
AS $function$
DECLARE
    quote_owner_id bigint;
BEGIN
    SELECT quote.user_id INTO quote_owner_id
      FROM public.tradeup_quotes AS quote
     WHERE quote.id = p_quote_id;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'quote does not exist' USING ERRCODE = '23503';
    END IF;
    IF quote_owner_id IS DISTINCT FROM p_calling_user_id THEN
        RAISE EXCEPTION 'quote does not belong to the calling user'
            USING ERRCODE = '42501';
    END IF;

    RETURN public.finalize_contract(p_quote_id, p_locked_input_ids, p_idempotency_key);
END;
$function$;

CREATE OR REPLACE FUNCTION approve_critical_action_as_admin(
    p_calling_admin_id bigint,
    p_critical_action_id bigint,
    p_approver_admin_id bigint,
    p_payload_hash bytea
) RETURNS bigint
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog
AS $function$
BEGIN
    IF p_calling_admin_id IS DISTINCT FROM p_approver_admin_id THEN
        RAISE EXCEPTION 'an administrator cannot submit an approval as another administrator'
            USING ERRCODE = '42501';
    END IF;

    RETURN public.approve_critical_action(
        p_critical_action_id, p_approver_admin_id, p_payload_hash
    );
END;
$function$;

REVOKE ALL ON FUNCTION finalize_contract_for_user(bigint, bigint, bigint[], uuid)
    FROM PUBLIC;
REVOKE ALL ON FUNCTION approve_critical_action_as_admin(bigint, bigint, bigint, bytea)
    FROM PUBLIC;

DO $block$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'contracter_runtime') THEN
        REVOKE EXECUTE ON FUNCTION finalize_contract(bigint, bigint[], uuid)
            FROM contracter_runtime;
        GRANT EXECUTE ON FUNCTION
            finalize_contract_for_user(bigint, bigint, bigint[], uuid)
            TO contracter_runtime;
    END IF;

    IF EXISTS (
        SELECT 1 FROM pg_roles WHERE rolname = 'contracter_admin_runtime'
    ) THEN
        REVOKE EXECUTE ON FUNCTION approve_critical_action(bigint, bigint, bytea)
            FROM contracter_admin_runtime;
        GRANT EXECUTE ON FUNCTION
            approve_critical_action_as_admin(bigint, bigint, bigint, bytea)
            TO contracter_admin_runtime;
    END IF;
END;
$block$;
