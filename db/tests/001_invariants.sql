\set ON_ERROR_STOP on

BEGIN;

-- Force allocation of this session's temporary schema before defining
-- transaction-scoped assertion helpers in pg_temp.
CREATE TEMP TABLE _assertion_context (unused boolean);

-- This suite deliberately uses only PostgreSQL and PL/pgSQL.  It is both the
-- executable contract for the migrations and the RED test written before them.
-- Stable SQLSTATE contract used below:
--   23505 unique_violation
--   23514 check_violation (business invariant)
--   42501 insufficient_privilege (missing required second approval)
--   55000 object_not_in_prerequisite_state (append-only mutation)

CREATE OR REPLACE FUNCTION pg_temp.assert_true(
    test_name text,
    condition boolean
) RETURNS void
LANGUAGE plpgsql
AS $function$
BEGIN
    IF condition IS DISTINCT FROM true THEN
        RAISE EXCEPTION 'assertion failed: %', test_name;
    END IF;
END;
$function$;

CREATE OR REPLACE FUNCTION pg_temp.assert_sqlstate(
    test_name text,
    expected_state text,
    statement text
) RETURNS void
LANGUAGE plpgsql
AS $function$
DECLARE
    actual_state text;
    actual_message text;
BEGIN
    BEGIN
        EXECUTE statement;
    EXCEPTION
        WHEN OTHERS THEN
            GET STACKED DIAGNOSTICS
                actual_state = RETURNED_SQLSTATE,
                actual_message = MESSAGE_TEXT;

            IF actual_state <> expected_state THEN
                RAISE EXCEPTION
                    'assertion failed: %, expected SQLSTATE %, got % (%)',
                    test_name,
                    expected_state,
                    actual_state,
                    actual_message;
            END IF;

            RETURN;
    END;

    RAISE EXCEPTION
        'assertion failed: %, expected SQLSTATE %, but statement succeeded',
        test_name,
        expected_state;
END;
$function$;

CREATE OR REPLACE FUNCTION pg_temp.assert_ok(
    test_name text,
    statement text
) RETURNS void
LANGUAGE plpgsql
AS $function$
DECLARE
    actual_state text;
    actual_message text;
BEGIN
    BEGIN
        EXECUTE statement;
    EXCEPTION
        WHEN OTHERS THEN
            GET STACKED DIAGNOSTICS
                actual_state = RETURNED_SQLSTATE,
                actual_message = MESSAGE_TEXT;
            RAISE EXCEPTION
                'assertion failed: %, unexpected SQLSTATE % (%)',
                test_name,
                actual_state,
                actual_message;
    END;
END;
$function$;

CREATE OR REPLACE FUNCTION pg_temp.has_unique_single_column(
    relation regclass,
    column_name text
) RETURNS boolean
LANGUAGE sql
STABLE
AS $function$
    SELECT EXISTS (
        SELECT 1
        FROM pg_index AS index_definition
        JOIN pg_attribute AS indexed_column
          ON indexed_column.attrelid = index_definition.indrelid
         AND indexed_column.attnum = index_definition.indkey[0]
        WHERE index_definition.indrelid = relation
          AND index_definition.indisvalid
          AND index_definition.indisunique
          AND index_definition.indnkeyatts = 1
          AND indexed_column.attname = column_name
    );
$function$;

CREATE OR REPLACE FUNCTION pg_temp.assert_append_only(
    relation regclass
) RETURNS void
LANGUAGE plpgsql
AS $function$
BEGIN
    -- A statement-level guard is intentional: direct mutation is forbidden even
    -- when the predicate happens to match zero rows.
    PERFORM pg_temp.assert_sqlstate(
        relation::text || ' rejects UPDATE',
        '55000',
        format('UPDATE %s SET id = DEFAULT WHERE false', relation)
    );

    PERFORM pg_temp.assert_sqlstate(
        relation::text || ' rejects DELETE',
        '55000',
        format('DELETE FROM %s WHERE false', relation)
    );
END;
$function$;

-- Tests own their reference rows because Task 3 installs production seed data.
INSERT INTO critical_action_types (code, requires_dual_approval)
VALUES ('credit_adjustment', true)
ON CONFLICT (code) DO NOTHING;

INSERT INTO ledger_account_kinds (code)
VALUES ('system_treasury'), ('user_credit')
ON CONFLICT (code) DO NOTHING;

