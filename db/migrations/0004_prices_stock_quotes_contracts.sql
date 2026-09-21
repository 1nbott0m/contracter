CREATE TABLE IF NOT EXISTS price_sources (
    code text PRIMARY KEY,
    display_name text NOT NULL,
    enabled boolean NOT NULL DEFAULT false,
    CHECK (code = lower(code) AND code ~ '^[a-z][a-z0-9_]*$')
);

CREATE TABLE IF NOT EXISTS sale_evidence (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    public_id uuid NOT NULL DEFAULT gen_random_uuid() UNIQUE,
    source_code text NOT NULL REFERENCES price_sources(code),
    external_event_key text NOT NULL,
    sku_id bigint NOT NULL REFERENCES skus(id),
    variant_code text NOT NULL,
    currency_code text NOT NULL REFERENCES currencies(code),
    gross_microcredits bigint NOT NULL,
    source_timestamp timestamptz NOT NULL,
    received_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    validity_reason_code text NOT NULL REFERENCES sale_validity_reasons(code),
    normalized_payload_hash bytea NOT NULL,
    UNIQUE (source_code, external_event_key),
    CHECK (variant_code = 'normal'),
    CHECK (gross_microcredits > 0),
    CHECK (octet_length(normalized_payload_hash) = 32)
);

CREATE INDEX IF NOT EXISTS sale_evidence_recent_valid_idx
    ON sale_evidence (sku_id, source_timestamp DESC, id)
    INCLUDE (gross_microcredits, currency_code, validity_reason_code);

CREATE TABLE IF NOT EXISTS price_daily_aggregates (
    sku_id bigint NOT NULL REFERENCES skus(id),
    source_code text NOT NULL REFERENCES price_sources(code),
    aggregate_date date NOT NULL,
    valid_sale_count integer NOT NULL,
    gross_notional_microcredits numeric(30,0) NOT NULL,
    trimmed_mean_microcredits bigint,
    dispersion_ratio numeric(12,8),
    computed_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    PRIMARY KEY (sku_id, source_code, aggregate_date),
    CHECK (valid_sale_count >= 0),
    CHECK (gross_notional_microcredits >= 0),
    CHECK (trimmed_mean_microcredits IS NULL OR trimmed_mean_microcredits > 0),
    CHECK (dispersion_ratio IS NULL OR dispersion_ratio >= 0)
);

CREATE TABLE IF NOT EXISTS valuation_snapshots (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    public_id uuid NOT NULL DEFAULT gen_random_uuid() UNIQUE,
    parent_snapshot_id bigint REFERENCES valuation_snapshots(id),
    formula_version text NOT NULL,
    snapshot_at timestamptz NOT NULL,
    created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    published_at timestamptz,
    CHECK (formula_version <> ''),
    CHECK (published_at IS NULL OR published_at >= created_at)
);

CREATE TABLE IF NOT EXISTS valuation_snapshot_items (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    snapshot_id bigint NOT NULL REFERENCES valuation_snapshots(id),
    sku_id bigint NOT NULL REFERENCES skus(id),
    verified_price_microcredits bigint NOT NULL,
    source_code text NOT NULL REFERENCES price_sources(code),
    window_days smallint NOT NULL,
    valid_sale_count integer NOT NULL,
    evidence_cutoff_at timestamptz NOT NULL,
    evidence_digest bytea NOT NULL,
    UNIQUE (snapshot_id, sku_id),
    CHECK (verified_price_microcredits > 0),
    CHECK (window_days IN (7, 30)),
    CHECK (valid_sale_count >= 20),
    CHECK (octet_length(evidence_digest) = 32)
);

CREATE TABLE IF NOT EXISTS current_valuations (
    sku_id bigint PRIMARY KEY REFERENCES skus(id),
    snapshot_id bigint NOT NULL REFERENCES valuation_snapshots(id),
    snapshot_item_id bigint NOT NULL UNIQUE REFERENCES valuation_snapshot_items(id),
    verified_price_microcredits bigint NOT NULL,
    updated_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    CHECK (verified_price_microcredits > 0)
);

CREATE TABLE IF NOT EXISTS price_halts (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    public_id uuid NOT NULL DEFAULT gen_random_uuid() UNIQUE,
    sku_id bigint NOT NULL REFERENCES skus(id),
    reason_code text NOT NULL REFERENCES price_halt_reasons(code),
    observed_ratio numeric(12,8),
    halted_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    lifted_at timestamptz,
    lifted_by_critical_action_id bigint REFERENCES critical_actions(id),
    CHECK (observed_ratio IS NULL OR observed_ratio >= 0),
    CHECK (lifted_at IS NULL OR lifted_at >= halted_at)
);

CREATE UNIQUE INDEX IF NOT EXISTS price_halts_active_sku_key
    ON price_halts (sku_id) WHERE lifted_at IS NULL;

CREATE TABLE IF NOT EXISTS stock_policy_versions (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    public_id uuid NOT NULL DEFAULT gen_random_uuid() UNIQUE,
    version integer NOT NULL UNIQUE,
    created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    activated_at timestamptz,
    retired_at timestamptz,
    CHECK (version > 0),
    CHECK (retired_at IS NULL OR activated_at IS NOT NULL),
    CHECK (retired_at IS NULL OR retired_at > activated_at)
);

