CREATE TABLE IF NOT EXISTS collections (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    public_id uuid NOT NULL DEFAULT gen_random_uuid() UNIQUE,
    slug text NOT NULL UNIQUE,
    display_name text NOT NULL,
    enabled boolean NOT NULL DEFAULT true,
    CHECK (slug = lower(slug) AND slug ~ '^[a-z0-9][a-z0-9-]*$')
);

CREATE TABLE IF NOT EXISTS wear_bands (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    code text NOT NULL UNIQUE,
    lower_bound numeric(9,8) NOT NULL,
    upper_bound numeric(9,8) NOT NULL,
    includes_upper_bound boolean NOT NULL DEFAULT false,
    CHECK (lower_bound >= 0 AND upper_bound <= 1),
    CHECK (lower_bound < upper_bound)
);

CREATE TABLE IF NOT EXISTS catalog_items (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    public_id uuid NOT NULL DEFAULT gen_random_uuid() UNIQUE,
    collection_id bigint NOT NULL REFERENCES collections(id),
    rarity_code text NOT NULL REFERENCES rarities(code),
    stable_name text NOT NULL,
    min_float numeric(9,8) NOT NULL,
    max_float numeric(9,8) NOT NULL,
    enabled boolean NOT NULL DEFAULT true,
    is_stattrak boolean NOT NULL DEFAULT false,
    is_souvenir boolean NOT NULL DEFAULT false,
    UNIQUE (collection_id, stable_name),
    CHECK (min_float >= 0 AND max_float <= 1),
    CHECK (min_float < max_float),
    CHECK (NOT is_stattrak AND NOT is_souvenir)
);

CREATE INDEX IF NOT EXISTS catalog_items_outputs_idx
    ON catalog_items (collection_id, rarity_code, id) WHERE enabled;

CREATE TABLE IF NOT EXISTS skus (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    public_id uuid NOT NULL DEFAULT gen_random_uuid() UNIQUE,
    catalog_item_id bigint NOT NULL REFERENCES catalog_items(id),
    wear_band_id bigint NOT NULL REFERENCES wear_bands(id),
    enabled boolean NOT NULL DEFAULT true,
    UNIQUE (catalog_item_id, wear_band_id)
);

CREATE TABLE IF NOT EXISTS inventory_items (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    public_id uuid NOT NULL DEFAULT gen_random_uuid() UNIQUE,
    sku_id bigint NOT NULL REFERENCES skus(id),
    canonical_float numeric(9,8) NOT NULL,
    created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    retired_at timestamptz,
    CHECK (canonical_float BETWEEN 0 AND 1),
    CHECK (retired_at IS NULL OR retired_at >= created_at)
);

CREATE TABLE IF NOT EXISTS inventory_positions (
    inventory_item_id bigint PRIMARY KEY REFERENCES inventory_items(id),
    owner_user_id bigint REFERENCES users(id),
    in_warehouse boolean NOT NULL DEFAULT false,
    version bigint NOT NULL DEFAULT 1,
    updated_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    CHECK ((owner_user_id IS NOT NULL)::integer + in_warehouse::integer = 1),
    CHECK (version > 0)
);

CREATE INDEX IF NOT EXISTS inventory_positions_usable_user_idx
    ON inventory_positions (owner_user_id, inventory_item_id)
    WHERE owner_user_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS inventory_items_sku_idx ON inventory_items (sku_id, id);

CREATE TABLE IF NOT EXISTS inventory_transfer_events (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    public_id uuid NOT NULL DEFAULT gen_random_uuid() UNIQUE,
    inventory_item_id bigint NOT NULL REFERENCES inventory_items(id),
    event_kind_code text NOT NULL REFERENCES inventory_event_kinds(code),
    from_user_id bigint REFERENCES users(id),
    from_warehouse boolean NOT NULL DEFAULT false,
    to_user_id bigint REFERENCES users(id),
    to_warehouse boolean NOT NULL DEFAULT false,
    operation_public_id uuid NOT NULL,
    occurred_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    previous_event_hash bytea,
    event_hash bytea NOT NULL,
    CHECK ((from_user_id IS NOT NULL)::integer + from_warehouse::integer <= 1),
    CHECK ((to_user_id IS NOT NULL)::integer + to_warehouse::integer = 1),
    CHECK (from_user_id IS DISTINCT FROM to_user_id OR
           from_warehouse IS DISTINCT FROM to_warehouse),
    CHECK (previous_event_hash IS NULL OR octet_length(previous_event_hash) = 32),
    CHECK (octet_length(event_hash) = 32)
);