INSERT INTO users (public_id, login, password_hash)
VALUES
    ('10000000-0000-0000-0000-000000000001', 'invariant_admin_a', 'argon2id-test-hash-a'),
    ('10000000-0000-0000-0000-000000000002', 'invariant_admin_b', 'argon2id-test-hash-b'),
    ('10000000-0000-0000-0000-000000000003', 'invariant_target_a', 'argon2id-test-hash-c'),
    ('10000000-0000-0000-0000-000000000004', 'invariant_target_b', 'argon2id-test-hash-d');

SELECT pg_temp.assert_sqlstate(
    'duplicate login is rejected',
    '23505',
    $sql$
        INSERT INTO users (public_id, login, password_hash)
        VALUES (
            '10000000-0000-0000-0000-000000000005',
            'invariant_admin_a',
            'argon2id-test-hash-duplicate'
        )
    $sql$
);

INSERT INTO invitations (public_id, token_hash, expires_at)
VALUES (
    '20000000-0000-0000-0000-000000000001',
    decode(repeat('11', 32), 'hex'),
    clock_timestamp() + interval '1 hour'
);

INSERT INTO invitation_redemptions (invitation_id, user_id)
SELECT invitation.id, app_user.id
FROM invitations AS invitation
JOIN users AS app_user
  ON app_user.public_id = '10000000-0000-0000-0000-000000000003'
WHERE invitation.public_id = '20000000-0000-0000-0000-000000000001';

SELECT pg_temp.assert_sqlstate(
    'an invitation is single-use',
    '23505',
    $sql$
        INSERT INTO invitation_redemptions (invitation_id, user_id)
        SELECT invitation.id, app_user.id
        FROM invitations AS invitation
        JOIN users AS app_user
          ON app_user.public_id = '10000000-0000-0000-0000-000000000004'
        WHERE invitation.public_id = '20000000-0000-0000-0000-000000000001'
    $sql$
);

INSERT INTO administrators (user_id, is_active)
SELECT id, true
FROM users
WHERE public_id IN (
    '10000000-0000-0000-0000-000000000001',
    '10000000-0000-0000-0000-000000000002'
);

INSERT INTO ledger_accounts (public_id, kind_code, owner_user_id)
VALUES (
    '30000000-0000-0000-0000-000000000001',
    'system_treasury',
    NULL
);

INSERT INTO ledger_accounts (public_id, kind_code, owner_user_id)
SELECT
    CASE app_user.public_id
        WHEN '10000000-0000-0000-0000-000000000003'
            THEN '30000000-0000-0000-0000-000000000003'::uuid
        ELSE '30000000-0000-0000-0000-000000000004'::uuid
    END,
    'user_credit',
    app_user.id
FROM users AS app_user
WHERE app_user.public_id IN (
    '10000000-0000-0000-0000-000000000003',
    '10000000-0000-0000-0000-000000000004'
);

-- Fresh action: proposer cannot approve it, while the other active admin can.
WITH action_payload AS (
    SELECT '{"purpose":"self-approval-test"}'::jsonb AS body
)
INSERT INTO critical_actions (
    public_id,
    action_type_code,
    proposer_admin_id,
    payload,
    payload_hash,
    requested_at,
    expires_at
)
SELECT
    '40000000-0000-0000-0000-000000000001',
    'credit_adjustment',
    administrator.id,
    action_payload.body,
    digest(convert_to(action_payload.body::text, 'UTF8'), 'sha256'),
    clock_timestamp(),
    clock_timestamp() + interval '24 hours'
FROM administrators AS administrator
JOIN users AS app_user ON app_user.id = administrator.user_id
CROSS JOIN action_payload
WHERE app_user.public_id = '10000000-0000-0000-0000-000000000001';

SELECT pg_temp.assert_sqlstate(
    'critical-action proposer cannot self-approve',
    '23514',
    $sql$
        INSERT INTO critical_action_approval_events (
            critical_action_id,
            approver_admin_id,
            payload_hash
        )
        SELECT action.id, action.proposer_admin_id, action.payload_hash
        FROM critical_actions AS action
        WHERE action.public_id = '40000000-0000-0000-0000-000000000001'
    $sql$
);

INSERT INTO critical_action_approval_events (
    critical_action_id,
    approver_admin_id,
    payload_hash
)
SELECT action.id, administrator.id, action.payload_hash
FROM critical_actions AS action
JOIN users AS app_user
  ON app_user.public_id = '10000000-0000-0000-0000-000000000002'
