-- Runtime identities come from the authenticated application, never from the
-- HTTP payload. Only the two owner-bound entry points are granted to runtime.
CREATE TABLE IF NOT EXISTS market_purchase_events (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    public_id uuid NOT NULL DEFAULT gen_random_uuid() UNIQUE,
    user_id bigint NOT NULL REFERENCES users(id),
    sku_id bigint NOT NULL REFERENCES skus(id),
    inventory_item_id bigint NOT NULL REFERENCES inventory_items(id),
    valuation_snapshot_item_id bigint NOT NULL REFERENCES valuation_snapshot_items(id),
    amount_microcredits bigint NOT NULL CHECK (amount_microcredits > 0),
    idempotency_key uuid NOT NULL UNIQUE,
    ledger_transaction_id bigint NOT NULL UNIQUE REFERENCES ledger_transactions(id),
    occurred_at timestamptz NOT NULL DEFAULT clock_timestamp()
);

CREATE TABLE IF NOT EXISTS market_buyback_events (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    public_id uuid NOT NULL DEFAULT gen_random_uuid() UNIQUE,
    user_id bigint NOT NULL REFERENCES users(id),
    sku_id bigint NOT NULL REFERENCES skus(id),
    inventory_item_id bigint NOT NULL REFERENCES inventory_items(id),
    valuation_snapshot_item_id bigint NOT NULL REFERENCES valuation_snapshot_items(id),
    amount_microcredits bigint NOT NULL CHECK (amount_microcredits > 0),
    idempotency_key uuid NOT NULL UNIQUE,
    ledger_transaction_id bigint NOT NULL UNIQUE REFERENCES ledger_transactions(id),
    occurred_at timestamptz NOT NULL DEFAULT clock_timestamp()
);

DO $block$
DECLARE relation_name text;
BEGIN
    FOREACH relation_name IN ARRAY ARRAY['market_purchase_events', 'market_buyback_events'] LOOP
        EXECUTE format('DROP TRIGGER IF EXISTS journal_no_update_delete ON public.%I', relation_name);
        EXECUTE format('CREATE TRIGGER journal_no_update_delete BEFORE UPDATE OR DELETE ON public.%I '
            'FOR EACH STATEMENT EXECUTE FUNCTION public.reject_append_only_mutation()', relation_name);
        EXECUTE format('DROP TRIGGER IF EXISTS journal_owner_insert_only ON public.%I', relation_name);
        EXECUTE format('CREATE TRIGGER journal_owner_insert_only BEFORE INSERT ON public.%I '
            'FOR EACH STATEMENT EXECUTE FUNCTION public.require_journal_owner_insert()', relation_name);
    END LOOP;
END;
$block$;

CREATE OR REPLACE FUNCTION settle_market_item_for_user(
    p_user_id bigint, p_resource_id uuid, p_idempotency_key uuid, p_purchase boolean
) RETURNS TABLE(operation_id uuid, inventory_item_id uuid, amount_microcredits bigint)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog
AS $function$
DECLARE
    prior record;
    v_sku_id bigint;
    v_item_id bigint;
    v_item_public_id uuid;
    v_valuation_id bigint;
    v_price bigint;
    v_amount bigint;
    v_wallet bigint;
    v_treasury bigint;
    v_transaction bigint;
    v_operation uuid := gen_random_uuid();
    v_kind text;
    v_previous_hash bytea;
