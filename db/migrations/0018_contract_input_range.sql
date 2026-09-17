-- Contracts accept four to ten inputs, not exactly ten.
--
-- CONTRACTER_GAME_MECHANICS.md replaces the fixed count with a range, and
-- states the old "exactly 10" rule no longer applies. crates/economy-core
-- now divides the collection weights and the output float by the
-- submitted count rather than by a constant, so the probability maths
-- already holds for any count in range; this migration moves the
-- database's own guard to match.
--
-- 0004 is published and SQLx verifies its checksum, so this replaces the
-- function rather than editing it. Only the two count checks differ:
-- every lock, every ledger posting, and the whole idempotency path are
-- reproduced unaltered.
--
-- What is deliberately NOT relaxed is distinctness. A bare length check
-- would let one item be passed five times and spent as though it were
-- five, so the DISTINCT count is compared against the cardinality itself.
-- contract_inputs_position_check and quote_inputs_position_check
-- (positions 1..10) still hold, because ten remains the maximum.

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
    -- Validate cardinality before any lookup or lock, so a malformed call
    -- is refused before it can take a single row lock.
    --
    -- Four to ten inclusive, and every id distinct. The DISTINCT count is
    -- compared against the raw cardinality as well: without that, a caller
    -- could pass one item five times and satisfy a bare length check while
    -- spending one item as if it were five.
    IF p_locked_input_ids IS NULL
       OR cardinality(p_locked_input_ids) NOT BETWEEN 4 AND 10
       OR (SELECT count(DISTINCT requested_input.input_id)
             FROM unnest(p_locked_input_ids) AS requested_input(input_id))
          <> cardinality(p_locked_input_ids) THEN
        RAISE EXCEPTION 'contract finalization requires four to ten unique inputs'
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

    -- The quote's own inputs must be exactly the set being spent. The two
    -- EXISTS clauses below already prove set equality in both directions;
    -- the count is compared against the caller's cardinality rather than a
    -- literal, so the accepted range lives in exactly one place.
    IF (SELECT count(*) FROM public.quote_inputs WHERE quote_id = p_quote_id)
       <> cardinality(p_locked_input_ids) OR
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

-- Re-establish the privilege boundary from 0013. CREATE OR REPLACE keeps
-- a function's existing ACL, but stating it here means this migration
-- does not silently depend on that: the identity-checked wrapper
-- finalize_contract_for_user stays the only path the runtime role has.
DO $block$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'contracter_runtime') THEN
        REVOKE EXECUTE ON FUNCTION finalize_contract(bigint, bigint[], uuid)
            FROM contracter_runtime;
    END IF;
END;
$block$;