JOIN administrators AS administrator
  ON administrator.user_id = app_user.id
WHERE action.public_id = '40000000-0000-0000-0000-000000000001';

-- The payload and hash are immutable after proposal.
SELECT pg_temp.assert_sqlstate(
    'critical-action payload is immutable',
    '55000',
    $sql$
        UPDATE critical_actions
        SET payload = '{"purpose":"tampered"}'::jsonb
        WHERE public_id = '40000000-0000-0000-0000-000000000001'
    $sql$
);

-- An approval recorded after the action's 24-hour window is rejected.
WITH action_payload AS (
    SELECT '{"purpose":"expired-approval-test"}'::jsonb AS body
)
INSERT INTO critical_actions (
    public_id,
    action_type_code,
    proposer_admin_id,
    payload,
    payload_hash,
    requested_at,
    expires_at
)
SELECT
    '40000000-0000-0000-0000-000000000002',
    'credit_adjustment',
    administrator.id,
    action_payload.body,
    digest(convert_to(action_payload.body::text, 'UTF8'), 'sha256'),
    clock_timestamp() - interval '25 hours',
    clock_timestamp() - interval '1 hour'
FROM administrators AS administrator
JOIN users AS app_user ON app_user.id = administrator.user_id
CROSS JOIN action_payload
WHERE app_user.public_id = '10000000-0000-0000-0000-000000000001';

SELECT pg_temp.assert_sqlstate(
    'critical-action approval expires after 24 hours',
    '23514',
    $sql$
        INSERT INTO critical_action_approval_events (
            critical_action_id,
            approver_admin_id,
            payload_hash
        )
        SELECT action.id, administrator.id, action.payload_hash
        FROM critical_actions AS action
        JOIN users AS app_user
          ON app_user.public_id = '10000000-0000-0000-0000-000000000002'
        JOIN administrators AS administrator
          ON administrator.user_id = app_user.id
        WHERE action.public_id = '40000000-0000-0000-0000-000000000002'
    $sql$
);

-- The threshold is strictly above 100 USD.  Internal settlement is in
-- microcredits: 10000 cents = 100,000,000 microcredits.
SELECT pg_temp.assert_ok(
    'a 10000-cent adjustment does not require dual approval',
    $sql$
        SELECT post_credit_adjustment(
            (SELECT administrator.id
             FROM administrators AS administrator
             JOIN users AS app_user ON app_user.id = administrator.user_id
             WHERE app_user.public_id = '10000000-0000-0000-0000-000000000001'),
            (SELECT id FROM users
             WHERE public_id = '10000000-0000-0000-0000-000000000003'),
            100000000,
            '50000000-0000-0000-0000-000000000001',
            NULL
        )
    $sql$
);

SELECT pg_temp.assert_sqlstate(
    'a credit adjustment execution key cannot be reused with another request',
    '23505',
    $sql$
        SELECT post_credit_adjustment(
            (SELECT administrator.id
             FROM administrators AS administrator
             JOIN users AS app_user ON app_user.id = administrator.user_id
             WHERE app_user.public_id = '10000000-0000-0000-0000-000000000001'),
            (SELECT id FROM users
             WHERE public_id = '10000000-0000-0000-0000-000000000003'),
            99999999,
            '50000000-0000-0000-0000-000000000001',
            NULL
        )
    $sql$
);

SELECT pg_temp.assert_sqlstate(
    'a 10001-cent adjustment requires dual approval',
    '42501',
    $sql$
        SELECT post_credit_adjustment(
            (SELECT administrator.id
             FROM administrators AS administrator
             JOIN users AS app_user ON app_user.id = administrator.user_id
             WHERE app_user.public_id = '10000000-0000-0000-0000-000000000001'),
            (SELECT id FROM users
             WHERE public_id = '10000000-0000-0000-0000-000000000004'),
            100010000,
            '50000000-0000-0000-0000-000000000002',
            NULL
        )
    $sql$
);

-- A matching immutable payload approved by the other administrator authorizes
-- the same above-threshold adjustment.
WITH action_payload AS (
    SELECT jsonb_build_object(
        'target_user_id', app_user.id,
        'amount_microcredits', 100010000,
        'execution_key', '50000000-0000-0000-0000-000000000003'::uuid
    ) AS body
    FROM users AS app_user
    WHERE app_user.public_id = '10000000-0000-0000-0000-000000000004'
)
INSERT INTO critical_actions (
    public_id,
    action_type_code,
    proposer_admin_id,
    payload,
    payload_hash,
    requested_at,
    expires_at
)
SELECT
    '40000000-0000-0000-0000-000000000003',
    'credit_adjustment',
    administrator.id,
    action_payload.body,
    digest(convert_to(action_payload.body::text, 'UTF8'), 'sha256'),
    clock_timestamp(),
    clock_timestamp() + interval '24 hours'