CREATE TABLE IF NOT EXISTS stock_policy_bands (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    stock_policy_version_id bigint NOT NULL REFERENCES stock_policy_versions(id),
    rarity_code text NOT NULL REFERENCES rarities(code),
    minimum_units integer NOT NULL,
    target_units integer NOT NULL,
    maximum_units integer NOT NULL,
    UNIQUE (stock_policy_version_id, rarity_code),
    CHECK (minimum_units >= 0),
    CHECK (minimum_units <= target_units AND target_units <= maximum_units)
);

CREATE TABLE IF NOT EXISTS warehouse_stock (
    sku_id bigint PRIMARY KEY REFERENCES skus(id),
    available_units integer NOT NULL DEFAULT 0,
    reserved_units integer NOT NULL DEFAULT 0,
    version bigint NOT NULL DEFAULT 0,
    updated_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    CHECK (available_units >= 0),
    CHECK (reserved_units >= 0),
    CHECK (reserved_units <= available_units),
    CHECK (version >= 0)
);

CREATE INDEX IF NOT EXISTS warehouse_stock_available_idx
    ON warehouse_stock (available_units DESC, sku_id)
    WHERE available_units > reserved_units;

CREATE TABLE IF NOT EXISTS risk_policy_versions (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    public_id uuid NOT NULL DEFAULT gen_random_uuid() UNIQUE,
    version integer NOT NULL UNIQUE,
    minimum_coverage_ratio numeric(12,8) NOT NULL,
    maximum_quote_reserve_ratio numeric(12,8) NOT NULL,
    maximum_item_liability_ratio numeric(12,8) NOT NULL,
    maximum_collection_liability_ratio numeric(12,8) NOT NULL,
    minimum_notional_microcredits bigint NOT NULL,
    maximum_dispersion_ratio numeric(12,8) NOT NULL,
    created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    activated_at timestamptz,
    CHECK (version > 0),
    CHECK (minimum_coverage_ratio >= 1),
    CHECK (maximum_quote_reserve_ratio BETWEEN 0 AND 1),
    CHECK (maximum_item_liability_ratio BETWEEN 0 AND 1),
    CHECK (maximum_collection_liability_ratio BETWEEN 0 AND 1),
    CHECK (minimum_notional_microcredits >= 0),
    CHECK (maximum_dispersion_ratio >= 0)
);

CREATE TABLE IF NOT EXISTS risk_state (
    singleton boolean PRIMARY KEY DEFAULT true CHECK (singleton),
    valuation_snapshot_id bigint REFERENCES valuation_snapshots(id),
    risk_policy_version_id bigint REFERENCES risk_policy_versions(id),
    liquid_reserve_microcredits bigint NOT NULL DEFAULT 0,
    stressed_liability_microcredits bigint NOT NULL DEFAULT 0,
    outstanding_quote_exposure_microcredits bigint NOT NULL DEFAULT 0,
    version bigint NOT NULL DEFAULT 0,
    updated_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    CHECK (liquid_reserve_microcredits >= 0),
    CHECK (stressed_liability_microcredits >= 0),
    CHECK (outstanding_quote_exposure_microcredits >= 0),
    CHECK (version >= 0)
);

INSERT INTO risk_state (singleton) VALUES (true)
ON CONFLICT (singleton) DO NOTHING;

CREATE TABLE IF NOT EXISTS risk_sku_exposures (
    sku_id bigint PRIMARY KEY REFERENCES skus(id),
    liability_microcredits bigint NOT NULL DEFAULT 0,
    reserved_units integer NOT NULL DEFAULT 0,
    version bigint NOT NULL DEFAULT 0,
    CHECK (liability_microcredits >= 0),
    CHECK (reserved_units >= 0)
);

CREATE TABLE IF NOT EXISTS risk_collection_exposures (
    collection_id bigint PRIMARY KEY REFERENCES collections(id),
    liability_microcredits bigint NOT NULL DEFAULT 0,
    version bigint NOT NULL DEFAULT 0,
    CHECK (liability_microcredits >= 0)
);

CREATE TABLE IF NOT EXISTS quote_signing_keys (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    public_id uuid NOT NULL DEFAULT gen_random_uuid() UNIQUE,
    algorithm text NOT NULL DEFAULT 'ed25519',
    public_key bytea NOT NULL,
    activated_at timestamptz NOT NULL,
    retired_at timestamptz,
    CHECK (algorithm = 'ed25519'),
    CHECK (octet_length(public_key) = 32),
    CHECK (retired_at IS NULL OR retired_at > activated_at)
);

CREATE TABLE IF NOT EXISTS seed_commitments (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    public_id uuid NOT NULL DEFAULT gen_random_uuid() UNIQUE,
    sequence_number bigint NOT NULL UNIQUE,
    commitment_hash bytea NOT NULL UNIQUE,
    encoding_version text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    CHECK (sequence_number > 0),
    CHECK (octet_length(commitment_hash) = 32),
    CHECK (encoding_version <> '')
);