CREATE INDEX IF NOT EXISTS inventory_transfer_events_item_idx
    ON inventory_transfer_events (inventory_item_id, id DESC);

CREATE TABLE IF NOT EXISTS ledger_accounts (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    public_id uuid NOT NULL DEFAULT gen_random_uuid() UNIQUE,
    kind_code text NOT NULL REFERENCES ledger_account_kinds(code),
    owner_user_id bigint REFERENCES users(id),
    created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    closed_at timestamptz,
    CHECK (closed_at IS NULL OR closed_at >= created_at)
);

CREATE UNIQUE INDEX IF NOT EXISTS ledger_accounts_user_kind_key
    ON ledger_accounts (owner_user_id, kind_code) WHERE owner_user_id IS NOT NULL;

CREATE UNIQUE INDEX IF NOT EXISTS ledger_accounts_singleton_system_kind_key
    ON ledger_accounts (kind_code) WHERE owner_user_id IS NULL;

CREATE TABLE IF NOT EXISTS ledger_transactions (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    public_id uuid NOT NULL DEFAULT gen_random_uuid() UNIQUE,
    operation_kind text NOT NULL,
    idempotency_key uuid NOT NULL UNIQUE,
    request_hash bytea NOT NULL,
    created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    CHECK (operation_kind ~ '^[a-z][a-z0-9_]*$'),
    CHECK (octet_length(request_hash) = 32)
);

CREATE TABLE IF NOT EXISTS ledger_postings (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    ledger_transaction_id bigint NOT NULL REFERENCES ledger_transactions(id),
    account_id bigint NOT NULL REFERENCES ledger_accounts(id),
    amount_microcredits bigint NOT NULL,
    created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    UNIQUE (ledger_transaction_id, account_id),
    CHECK (amount_microcredits <> 0)
);

CREATE INDEX IF NOT EXISTS ledger_postings_account_idx
    ON ledger_postings (account_id, id DESC);

CREATE TABLE IF NOT EXISTS ledger_balances (
    account_id bigint PRIMARY KEY REFERENCES ledger_accounts(id),
    balance_microcredits bigint NOT NULL DEFAULT 0,
    version bigint NOT NULL DEFAULT 0,
    updated_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    CHECK (version >= 0)
);

CREATE OR REPLACE FUNCTION check_ledger_transaction_balanced()
RETURNS trigger
LANGUAGE plpgsql
SET search_path = pg_catalog
AS $function$
DECLARE
    affected_transaction_id bigint;
    posting_count bigint;
    posting_sum numeric;
BEGIN
    affected_transaction_id := COALESCE(NEW.ledger_transaction_id,
                                        OLD.ledger_transaction_id);

    SELECT count(*), COALESCE(sum(posting.amount_microcredits::numeric), 0)
      INTO posting_count, posting_sum
      FROM public.ledger_postings AS posting
     WHERE posting.ledger_transaction_id = affected_transaction_id;

    IF posting_count < 2 OR posting_sum <> 0 THEN
        RAISE EXCEPTION 'ledger transaction % is unbalanced',
                        affected_transaction_id
            USING ERRCODE = '23514';
    END IF;

    RETURN NULL;
END;
$function$;

DROP TRIGGER IF EXISTS ledger_postings_balanced ON ledger_postings;
CREATE CONSTRAINT TRIGGER ledger_postings_balanced
AFTER INSERT OR UPDATE OR DELETE ON ledger_postings
DEFERRABLE INITIALLY DEFERRED
FOR EACH ROW
EXECUTE FUNCTION check_ledger_transaction_balanced();