FROM administrators AS administrator
JOIN users AS app_user ON app_user.id = administrator.user_id
CROSS JOIN action_payload
WHERE app_user.public_id = '10000000-0000-0000-0000-000000000001';

INSERT INTO critical_action_approval_events (
    critical_action_id,
    approver_admin_id,
    payload_hash
)
SELECT action.id, administrator.id, action.payload_hash
FROM critical_actions AS action
JOIN users AS app_user
  ON app_user.public_id = '10000000-0000-0000-0000-000000000002'
JOIN administrators AS administrator
  ON administrator.user_id = app_user.id
WHERE action.public_id = '40000000-0000-0000-0000-000000000003';

SELECT pg_temp.assert_ok(
    'a valid second approval authorizes a 10001-cent adjustment',
    $sql$
        SELECT post_credit_adjustment(
            (SELECT administrator.id
             FROM administrators AS administrator
             JOIN users AS app_user ON app_user.id = administrator.user_id
             WHERE app_user.public_id = '10000000-0000-0000-0000-000000000001'),
            (SELECT id FROM users
             WHERE public_id = '10000000-0000-0000-0000-000000000004'),
            100010000,
            '50000000-0000-0000-0000-000000000003',
            (SELECT id FROM critical_actions
             WHERE public_id = '40000000-0000-0000-0000-000000000003')
        )
    $sql$
);

-- post_ledger_transaction(operation_kind, idempotency_key, entries) is the
-- privileged posting boundary.  The deferred balance constraint may surface
-- when the function forces its constraints immediate, but the public contract
-- is the same stable check_violation.
SELECT pg_temp.assert_sqlstate(
    'an unbalanced ledger transaction is rejected',
    '23514',
    $sql$
        SELECT post_ledger_transaction(
            'invariant_unbalanced',
            '60000000-0000-0000-0000-000000000001',
            jsonb_build_array(
                jsonb_build_object(
                    'account_id',
                    (SELECT id FROM ledger_accounts
                     WHERE public_id = '30000000-0000-0000-0000-000000000003'),
                    'amount_microcredits',
                    1
                )
            )
        )
    $sql$
);

-- Input cardinality is validated before quote lookup or mutation.  This keeps
-- malformed requests cheap and makes the accepted range independently
-- observable even though quote/stock fixtures are introduced by later tests.
-- A contract takes four to ten inputs inclusive (0018); both edges are
-- asserted, because a range guard that only checks one side is half a guard.
SELECT pg_temp.assert_sqlstate(
    'three locked contract inputs are below the minimum',
    '23514',
    $sql$
        SELECT finalize_contract(
            0,
            ARRAY[1,2,3]::bigint[],
            '70000000-0000-0000-0000-000000000001'
        )
    $sql$
);

SELECT pg_temp.assert_sqlstate(
    'eleven locked contract inputs are above the maximum',
    '23514',
    $sql$
        SELECT finalize_contract(
            0,
            ARRAY[1,2,3,4,5,6,7,8,9,10,11]::bigint[],
            '70000000-0000-0000-0000-000000000002'
        )
    $sql$
);

-- Distinctness is not implied by the range.  Without this, one item passed
-- five times would satisfy a bare length check and be spent as though it
-- were five different items.
SELECT pg_temp.assert_sqlstate(
    'a repeated inventory item cannot pad a contract to the minimum',
    '23514',
    $sql$
        SELECT finalize_contract(
            0,
            ARRAY[1,1,2,3]::bigint[],
            '70000000-0000-0000-0000-000000000003'
        )
    $sql$
);

-- The lower and upper bounds themselves are accepted by the cardinality
-- guard: each gets past it and fails later, on the quote that does not
-- exist (23503, a foreign-key failure distinct from the 23514 above), which
-- is what proves the guard let them through rather than the call happening
-- to fail for the same reason as the out-of-range cases.
SELECT pg_temp.assert_sqlstate(
    'four unique inputs pass the cardinality guard',
    '23503',
    $sql$
        SELECT finalize_contract(
            0,
            ARRAY[1,2,3,4]::bigint[],
            '70000000-0000-0000-0000-000000000004'
        )
    $sql$
);