CREATE TABLE IF NOT EXISTS seed_allocations (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    public_id uuid NOT NULL DEFAULT gen_random_uuid() UNIQUE,
    commitment_id bigint NOT NULL UNIQUE REFERENCES seed_commitments(id),
    user_id bigint NOT NULL REFERENCES users(id),
    allocated_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    expires_at timestamptz NOT NULL,
    released_at timestamptz,
    CHECK (expires_at > allocated_at),
    CHECK (expires_at <= allocated_at + interval '15 seconds'),
    CHECK (released_at IS NULL OR released_at >= allocated_at)
);

CREATE UNIQUE INDEX IF NOT EXISTS seed_allocations_active_user_key
    ON seed_allocations (user_id) WHERE released_at IS NULL;

CREATE TABLE IF NOT EXISTS seed_commitment_events (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    public_id uuid NOT NULL DEFAULT gen_random_uuid() UNIQUE,
    commitment_id bigint NOT NULL REFERENCES seed_commitments(id),
    allocation_id bigint REFERENCES seed_allocations(id),
    event_kind_code text NOT NULL REFERENCES seed_event_kinds(code),
    event_payload jsonb NOT NULL DEFAULT '{}'::jsonb,
    occurred_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    previous_event_hash bytea,
    event_hash bytea NOT NULL UNIQUE,
    CHECK (jsonb_typeof(event_payload) = 'object'),
    CHECK (previous_event_hash IS NULL OR octet_length(previous_event_hash) = 32),
    CHECK (octet_length(event_hash) = 32)
);

CREATE INDEX IF NOT EXISTS seed_commitment_events_chain_idx
    ON seed_commitment_events (id, event_hash);

CREATE TABLE IF NOT EXISTS seed_revelation_events (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    public_id uuid NOT NULL DEFAULT gen_random_uuid() UNIQUE,
    commitment_id bigint NOT NULL UNIQUE REFERENCES seed_commitments(id),
    server_seed bytea NOT NULL,
    reveal_reason_code text NOT NULL,
    revealed_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    previous_event_hash bytea,
    event_hash bytea NOT NULL UNIQUE,
    CHECK (octet_length(server_seed) = 32),
    CHECK (reveal_reason_code ~ '^[a-z][a-z0-9_]*$'),
    CHECK (previous_event_hash IS NULL OR octet_length(previous_event_hash) = 32),
    CHECK (octet_length(event_hash) = 32)
);

CREATE TABLE IF NOT EXISTS seed_daily_roots (
    root_date date PRIMARY KEY,
    last_event_id bigint NOT NULL,
    root_hash bytea NOT NULL,
    exported_at timestamptz,
    external_reference text,
    CHECK (last_event_id > 0),
    CHECK (octet_length(root_hash) = 32),
    CHECK ((exported_at IS NULL) = (external_reference IS NULL))
);

CREATE TABLE IF NOT EXISTS tradeup_quotes (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    public_id uuid NOT NULL DEFAULT gen_random_uuid() UNIQUE,
    user_id bigint NOT NULL REFERENCES users(id),
    allocation_id bigint NOT NULL UNIQUE REFERENCES seed_allocations(id),
    commitment_id bigint NOT NULL UNIQUE REFERENCES seed_commitments(id),
    valuation_snapshot_id bigint NOT NULL REFERENCES valuation_snapshots(id),
    stock_policy_version_id bigint NOT NULL REFERENCES stock_policy_versions(id),
    risk_policy_version_id bigint NOT NULL REFERENCES risk_policy_versions(id),
    signing_key_id bigint NOT NULL REFERENCES quote_signing_keys(id),
    status_code text NOT NULL REFERENCES quote_statuses(code),
    formula_version text NOT NULL,
    client_seed bytea NOT NULL,
    nonce bigint NOT NULL,
    verified_input_value_microcredits bigint NOT NULL,
    expected_buyback_microcredits bigint NOT NULL,
    quote_total_microcredits bigint NOT NULL,
    adjustment_microcredits bigint NOT NULL,
    maximum_exposure_microcredits bigint NOT NULL,
    ordered_outcome_digest bytea NOT NULL,
    signature bytea NOT NULL,
    selected_outcome_position smallint,
    created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    expires_at timestamptz NOT NULL,
    CHECK (nonce >= 0),
    CHECK (verified_input_value_microcredits >= 0),
    CHECK (expected_buyback_microcredits >= 0),
    CHECK (quote_total_microcredits >= 0),
    CHECK (maximum_exposure_microcredits >= 0),
    CHECK (octet_length(ordered_outcome_digest) = 32),
    CHECK (octet_length(signature) = 64),
    CHECK (selected_outcome_position IS NULL OR selected_outcome_position > 0),
    CHECK (expires_at > created_at),
    CHECK (expires_at <= created_at + interval '60 seconds')
);

CREATE INDEX IF NOT EXISTS tradeup_quotes_expiry_idx
    ON tradeup_quotes (expires_at, id);

CREATE UNIQUE INDEX IF NOT EXISTS tradeup_quotes_active_user_key
    ON tradeup_quotes (user_id) WHERE status_code = 'active';

CREATE TABLE IF NOT EXISTS user_active_operations (
    user_id bigint PRIMARY KEY REFERENCES users(id),
    operation_kind text NOT NULL,
    operation_id bigint NOT NULL,
    expires_at timestamptz NOT NULL,
    CHECK (operation_kind IN ('allocation', 'quote'))
);