CREATE OR REPLACE FUNCTION post_ledger_transaction(
    p_operation_kind text,
    p_idempotency_key uuid,
    p_entries jsonb
) RETURNS bigint
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog
AS $function$
DECLARE
    transaction_id bigint;
    existing_hash bytea;
    request_hash bytea;
    entry_count bigint;
    distinct_account_count bigint;
    account_count bigint;
    entry_sum numeric;
    previous_write_setting text;
BEGIN
    IF p_operation_kind IS NULL OR
       p_operation_kind !~ '^[a-z][a-z0-9_]*$' OR
       p_idempotency_key IS NULL OR
       jsonb_typeof(p_entries) <> 'array' THEN
        RAISE EXCEPTION 'invalid ledger transaction request'
            USING ERRCODE = '23514';
    END IF;

    request_hash := public.digest(
        convert_to(p_operation_kind || ':' || p_entries::text, 'UTF8'),
        'sha256'
    );

    PERFORM pg_advisory_xact_lock(hashtextextended(p_idempotency_key::text, 0));

    SELECT id, ledger_transaction.request_hash
      INTO transaction_id, existing_hash
      FROM public.ledger_transactions AS ledger_transaction
     WHERE idempotency_key = p_idempotency_key;

    IF FOUND THEN
        IF existing_hash <> request_hash THEN
            RAISE EXCEPTION 'idempotency key was reused with another request'
                USING ERRCODE = '23505';
        END IF;
        RETURN transaction_id;
    END IF;

    SELECT count(*),
           count(DISTINCT (entry.value->>'account_id')::bigint),
           COALESCE(sum((entry.value->>'amount_microcredits')::numeric), 0)
      INTO entry_count, distinct_account_count, entry_sum
      FROM jsonb_array_elements(p_entries) AS entry(value);

    IF entry_count < 2 OR distinct_account_count <> entry_count OR entry_sum <> 0 THEN
        RAISE EXCEPTION 'ledger transaction must contain distinct balanced postings'
            USING ERRCODE = '23514';
    END IF;

    IF EXISTS (
        SELECT 1
          FROM jsonb_array_elements(p_entries) AS entry(value)
         WHERE (entry.value->>'amount_microcredits')::bigint = 0
    ) THEN
        RAISE EXCEPTION 'ledger postings cannot be zero'
            USING ERRCODE = '23514';
    END IF;

    PERFORM account.id
      FROM public.ledger_accounts AS account
     WHERE account.id IN (
        SELECT (entry.value->>'account_id')::bigint
          FROM jsonb_array_elements(p_entries) AS entry(value)
     )
     ORDER BY account.id
     FOR UPDATE;

    SELECT count(*) INTO account_count
      FROM public.ledger_accounts AS account
     WHERE account.id IN (
        SELECT (entry.value->>'account_id')::bigint
          FROM jsonb_array_elements(p_entries) AS entry(value)
     );

    IF account_count <> entry_count THEN
        RAISE EXCEPTION 'ledger request references a missing account'
            USING ERRCODE = '23503';
    END IF;

    previous_write_setting := current_setting('contracter.journal_write', true);
    PERFORM set_config('contracter.journal_write', 'on', true);

    BEGIN
        INSERT INTO public.ledger_transactions (
            operation_kind,
            idempotency_key,
            request_hash
        ) VALUES (
            p_operation_kind,
            p_idempotency_key,
            request_hash
        )
        RETURNING id INTO transaction_id;

        INSERT INTO public.ledger_postings (
            ledger_transaction_id,
            account_id,
            amount_microcredits
        )
        SELECT transaction_id,
               (entry.value->>'account_id')::bigint,
               (entry.value->>'amount_microcredits')::bigint
          FROM jsonb_array_elements(p_entries) AS entry(value)
         ORDER BY (entry.value->>'account_id')::bigint;

        SET CONSTRAINTS public.ledger_postings_balanced IMMEDIATE;
        SET CONSTRAINTS public.ledger_postings_balanced DEFERRED;

        INSERT INTO public.ledger_balances (
            account_id,
            balance_microcredits,
            version,
            updated_at
        )
        SELECT (entry.value->>'account_id')::bigint,
               (entry.value->>'amount_microcredits')::bigint,
               1,
               clock_timestamp()
          FROM jsonb_array_elements(p_entries) AS entry(value)
        ON CONFLICT (account_id) DO UPDATE
        SET balance_microcredits = public.ledger_balances.balance_microcredits +
                                   EXCLUDED.balance_microcredits,
            version = public.ledger_balances.version + 1,
            updated_at = EXCLUDED.updated_at;
    EXCEPTION
        WHEN OTHERS THEN
            PERFORM set_config(
                'contracter.journal_write',
                COALESCE(previous_write_setting, 'off'),
                true
            );
            RAISE;
    END;

    PERFORM set_config(
        'contracter.journal_write',
        COALESCE(previous_write_setting, 'off'),
        true
    );

    RETURN transaction_id;