SELECT pg_temp.assert_sqlstate(
    'ten unique inputs pass the cardinality guard',
    '23503',
    $sql$
        SELECT finalize_contract(
            0,
            ARRAY[1,2,3,4,5,6,7,8,9,10]::bigint[],
            '70000000-0000-0000-0000-000000000005'
        )
    $sql$
);

-- finalize_contract_for_user / approve_critical_action_as_admin
-- (db/migrations/0009_security_hardening.sql) bind a caller-supplied
-- identity to the quote/approval before delegating to the underlying
-- privileged function, closing the IDOR gap those functions otherwise
-- leave to the application layer. A nonexistent quote/critical action still
-- surfaces the same "does not exist" error the wrapped function raises.
SELECT pg_temp.assert_sqlstate(
    'finalize_contract_for_user rejects a nonexistent quote the same as finalize_contract',
    '23503',
    $sql$
        SELECT finalize_contract_for_user(
            1,
            0,
            ARRAY[1,2,3,4,5,6,7,8,9,10]::bigint[],
            '70000000-0000-0000-0000-000000000003'
        )
    $sql$
);

SELECT pg_temp.assert_sqlstate(
    'approve_critical_action_as_admin rejects a caller impersonating another admin',
    '42501',
    $sql$
        SELECT approve_critical_action_as_admin(
            1,
            0,
            2,
            decode(repeat('00', 32), 'hex')
        )
    $sql$
);

-- Every role-privilege assertion below is guarded by "the role exists",
-- because the runtime roles are provisioned outside the migrations and a
-- bare schema dump has none.  That guard is also a trap: without the role,
-- each of those assertions passes for the wrong reason and the run looks
-- exactly like one that verified them.  Say so, loudly, once.
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'contracter_runtime')
    THEN
        RAISE WARNING 'SKIPPED (not verified): every contracter_runtime privilege assertion in this suite. The role does not exist in this database, so the least-privilege boundary is UNVERIFIED here. Create the runtime roles before treating this run as evidence.';
    ELSE
        RAISE NOTICE 'contracter_runtime exists: privilege assertions below are live.';
    END IF;
END;
$$;

-- contracter_runtime and contracter_admin_runtime must never receive
-- column-level access to password_hash: a bare GRANT SELECT ON users
-- covers every column unless explicitly restricted (finding from an
-- independent adversarial security review; docs/database/02-gap-analysis.md
-- finding 12). Only exercised when the runtime roles actually exist, same
-- convention as every other role-privilege check in this project.
SELECT pg_temp.assert_true(
    'contracter_runtime has no column privilege on users.password_hash',
    NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'contracter_runtime')
    OR NOT has_column_privilege('contracter_runtime', 'users', 'password_hash', 'SELECT')
);
SELECT pg_temp.assert_true(
    'contracter_admin_runtime has no column privilege on users.password_hash',
    NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'contracter_admin_runtime')
    OR NOT has_column_privilege(
        'contracter_admin_runtime', 'users', 'password_hash', 'SELECT'
    )
);
SELECT pg_temp.assert_true(
    'contracter_runtime cannot execute finalize_contract directly, only the identity-checked wrapper',
    NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'contracter_runtime')
    OR (
        NOT has_function_privilege(
            'contracter_runtime', 'finalize_contract(bigint, bigint[], uuid)', 'EXECUTE'
        )
        AND has_function_privilege(
            'contracter_runtime',
            'finalize_contract_for_user(bigint, bigint, bigint[], uuid)',
            'EXECUTE'
        )
    )
);
SELECT pg_temp.assert_true(
    'contracter_admin_runtime cannot execute approve_critical_action directly, only the identity-checked wrapper',
    NOT EXISTS (
        SELECT 1 FROM pg_roles WHERE rolname = 'contracter_admin_runtime'
    )
    OR (
        NOT has_function_privilege(
            'contracter_admin_runtime',
            'approve_critical_action(bigint, bigint, bytea)',
            'EXECUTE'
        )
        AND has_function_privilege(
            'contracter_admin_runtime',
            'approve_critical_action_as_admin(bigint, bigint, bigint, bytea)',
            'EXECUTE'
        )
    )
);