CREATE OR REPLACE FUNCTION claim_seed_allocation()
RETURNS trigger
LANGUAGE plpgsql
SET search_path = pg_catalog
AS $function$
BEGIN
    UPDATE public.seed_allocations
       SET released_at = clock_timestamp()
     WHERE user_id = NEW.user_id
       AND released_at IS NULL
       AND expires_at <= clock_timestamp();
    DELETE FROM public.user_active_operations
     WHERE user_id = NEW.user_id AND expires_at <= clock_timestamp();
    INSERT INTO public.user_active_operations
        (user_id, operation_kind, operation_id, expires_at)
    VALUES (NEW.user_id, 'allocation', NEW.id, NEW.expires_at);
    RETURN NEW;
END;
$function$;

DROP TRIGGER IF EXISTS seed_allocations_claim_user ON seed_allocations;
CREATE TRIGGER seed_allocations_claim_user
BEFORE INSERT ON seed_allocations
FOR EACH ROW EXECUTE FUNCTION claim_seed_allocation();

CREATE OR REPLACE FUNCTION promote_allocation_to_quote()
RETURNS trigger
LANGUAGE plpgsql
SET search_path = pg_catalog
AS $function$
BEGIN
    IF NEW.status_code <> 'active' THEN RETURN NEW; END IF;
    UPDATE public.user_active_operations
       SET operation_kind = 'quote', operation_id = NEW.id,
           expires_at = NEW.expires_at
     WHERE user_id = NEW.user_id
       AND operation_kind = 'allocation'
       AND operation_id = NEW.allocation_id;
    IF NOT FOUND THEN
        INSERT INTO public.user_active_operations
            (user_id, operation_kind, operation_id, expires_at)
        VALUES (NEW.user_id, 'quote', NEW.id, NEW.expires_at);
    END IF;
    RETURN NEW;
END;
$function$;

DROP TRIGGER IF EXISTS tradeup_quotes_promote_allocation ON tradeup_quotes;
CREATE TRIGGER tradeup_quotes_promote_allocation
AFTER INSERT ON tradeup_quotes
FOR EACH ROW EXECUTE FUNCTION promote_allocation_to_quote();

CREATE TABLE IF NOT EXISTS quote_inputs (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    quote_id bigint NOT NULL REFERENCES tradeup_quotes(id),
    position smallint NOT NULL,
    inventory_item_id bigint NOT NULL REFERENCES inventory_items(id),
    valuation_snapshot_item_id bigint NOT NULL REFERENCES valuation_snapshot_items(id),
    locked_position_version bigint NOT NULL,
    UNIQUE (quote_id, position),
    UNIQUE (quote_id, inventory_item_id),
    CHECK (position BETWEEN 1 AND 10),
    CHECK (locked_position_version > 0)
);

CREATE TABLE IF NOT EXISTS inventory_item_locks (
    inventory_item_id bigint PRIMARY KEY REFERENCES inventory_items(id),
    quote_id bigint NOT NULL REFERENCES tradeup_quotes(id),
    locked_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    expires_at timestamptz NOT NULL,
    CHECK (expires_at > locked_at)
);

CREATE INDEX IF NOT EXISTS inventory_item_locks_quote_idx
    ON inventory_item_locks (quote_id, inventory_item_id);

CREATE TABLE IF NOT EXISTS quote_outcomes (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    quote_id bigint NOT NULL REFERENCES tradeup_quotes(id),
    position smallint NOT NULL,
    sku_id bigint NOT NULL REFERENCES skus(id),
    candidate_inventory_item_id bigint NOT NULL REFERENCES inventory_items(id),
    valuation_snapshot_item_id bigint NOT NULL REFERENCES valuation_snapshot_items(id),
    probability_numerator bigint NOT NULL,
    probability_denominator bigint NOT NULL,
    output_float numeric(9,8) NOT NULL,
    buyback_microcredits bigint NOT NULL,
    is_selected boolean NOT NULL DEFAULT false,
    UNIQUE (quote_id, position),
    UNIQUE (quote_id, candidate_inventory_item_id),
    CHECK (position > 0),
    CHECK (probability_numerator > 0),
    CHECK (probability_denominator > 0),
    CHECK (probability_numerator <= probability_denominator),
    CHECK (output_float BETWEEN 0 AND 1),
    CHECK (buyback_microcredits >= 0)
);

CREATE UNIQUE INDEX IF NOT EXISTS quote_outcomes_one_selected_key
    ON quote_outcomes (quote_id) WHERE is_selected;

CREATE TABLE IF NOT EXISTS quote_candidate_reservations (
    quote_id bigint NOT NULL REFERENCES tradeup_quotes(id),
    quote_outcome_id bigint NOT NULL UNIQUE REFERENCES quote_outcomes(id),
    inventory_item_id bigint NOT NULL UNIQUE REFERENCES inventory_items(id),
    sku_id bigint NOT NULL REFERENCES skus(id),
    reserved_until timestamptz NOT NULL,
    PRIMARY KEY (quote_id, quote_outcome_id)
);