END;
$function$;

CREATE TABLE IF NOT EXISTS credit_adjustment_events (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    public_id uuid NOT NULL DEFAULT gen_random_uuid() UNIQUE,
    initiator_admin_id bigint NOT NULL REFERENCES administrators(id),
    target_user_id bigint NOT NULL REFERENCES users(id),
    amount_microcredits bigint NOT NULL,
    execution_key uuid NOT NULL UNIQUE,
    critical_action_id bigint UNIQUE REFERENCES critical_actions(id),
    ledger_transaction_id bigint NOT NULL UNIQUE REFERENCES ledger_transactions(id),
    occurred_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    CHECK (amount_microcredits <> 0),
    CHECK (amount_microcredits <> '-9223372036854775808'::bigint)
);

CREATE INDEX IF NOT EXISTS credit_adjustment_rolling_idx
    ON credit_adjustment_events (
        initiator_admin_id,
        target_user_id,
        occurred_at DESC
    );

CREATE OR REPLACE FUNCTION post_credit_adjustment(
    p_initiator_admin_id bigint,
    p_target_user_id bigint,
    p_amount_microcredits bigint,
    p_execution_key uuid,
    p_critical_action_id bigint
) RETURNS bigint
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog
AS $function$
DECLARE
    system_account_id bigint;
    target_account_id bigint;
    existing_transaction_id bigint;
    rolling_amount numeric;
    approval_required boolean;
    action_row public.critical_actions%ROWTYPE;
    expected_payload jsonb;
    transaction_id bigint;