BEGIN
    IF p_user_id IS NULL OR p_resource_id IS NULL OR p_idempotency_key IS NULL OR p_purchase IS NULL THEN
        RAISE EXCEPTION 'invalid market request' USING ERRCODE = '23514';
    END IF;
    v_kind := CASE WHEN p_purchase THEN 'market_purchase' ELSE 'market_buyback' END;
    -- Match the generic ledger lock namespace, including cross-operation keys.
    PERFORM pg_advisory_xact_lock(hashtextextended(p_idempotency_key::text, 0));
    SELECT event.public_id, event.user_id, event.request_resource_id,
           event.item_public_id, event.amount, event.is_purchase
      INTO prior
      FROM (
          SELECT purchase.public_id, purchase.user_id, sku.public_id AS request_resource_id,
                 item.public_id AS item_public_id, purchase.amount_microcredits AS amount, true AS is_purchase
            FROM public.market_purchase_events AS purchase
            JOIN public.skus AS sku ON sku.id = purchase.sku_id
            JOIN public.inventory_items AS item ON item.id = purchase.inventory_item_id
           WHERE purchase.idempotency_key = p_idempotency_key
          UNION ALL
          SELECT buyback.public_id, buyback.user_id, item.public_id, item.public_id,
                 buyback.amount_microcredits, false
            FROM public.market_buyback_events AS buyback
            JOIN public.inventory_items AS item ON item.id = buyback.inventory_item_id
           WHERE buyback.idempotency_key = p_idempotency_key
      ) AS event;
    IF FOUND THEN
        IF prior.user_id <> p_user_id OR prior.request_resource_id <> p_resource_id OR prior.is_purchase <> p_purchase THEN
            RAISE EXCEPTION 'idempotency key was reused with another request' USING ERRCODE = '23505';
        END IF;
        RETURN QUERY SELECT prior.public_id, prior.item_public_id, prior.amount;
        RETURN;
    END IF;
    IF EXISTS (SELECT 1 FROM public.ledger_transactions WHERE idempotency_key = p_idempotency_key) THEN
        RAISE EXCEPTION 'idempotency key was reused with another operation' USING ERRCODE = '23505';
    END IF;

    -- Snapshot publication and quote settlement use this same lock first.
    -- It prevents an item reservation or publication changing underneath a trade.
    PERFORM 1 FROM public.risk_state WHERE singleton FOR UPDATE;
    PERFORM 1 FROM public.users WHERE id = p_user_id AND disabled_at IS NULL FOR SHARE;
    IF NOT FOUND THEN RAISE EXCEPTION 'active user required' USING ERRCODE = '42501'; END IF;

    IF p_purchase THEN
        SELECT sku.id INTO v_sku_id FROM public.skus AS sku WHERE sku.public_id = p_resource_id;
    ELSE
        SELECT item.id, item.public_id, item.sku_id INTO v_item_id, v_item_public_id, v_sku_id
          FROM public.inventory_items AS item JOIN public.inventory_positions AS position ON position.inventory_item_id = item.id
         WHERE item.public_id = p_resource_id AND item.retired_at IS NULL
           AND position.owner_user_id = p_user_id AND NOT position.in_warehouse
         FOR UPDATE OF position;
        IF NOT FOUND THEN RAISE EXCEPTION 'owned active item required' USING ERRCODE = '42501'; END IF;
    END IF;

    SELECT valuation.id, valuation.verified_price_microcredits INTO v_valuation_id, v_price
      FROM public.current_valuations AS current_price
      JOIN public.valuation_snapshot_items AS valuation ON valuation.id = current_price.snapshot_item_id
        AND valuation.sku_id = current_price.sku_id AND valuation.snapshot_id = current_price.snapshot_id
        AND valuation.verified_price_microcredits = current_price.verified_price_microcredits
      JOIN public.valuation_snapshots AS snapshot ON snapshot.id = valuation.snapshot_id AND snapshot.published_at IS NOT NULL
      JOIN public.price_sources AS source ON source.code = valuation.source_code AND source.enabled
      JOIN public.skus AS sku ON sku.id = valuation.sku_id AND sku.enabled
      JOIN public.catalog_items AS catalog ON catalog.id = sku.catalog_item_id AND catalog.enabled
      JOIN public.collections AS collection ON collection.id = catalog.collection_id AND collection.enabled
     WHERE current_price.sku_id = v_sku_id
       AND NOT EXISTS (SELECT 1 FROM public.price_halts WHERE sku_id = v_sku_id AND lifted_at IS NULL);
    IF NOT FOUND THEN RAISE EXCEPTION 'market price unavailable' USING ERRCODE = '23514'; END IF;
    -- numeric division chooses a result scale and can round before floor for
    -- very large integers. div() computes the positive integer quotient exactly.
    v_amount := CASE WHEN p_purchase THEN v_price ELSE div(v_price::numeric * 85, 100)::bigint END;
    IF v_amount <= 0 THEN RAISE EXCEPTION 'market settlement must be positive' USING ERRCODE = '23514'; END IF;

    IF p_purchase THEN
        SELECT item.id, item.public_id INTO v_item_id, v_item_public_id
          FROM public.inventory_items AS item
          JOIN public.inventory_positions AS position ON position.inventory_item_id = item.id
         WHERE item.sku_id = v_sku_id AND item.retired_at IS NULL AND position.in_warehouse
           AND NOT EXISTS (SELECT 1 FROM public.inventory_item_locks AS lock
                            WHERE lock.inventory_item_id = item.id AND lock.expires_at > clock_timestamp())
           AND NOT EXISTS (SELECT 1 FROM public.quote_candidate_reservations AS reservation
                            WHERE reservation.inventory_item_id = item.id AND reservation.reserved_until > clock_timestamp())
         ORDER BY item.id LIMIT 1 FOR UPDATE OF position;
        IF NOT FOUND THEN RAISE EXCEPTION 'warehouse item unavailable' USING ERRCODE = '23514'; END IF;
    ELSE
        IF EXISTS (SELECT 1 FROM public.inventory_item_locks AS lock
                    WHERE lock.inventory_item_id = v_item_id AND lock.expires_at > clock_timestamp())
           OR EXISTS (SELECT 1 FROM public.quote_candidate_reservations AS reservation
                       WHERE reservation.inventory_item_id = v_item_id AND reservation.reserved_until > clock_timestamp()) THEN
            RAISE EXCEPTION 'inventory item is locked' USING ERRCODE = '23514';
        END IF;
    END IF;
    PERFORM 1 FROM public.warehouse_stock WHERE sku_id = v_sku_id FOR UPDATE;
    IF NOT FOUND THEN RAISE EXCEPTION 'warehouse stock missing' USING ERRCODE = '23514'; END IF;
    IF p_purchase AND NOT EXISTS (SELECT 1 FROM public.warehouse_stock
                                 WHERE sku_id = v_sku_id AND available_units > reserved_units) THEN
        RAISE EXCEPTION 'warehouse stock unavailable' USING ERRCODE = '23514';
    END IF;

    SELECT id INTO v_wallet FROM public.ledger_accounts
     WHERE owner_user_id = p_user_id AND kind_code = 'user_credit' AND closed_at IS NULL;
    SELECT id INTO v_treasury FROM public.ledger_accounts
     WHERE owner_user_id IS NULL AND kind_code = 'system_treasury' AND closed_at IS NULL;
    IF v_wallet IS NULL OR v_treasury IS NULL THEN
        RAISE EXCEPTION 'market settlement account missing' USING ERRCODE = '23503';
    END IF;
    PERFORM account.id FROM public.ledger_accounts AS account
     WHERE account.id IN (v_wallet, v_treasury) ORDER BY account.id FOR UPDATE;
    IF EXISTS (SELECT 1 FROM public.ledger_accounts WHERE id IN (v_wallet, v_treasury) AND closed_at IS NOT NULL) THEN
        RAISE EXCEPTION 'market settlement account closed' USING ERRCODE = '23514';
    END IF;
    IF p_purchase AND COALESCE((SELECT balance_microcredits FROM public.ledger_balances WHERE account_id = v_wallet), 0) < v_amount THEN
        RAISE EXCEPTION 'insufficient balance' USING ERRCODE = '23514';
    END IF;
    v_transaction := public.post_ledger_transaction(v_kind, p_idempotency_key, jsonb_build_array(
        jsonb_build_object('account_id', v_wallet, 'amount_microcredits', CASE WHEN p_purchase THEN -v_amount ELSE v_amount END),
        jsonb_build_object('account_id', v_treasury, 'amount_microcredits', CASE WHEN p_purchase THEN v_amount ELSE -v_amount END)));

    UPDATE public.inventory_positions SET owner_user_id = CASE WHEN p_purchase THEN p_user_id ELSE NULL END,
        in_warehouse = NOT p_purchase, version = version + 1, updated_at = clock_timestamp()
     WHERE public.inventory_positions.inventory_item_id = v_item_id;
    UPDATE public.warehouse_stock SET available_units = available_units + CASE WHEN p_purchase THEN -1 ELSE 1 END,
        version = version + 1, updated_at = clock_timestamp() WHERE sku_id = v_sku_id;

    SELECT event_hash INTO v_previous_hash FROM public.inventory_transfer_events AS event
     WHERE event.inventory_item_id = v_item_id ORDER BY id DESC LIMIT 1;
    INSERT INTO public.inventory_transfer_events(inventory_item_id, event_kind_code,
        from_user_id, from_warehouse, to_user_id, to_warehouse, operation_public_id, previous_event_hash, event_hash)
    VALUES (v_item_id, v_kind, CASE WHEN p_purchase THEN NULL ELSE p_user_id END, p_purchase,
        CASE WHEN p_purchase THEN p_user_id ELSE NULL END, NOT p_purchase, v_operation, v_previous_hash,
        public.digest(COALESCE(v_previous_hash, ''::bytea) || convert_to(v_kind || ':' || v_operation::text || ':' ||
            v_item_id::text || ':' || p_user_id::text || ':' || v_amount::text, 'UTF8'), 'sha256'));
    IF p_purchase THEN
        INSERT INTO public.market_purchase_events(public_id, user_id, sku_id, inventory_item_id,
            valuation_snapshot_item_id, amount_microcredits, idempotency_key, ledger_transaction_id)
        VALUES (v_operation, p_user_id, v_sku_id, v_item_id, v_valuation_id, v_amount, p_idempotency_key, v_transaction);
    ELSE
        INSERT INTO public.market_buyback_events(public_id, user_id, sku_id, inventory_item_id,
            valuation_snapshot_item_id, amount_microcredits, idempotency_key, ledger_transaction_id)
        VALUES (v_operation, p_user_id, v_sku_id, v_item_id, v_valuation_id, v_amount, p_idempotency_key, v_transaction);
    END IF;
    RETURN QUERY SELECT v_operation, v_item_public_id, v_amount;