CREATE INDEX IF NOT EXISTS quote_candidate_reservations_sku_idx
    ON quote_candidate_reservations (sku_id, reserved_until);

CREATE TABLE IF NOT EXISTS quote_risk_exposures (
    quote_id bigint PRIMARY KEY REFERENCES tradeup_quotes(id),
    maximum_buyback_microcredits bigint NOT NULL,
    maximum_rebate_microcredits bigint NOT NULL,
    total_exposure_microcredits bigint NOT NULL,
    CHECK (maximum_buyback_microcredits >= 0),
    CHECK (maximum_rebate_microcredits >= 0),
    CHECK (total_exposure_microcredits =
           maximum_buyback_microcredits + maximum_rebate_microcredits)
);

CREATE TABLE IF NOT EXISTS contracts (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    public_id uuid NOT NULL DEFAULT gen_random_uuid() UNIQUE,
    quote_id bigint NOT NULL UNIQUE REFERENCES tradeup_quotes(id),
    user_id bigint NOT NULL REFERENCES users(id),
    status_code text NOT NULL REFERENCES contract_statuses(code),
    valuation_snapshot_id bigint NOT NULL REFERENCES valuation_snapshots(id),
    stock_policy_version_id bigint NOT NULL REFERENCES stock_policy_versions(id),
    formula_version text NOT NULL,
    ledger_transaction_id bigint UNIQUE REFERENCES ledger_transactions(id),
    created_at timestamptz NOT NULL DEFAULT clock_timestamp()
);

CREATE TABLE IF NOT EXISTS contract_inputs (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    contract_id bigint NOT NULL REFERENCES contracts(id),
    position smallint NOT NULL,
    inventory_item_id bigint NOT NULL REFERENCES inventory_items(id),
    valuation_snapshot_item_id bigint NOT NULL REFERENCES valuation_snapshot_items(id),
    input_float numeric(9,8) NOT NULL,
    UNIQUE (contract_id, position),
    UNIQUE (contract_id, inventory_item_id),
    CHECK (position BETWEEN 1 AND 10),
    CHECK (input_float BETWEEN 0 AND 1)
);

CREATE TABLE IF NOT EXISTS contract_outcomes (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    contract_id bigint NOT NULL UNIQUE REFERENCES contracts(id),
    quote_outcome_id bigint NOT NULL UNIQUE REFERENCES quote_outcomes(id),
    inventory_item_id bigint NOT NULL UNIQUE REFERENCES inventory_items(id),
    sku_id bigint NOT NULL REFERENCES skus(id),
    valuation_snapshot_item_id bigint NOT NULL REFERENCES valuation_snapshot_items(id),
    output_float numeric(9,8) NOT NULL,
    probability_numerator bigint NOT NULL,
    probability_denominator bigint NOT NULL,
    buyback_microcredits bigint NOT NULL,
    CHECK (output_float BETWEEN 0 AND 1),
    CHECK (probability_numerator > 0),
    CHECK (probability_denominator > 0),
    CHECK (probability_numerator <= probability_denominator),
    CHECK (buyback_microcredits >= 0)
);

CREATE TABLE IF NOT EXISTS quote_acceptance_events (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    public_id uuid NOT NULL DEFAULT gen_random_uuid() UNIQUE,
    quote_id bigint NOT NULL UNIQUE REFERENCES tradeup_quotes(id),
    contract_id bigint NOT NULL UNIQUE REFERENCES contracts(id),
    idempotency_key uuid NOT NULL UNIQUE,
    accepted_at timestamptz NOT NULL DEFAULT clock_timestamp()
);

CREATE OR REPLACE FUNCTION reject_unchanged_snapshot_item()
RETURNS trigger
LANGUAGE plpgsql
SET search_path = pg_catalog
AS $function$
DECLARE
    parent_id bigint;
BEGIN
    SELECT snapshot.parent_snapshot_id INTO parent_id
      FROM public.valuation_snapshots AS snapshot
     WHERE snapshot.id = NEW.snapshot_id;

    IF parent_id IS NOT NULL AND EXISTS (
        SELECT 1
          FROM public.valuation_snapshot_items AS previous_item
         WHERE previous_item.snapshot_id = parent_id
           AND previous_item.sku_id = NEW.sku_id
           AND previous_item.verified_price_microcredits =
               NEW.verified_price_microcredits
           AND previous_item.source_code = NEW.source_code
           AND previous_item.window_days = NEW.window_days
           AND previous_item.valid_sale_count = NEW.valid_sale_count
           AND previous_item.evidence_digest = NEW.evidence_digest
    ) THEN
        RAISE EXCEPTION 'unchanged valuation rows must not be snapshotted again'
            USING ERRCODE = '23514';
    END IF;

    RETURN NEW;
END;
$function$;

DROP TRIGGER IF EXISTS valuation_snapshot_items_changed_only
    ON valuation_snapshot_items;
CREATE TRIGGER valuation_snapshot_items_changed_only
BEFORE INSERT ON valuation_snapshot_items
FOR EACH ROW
EXECUTE FUNCTION reject_unchanged_snapshot_item();

CREATE OR REPLACE FUNCTION publish_valuation_snapshot(p_snapshot_id bigint)
RETURNS void
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog
AS $function$
DECLARE
    published_snapshot public.valuation_snapshots%ROWTYPE;