-- Exactly one durable acceptance event may exist per quote.  A retry with the
-- same idempotency key is served from that result rather than appending again;
-- a second acceptance with a different key is therefore blocked by quote_id.
SELECT pg_temp.assert_true(
    'duplicate quote acceptance is prevented by a unique quote_id',
    pg_temp.has_unique_single_column('quote_acceptance_events'::regclass, 'quote_id')
);

-- All history tables reject direct UPDATE and DELETE.  The migrations may use
-- security-definer functions for INSERT, but corrections remain compensating
-- events rather than journal rewrites.
SELECT pg_temp.assert_append_only('ledger_transactions'::regclass);
SELECT pg_temp.assert_append_only('ledger_postings'::regclass);
SELECT pg_temp.assert_append_only('inventory_transfer_events'::regclass);
SELECT pg_temp.assert_append_only('seed_commitment_events'::regclass);
SELECT pg_temp.assert_append_only('seed_revelation_events'::regclass);
SELECT pg_temp.assert_append_only('contract_inputs'::regclass);
SELECT pg_temp.assert_append_only('contract_outcomes'::regclass);
SELECT pg_temp.assert_append_only('critical_action_approval_events'::regclass);
SELECT pg_temp.assert_append_only('quote_acceptance_events'::regclass);
SELECT pg_temp.assert_append_only('sale_evidence'::regclass);

-- Auth access boundary (db/migrations/0015_auth_session_access.sql): the
-- HTTP runtime role reaches credentials and sessions only through the
-- narrow SECURITY DEFINER functions, never by reading the tables. In
-- particular the password_hash column restriction from 0013 must survive
-- the addition of a login flow that needs the hash.
SELECT pg_temp.assert_true(
    'contracter_runtime can execute the auth functions it needs',
    NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'contracter_runtime')
    OR (
        has_function_privilege(
            'contracter_runtime', 'find_user_credential_by_login(text)', 'EXECUTE'
        )
        AND has_function_privilege(
            'contracter_runtime', 'register_invited_user(bytea, text, text)', 'EXECUTE'
        )
        AND has_function_privilege(
            'contracter_runtime', 'create_user_session(bigint, bytea, interval)', 'EXECUTE'
        )
        AND has_function_privilege(
            'contracter_runtime', 'find_active_user_session(bytea)', 'EXECUTE'
        )
        AND has_function_privilege(
            'contracter_runtime', 'revoke_user_session(bytea)', 'EXECUTE'
        )
        AND has_function_privilege(
            'contracter_runtime', 'revoke_all_user_sessions(bigint)', 'EXECUTE'
        )
        AND has_function_privilege(
            'contracter_runtime', 'find_account_by_public_id(uuid)', 'EXECUTE'
        )
    )
);
-- The runtime role must not be able to enumerate logins.
--
-- It cannot read password_hash directly, but it can execute
-- find_user_credential_by_login, which returns one. Reading `users.login`
-- as well turns that verification function into a bulk dump via
-- CROSS JOIN LATERAL. Migration 0019 removed the column grant; this keeps
-- it removed, because restoring it looks harmless in isolation.
SELECT pg_temp.assert_true(
    'contracter_runtime cannot read users.login, so it cannot enumerate credentials',
    NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'contracter_runtime')
    OR NOT has_column_privilege('contracter_runtime', 'users', 'login', 'SELECT')
);

SELECT pg_temp.assert_true(
    'contracter_runtime still cannot read password_hash directly after the login flow exists',
    NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'contracter_runtime')
    OR NOT has_column_privilege('contracter_runtime', 'users', 'password_hash', 'SELECT')
);
SELECT pg_temp.assert_true(
    'contracter_runtime has no direct table access to sessions, invitations, or recovery codes',
    NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'contracter_runtime')
    OR (
        SELECT bool_and(
            NOT has_table_privilege('contracter_runtime', protected.name, action.name)
        )
        FROM (VALUES
            ('public.user_sessions'),
            ('public.invitations'),
            ('public.invitation_redemptions'),
            ('public.recovery_codes')
        ) AS protected(name)
        CROSS JOIN (VALUES
            ('SELECT'), ('INSERT'), ('UPDATE'), ('DELETE')
        ) AS action(name)
    )
);

-- An invitation is single-use: the redemption table enforces it
-- structurally, independently of register_invited_user's own check.
SELECT pg_temp.assert_true(
    'an invitation can be redeemed at most once',
    pg_temp.has_unique_single_column('invitation_redemptions'::regclass, 'invitation_id')
);

SELECT '001_invariants: ok' AS result;

ROLLBACK;