END;
$function$;

CREATE OR REPLACE FUNCTION purchase_market_item_for_user(p_user_id bigint, p_sku_id uuid, p_idempotency_key uuid)
RETURNS TABLE(operation_id uuid, inventory_item_id uuid, amount_microcredits bigint)
LANGUAGE sql SECURITY DEFINER SET search_path = pg_catalog
AS $function$ SELECT * FROM public.settle_market_item_for_user(p_user_id, p_sku_id, p_idempotency_key, true); $function$;

CREATE OR REPLACE FUNCTION buyback_market_item_for_user(p_user_id bigint, p_inventory_item_id uuid, p_idempotency_key uuid)
RETURNS TABLE(operation_id uuid, inventory_item_id uuid, amount_microcredits bigint)
LANGUAGE sql SECURITY DEFINER SET search_path = pg_catalog
AS $function$ SELECT * FROM public.settle_market_item_for_user(p_user_id, p_inventory_item_id, p_idempotency_key, false); $function$;

REVOKE ALL ON market_purchase_events, market_buyback_events FROM PUBLIC;
REVOKE ALL ON FUNCTION settle_market_item_for_user(bigint, uuid, uuid, boolean) FROM PUBLIC;
REVOKE ALL ON FUNCTION purchase_market_item_for_user(bigint, uuid, uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION buyback_market_item_for_user(bigint, uuid, uuid) FROM PUBLIC;
DO $block$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'contracter_runtime') THEN
        REVOKE ALL ON market_purchase_events, market_buyback_events FROM contracter_runtime;
        REVOKE ALL ON FUNCTION settle_market_item_for_user(bigint, uuid, uuid, boolean) FROM contracter_runtime;
        GRANT EXECUTE ON FUNCTION purchase_market_item_for_user(bigint, uuid, uuid),
            buyback_market_item_for_user(bigint, uuid, uuid) TO contracter_runtime;
    END IF;
END;
$block$;