BEGIN
    PERFORM 1 FROM public.risk_state WHERE singleton FOR UPDATE;

    SELECT * INTO published_snapshot
      FROM public.valuation_snapshots
     WHERE id = p_snapshot_id
     FOR UPDATE;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'valuation snapshot does not exist'
            USING ERRCODE = '23503';
    END IF;
    IF published_snapshot.published_at IS NOT NULL THEN
        RETURN;
    END IF;

    UPDATE public.tradeup_quotes
       SET status_code = 'invalidated'
     WHERE status_code = 'active'
       AND valuation_snapshot_id <> p_snapshot_id;

    DELETE FROM public.inventory_item_locks AS item_lock
     USING public.tradeup_quotes AS quote
     WHERE item_lock.quote_id = quote.id
       AND quote.status_code = 'invalidated';

    DELETE FROM public.quote_candidate_reservations AS reservation
     USING public.tradeup_quotes AS quote
     WHERE reservation.quote_id = quote.id
       AND quote.status_code = 'invalidated';

    DELETE FROM public.quote_risk_exposures AS exposure
     USING public.tradeup_quotes AS quote
     WHERE exposure.quote_id = quote.id
       AND quote.status_code = 'invalidated';

    DELETE FROM public.user_active_operations AS active_operation
     USING public.tradeup_quotes AS quote
     WHERE active_operation.operation_kind = 'quote'
       AND active_operation.operation_id = quote.id
       AND quote.status_code = 'invalidated';

    UPDATE public.valuation_snapshots
       SET published_at = clock_timestamp()
     WHERE id = p_snapshot_id;

    INSERT INTO public.current_valuations (
        sku_id,
        snapshot_id,
        snapshot_item_id,
        verified_price_microcredits,
        updated_at
    )
    SELECT snapshot_item.sku_id,
           snapshot_item.snapshot_id,
           snapshot_item.id,
           snapshot_item.verified_price_microcredits,
           clock_timestamp()
      FROM public.valuation_snapshot_items AS snapshot_item
     WHERE snapshot_item.snapshot_id = p_snapshot_id
    ON CONFLICT (sku_id) DO UPDATE
    SET snapshot_id = EXCLUDED.snapshot_id,
        snapshot_item_id = EXCLUDED.snapshot_item_id,
        verified_price_microcredits = EXCLUDED.verified_price_microcredits,
        updated_at = EXCLUDED.updated_at;

    UPDATE public.risk_state
       SET valuation_snapshot_id = p_snapshot_id,
           outstanding_quote_exposure_microcredits = 0,
           version = version + 1,
           updated_at = clock_timestamp()
     WHERE singleton;
END;
$function$;

CREATE OR REPLACE FUNCTION finalize_contract(
    p_quote_id bigint,
    p_locked_input_ids bigint[],
    p_idempotency_key uuid
) RETURNS bigint
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog
AS $function$
DECLARE
    quote_row public.tradeup_quotes%ROWTYPE;
    selected_outcome public.quote_outcomes%ROWTYPE;
    existing_quote_id bigint;
    contract_id bigint;
    transaction_id bigint;
    system_account_id bigint;
    user_account_id bigint;
    previous_write_setting text;