BEGIN
    IF p_amount_microcredits IS NULL OR p_amount_microcredits = 0 OR
       p_amount_microcredits = '-9223372036854775808'::bigint OR
       p_execution_key IS NULL THEN
        RAISE EXCEPTION 'invalid credit adjustment request'
            USING ERRCODE = '23514';
    END IF;

    PERFORM pg_advisory_xact_lock(hashtextextended(p_execution_key::text, 1));

    SELECT adjustment.ledger_transaction_id
      INTO existing_transaction_id
      FROM public.credit_adjustment_events AS adjustment
     WHERE adjustment.execution_key = p_execution_key;
    IF FOUND THEN
        RETURN existing_transaction_id;
    END IF;

    PERFORM 1
      FROM public.administrators AS administrator
     WHERE administrator.id = p_initiator_admin_id
       AND administrator.is_active
     FOR UPDATE;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'initiator must be an active administrator'
            USING ERRCODE = '42501';
    END IF;

    SELECT account.id INTO system_account_id
      FROM public.ledger_accounts AS account
     WHERE account.kind_code = 'system_treasury'
       AND account.owner_user_id IS NULL
       AND account.closed_at IS NULL;

    SELECT account.id INTO target_account_id
      FROM public.ledger_accounts AS account
     WHERE account.kind_code = 'user_credit'
       AND account.owner_user_id = p_target_user_id
       AND account.closed_at IS NULL;

    IF system_account_id IS NULL OR target_account_id IS NULL THEN
        RAISE EXCEPTION 'required credit ledger account is missing'
            USING ERRCODE = '23503';
    END IF;

    PERFORM account.id
      FROM public.ledger_accounts AS account
     WHERE account.id IN (system_account_id, target_account_id)
     ORDER BY account.id
     FOR UPDATE;

    SELECT COALESCE(sum(abs(adjustment.amount_microcredits::numeric)), 0)
      INTO rolling_amount
      FROM public.credit_adjustment_events AS adjustment
     WHERE adjustment.initiator_admin_id = p_initiator_admin_id
       AND adjustment.target_user_id = p_target_user_id
       AND adjustment.occurred_at >= clock_timestamp() - interval '24 hours';

    approval_required := abs(p_amount_microcredits::numeric) > 100000000 OR
                         rolling_amount + abs(p_amount_microcredits::numeric) > 100000000;

    IF approval_required THEN
        IF p_critical_action_id IS NULL THEN
            RAISE EXCEPTION 'dual approval is required for this credit adjustment'
                USING ERRCODE = '42501';
        END IF;

        SELECT * INTO action_row
          FROM public.critical_actions AS action
         WHERE action.id = p_critical_action_id
         FOR UPDATE;

        expected_payload := jsonb_build_object(
            'target_user_id', p_target_user_id,
            'amount_microcredits', p_amount_microcredits,
            'execution_key', p_execution_key
        );

        IF NOT FOUND OR action_row.action_type_code <> 'credit_adjustment' OR
           action_row.proposer_admin_id <> p_initiator_admin_id OR
           action_row.payload <> expected_payload OR
           action_row.payload_hash <> public.digest(
               convert_to(expected_payload::text, 'UTF8'), 'sha256'
           ) OR
           clock_timestamp() > action_row.expires_at OR
           clock_timestamp() > action_row.requested_at + interval '24 hours' THEN
            RAISE EXCEPTION 'critical action does not authorize this adjustment'
                USING ERRCODE = '42501';
        END IF;

        IF NOT EXISTS (
            SELECT 1
              FROM public.critical_action_approval_events AS approval
              JOIN public.administrators AS approver
                ON approver.id = approval.approver_admin_id
             WHERE approval.critical_action_id = action_row.id
               AND approval.payload_hash = action_row.payload_hash
               AND approval.approver_admin_id <> action_row.proposer_admin_id
               AND approver.is_active
               AND approval.approved_at <= action_row.expires_at
               AND approval.approved_at <= action_row.requested_at + interval '24 hours'
        ) OR EXISTS (
            SELECT 1
              FROM public.critical_action_execution_events AS execution
             WHERE execution.critical_action_id = action_row.id
        ) THEN
            RAISE EXCEPTION 'valid unused second approval is required'
                USING ERRCODE = '42501';
        END IF;
    ELSIF p_critical_action_id IS NOT NULL THEN
        RAISE EXCEPTION 'unexpected critical action for unprivileged adjustment'
            USING ERRCODE = '23514';
    END IF;

    transaction_id := public.post_ledger_transaction(
        'credit_adjustment',
        p_execution_key,
        jsonb_build_array(
            jsonb_build_object(
                'account_id', system_account_id,
                'amount_microcredits', -p_amount_microcredits
            ),
            jsonb_build_object(
                'account_id', target_account_id,
                'amount_microcredits', p_amount_microcredits
            )
        )
    );

    INSERT INTO public.credit_adjustment_events (
        initiator_admin_id,
        target_user_id,
        amount_microcredits,
        execution_key,
        critical_action_id,
        ledger_transaction_id
    ) VALUES (
        p_initiator_admin_id,
        p_target_user_id,
        p_amount_microcredits,
        p_execution_key,
        p_critical_action_id,
        transaction_id
    );

    IF approval_required THEN
        INSERT INTO public.critical_action_execution_events (
            critical_action_id,
            execution_key,
            payload_hash,
            result_reference
        ) VALUES (
            action_row.id,
            p_execution_key,
            action_row.payload_hash,
            'ledger_transaction:' || transaction_id::text
        );
    END IF;

    RETURN transaction_id;
END;
$function$;

REVOKE ALL ON FUNCTION post_ledger_transaction(text, uuid, jsonb) FROM PUBLIC;
REVOKE ALL ON FUNCTION post_credit_adjustment(bigint, bigint, bigint, uuid, bigint)
    FROM PUBLIC;