BEGIN
    -- Validate cardinality before any lookup or lock. Contracts have from
    -- four through ten distinct inputs; the range is enforced again by the
    -- quote's own immutable input set below.
    IF p_locked_input_ids IS NULL OR cardinality(p_locked_input_ids) NOT BETWEEN 4 AND 10 OR
       (SELECT count(DISTINCT requested_input.input_id)
          FROM unnest(p_locked_input_ids) AS requested_input(input_id)) NOT BETWEEN 4 AND 10 THEN
        RAISE EXCEPTION 'contract finalization requires between 4 and 10 unique inputs'
            USING ERRCODE = '23514';
    END IF;

    IF p_idempotency_key IS NULL THEN
        RAISE EXCEPTION 'idempotency key is required' USING ERRCODE = '23514';
    END IF;

    PERFORM pg_advisory_xact_lock(hashtextextended(p_idempotency_key::text, 2));

    SELECT acceptance.quote_id, acceptance.contract_id
      INTO existing_quote_id, contract_id
      FROM public.quote_acceptance_events AS acceptance
     WHERE acceptance.idempotency_key = p_idempotency_key;
    IF FOUND THEN
        IF existing_quote_id <> p_quote_id THEN
            RAISE EXCEPTION 'idempotency key was reused for another quote'
                USING ERRCODE = '23505';
        END IF;
        RETURN contract_id;
    END IF;

    PERFORM 1 FROM public.risk_state WHERE singleton FOR UPDATE;

    SELECT * INTO quote_row
      FROM public.tradeup_quotes AS quote
     WHERE quote.id = p_quote_id
     FOR UPDATE;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'quote does not exist' USING ERRCODE = '23503';
    END IF;

    IF quote_row.status_code <> 'active' OR
       quote_row.expires_at <= clock_timestamp() THEN
        RAISE EXCEPTION 'quote is no longer active' USING ERRCODE = '23514';
    END IF;

    IF quote_row.valuation_snapshot_id IS DISTINCT FROM (
        SELECT valuation_snapshot_id FROM public.risk_state WHERE singleton
    ) THEN
        RAISE EXCEPTION 'quote uses a stale risk snapshot' USING ERRCODE = '23514';
    END IF;

    IF (SELECT count(*) FROM public.quote_inputs WHERE quote_id = p_quote_id) <> 10 OR
       EXISTS (
           SELECT 1 FROM public.quote_inputs AS quote_input
            WHERE quote_input.quote_id = p_quote_id
              AND NOT (quote_input.inventory_item_id = ANY (p_locked_input_ids))
       ) OR EXISTS (
           SELECT 1 FROM unnest(p_locked_input_ids) AS requested_input(id)
            WHERE NOT EXISTS (
                SELECT 1 FROM public.quote_inputs AS quote_input
                 WHERE quote_input.quote_id = p_quote_id
                   AND quote_input.inventory_item_id = requested_input.id
            )
       ) THEN
        RAISE EXCEPTION 'finalization inputs differ from the locked quote inputs'
            USING ERRCODE = '23514';
    END IF;

    IF EXISTS (
        SELECT 1 FROM public.quote_inputs AS quote_input
         LEFT JOIN public.inventory_item_locks AS item_lock
           ON item_lock.inventory_item_id = quote_input.inventory_item_id
          AND item_lock.quote_id = quote_input.quote_id
        WHERE quote_input.quote_id = p_quote_id
          AND (item_lock.inventory_item_id IS NULL OR
               item_lock.expires_at <= clock_timestamp())
    ) THEN
        RAISE EXCEPTION 'one or more quote inputs are not locked'
            USING ERRCODE = '23514';
    END IF;

    SELECT outcome.* INTO selected_outcome
      FROM public.quote_outcomes AS outcome
     WHERE outcome.quote_id = p_quote_id
       AND outcome.position = quote_row.selected_outcome_position
       AND outcome.is_selected
     FOR UPDATE;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'quote has no committed selected outcome'
            USING ERRCODE = '23514';
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM public.quote_candidate_reservations AS reservation
         WHERE reservation.quote_id = p_quote_id
           AND reservation.quote_outcome_id = selected_outcome.id
           AND reservation.inventory_item_id =
               selected_outcome.candidate_inventory_item_id
           AND reservation.reserved_until > clock_timestamp()
    ) THEN
        RAISE EXCEPTION 'selected outcome is not reserved'
            USING ERRCODE = '23514';
    END IF;

    PERFORM position.inventory_item_id
      FROM public.inventory_positions AS position
     WHERE position.inventory_item_id = ANY (
        p_locked_input_ids || selected_outcome.candidate_inventory_item_id
     )
     ORDER BY position.inventory_item_id
     FOR UPDATE;

    IF NOT EXISTS (
        SELECT 1 FROM public.inventory_positions AS output_position
         WHERE output_position.inventory_item_id =
               selected_outcome.candidate_inventory_item_id
           AND output_position.in_warehouse
    ) THEN
        RAISE EXCEPTION 'selected output is unavailable'
            USING ERRCODE = '23514';
    END IF;

    PERFORM stock.sku_id
      FROM public.warehouse_stock AS stock
     WHERE stock.sku_id IN (
        SELECT item.sku_id FROM public.inventory_items AS item
         WHERE item.id = ANY (
            p_locked_input_ids || selected_outcome.candidate_inventory_item_id
         )
     )
     ORDER BY stock.sku_id
     FOR UPDATE;

    SELECT account.id INTO system_account_id
      FROM public.ledger_accounts AS account
     WHERE account.kind_code = 'system_treasury'
       AND account.owner_user_id IS NULL
       AND account.closed_at IS NULL;
    SELECT account.id INTO user_account_id
      FROM public.ledger_accounts AS account
     WHERE account.kind_code = 'user_credit'
       AND account.owner_user_id = quote_row.user_id
       AND account.closed_at IS NULL;

    IF quote_row.adjustment_microcredits <> 0 THEN
        IF system_account_id IS NULL OR user_account_id IS NULL THEN
            RAISE EXCEPTION 'quote settlement account is missing'
                USING ERRCODE = '23503';
        END IF;
        transaction_id := public.post_ledger_transaction(
            'contract_adjustment',
            p_idempotency_key,
            jsonb_build_array(
                jsonb_build_object(
                    'account_id', user_account_id,
                    'amount_microcredits', -quote_row.adjustment_microcredits
                ),
                jsonb_build_object(
                    'account_id', system_account_id,
                    'amount_microcredits', quote_row.adjustment_microcredits
                )
            )
        );
    END IF;

    previous_write_setting := current_setting('contracter.journal_write', true);
    PERFORM set_config('contracter.journal_write', 'on', true);

    BEGIN
        INSERT INTO public.contracts (
            quote_id,
            user_id,
            status_code,
            valuation_snapshot_id,
            stock_policy_version_id,
            formula_version,
            ledger_transaction_id
        ) VALUES (
            quote_row.id,
            quote_row.user_id,
            'completed',
            quote_row.valuation_snapshot_id,
            quote_row.stock_policy_version_id,
            quote_row.formula_version,
            transaction_id
        ) RETURNING id INTO contract_id;

        INSERT INTO public.contract_inputs (
            contract_id,
            position,
            inventory_item_id,
            valuation_snapshot_item_id,
            input_float
        )
        SELECT contract_id,
               quote_input.position,
               quote_input.inventory_item_id,
               quote_input.valuation_snapshot_item_id,
               item.canonical_float
          FROM public.quote_inputs AS quote_input
          JOIN public.inventory_items AS item
            ON item.id = quote_input.inventory_item_id
         WHERE quote_input.quote_id = p_quote_id
         ORDER BY quote_input.position;

        INSERT INTO public.contract_outcomes (
            contract_id,
            quote_outcome_id,
            inventory_item_id,
            sku_id,
            valuation_snapshot_item_id,
            output_float,
            probability_numerator,
            probability_denominator,
            buyback_microcredits
        ) VALUES (
            contract_id,
            selected_outcome.id,
            selected_outcome.candidate_inventory_item_id,
            selected_outcome.sku_id,
            selected_outcome.valuation_snapshot_item_id,
            selected_outcome.output_float,
            selected_outcome.probability_numerator,
            selected_outcome.probability_denominator,
            selected_outcome.buyback_microcredits
        );

        INSERT INTO public.quote_acceptance_events (
            quote_id,
            contract_id,
            idempotency_key
        ) VALUES (p_quote_id, contract_id, p_idempotency_key);

        INSERT INTO public.inventory_transfer_events (
            inventory_item_id,
            event_kind_code,
            from_user_id,
            from_warehouse,
            to_user_id,
            to_warehouse,
            operation_public_id,
            event_hash
        )
        SELECT item.id,
               'contract_input',
               quote_row.user_id,
               false,
               NULL,
               true,
               quote_row.public_id,
               public.digest(convert_to(
                   'contract_input:' || contract_id::text || ':' || item.id::text,
                   'UTF8'
               ), 'sha256')
          FROM public.inventory_items AS item
         WHERE item.id = ANY (p_locked_input_ids)
        UNION ALL
        SELECT selected_outcome.candidate_inventory_item_id,
               'contract_output',
               NULL,
               true,
               quote_row.user_id,
               false,
               quote_row.public_id,
               public.digest(convert_to(
                   'contract_output:' || contract_id::text || ':' ||
                   selected_outcome.candidate_inventory_item_id::text,
                   'UTF8'
               ), 'sha256');
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

    UPDATE public.inventory_positions
       SET owner_user_id = NULL,
           in_warehouse = true,
           version = version + 1,
           updated_at = clock_timestamp()
     WHERE inventory_item_id = ANY (p_locked_input_ids);

    UPDATE public.inventory_positions
       SET owner_user_id = quote_row.user_id,
           in_warehouse = false,
           version = version + 1,
           updated_at = clock_timestamp()
     WHERE inventory_item_id = selected_outcome.candidate_inventory_item_id;

    WITH returned_stock AS (
        SELECT item.sku_id, count(*)::integer AS units
          FROM public.inventory_items AS item
         WHERE item.id = ANY (p_locked_input_ids)
         GROUP BY item.sku_id
    )
    UPDATE public.warehouse_stock AS stock
       SET available_units = stock.available_units + returned_stock.units,
           version = stock.version + 1,
           updated_at = clock_timestamp()
      FROM returned_stock
     WHERE stock.sku_id = returned_stock.sku_id;

    UPDATE public.warehouse_stock
       SET available_units = available_units - 1,
           reserved_units = reserved_units - 1,
           version = version + 1,
           updated_at = clock_timestamp()
     WHERE sku_id = selected_outcome.sku_id
       AND available_units > 0
       AND reserved_units > 0;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'reserved output stock is unavailable'
            USING ERRCODE = '23514';
    END IF;

    DELETE FROM public.inventory_item_locks WHERE quote_id = p_quote_id;
    DELETE FROM public.quote_candidate_reservations WHERE quote_id = p_quote_id;
    DELETE FROM public.quote_risk_exposures WHERE quote_id = p_quote_id;
    DELETE FROM public.user_active_operations
     WHERE user_id = quote_row.user_id
       AND operation_kind = 'quote'
       AND operation_id = p_quote_id;

    UPDATE public.tradeup_quotes
       SET status_code = 'accepted'
     WHERE id = p_quote_id;

    UPDATE public.seed_allocations
       SET released_at = COALESCE(released_at, clock_timestamp())
     WHERE id = quote_row.allocation_id;

    UPDATE public.risk_state
       SET outstanding_quote_exposure_microcredits =
               outstanding_quote_exposure_microcredits -
               quote_row.maximum_exposure_microcredits,
           version = version + 1,
           updated_at = clock_timestamp()
     WHERE singleton
       AND outstanding_quote_exposure_microcredits >=
           quote_row.maximum_exposure_microcredits;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'risk exposure projection is inconsistent'
            USING ERRCODE = '23514';
    END IF;

    RETURN contract_id;
END;
$function$;

REVOKE ALL ON FUNCTION publish_valuation_snapshot(bigint) FROM PUBLIC;
REVOKE ALL ON FUNCTION finalize_contract(bigint, bigint[], uuid) FROM PUBLIC;
