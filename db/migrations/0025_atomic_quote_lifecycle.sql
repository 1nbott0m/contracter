-- Application supplies authenticated identity and a signed, server-computed
-- quote. SQL rechecks mutable ownership, stock and risk under one global risk
-- lock. This is the same lock ordering used by acceptance and repricing.
-- The marker distinguishes reservations actually added by this writer from
-- historical quotes, and is consumed exactly once by every terminal path.
CREATE TABLE IF NOT EXISTS atomic_quote_reservations (
    quote_id bigint PRIMARY KEY REFERENCES tradeup_quotes(id)
);
REVOKE ALL ON atomic_quote_reservations FROM PUBLIC;

CREATE OR REPLACE FUNCTION create_quote_for_user(
    p_user_id bigint, p_allocation_public_id uuid, p_quote jsonb,
    p_inputs jsonb, p_outcomes jsonb
) RETURNS uuid
LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog
AS $function$
DECLARE
    allocation public.seed_allocations%ROWTYPE;
    q public.tradeup_quotes%ROWTYPE;
    risk public.risk_state%ROWTYPE;
    policy public.risk_policy_versions%ROWTYPE;
    input_ids bigint[];
    candidate_ids bigint[];
    maximum_buyback bigint;
    rebate bigint;
    r record;
BEGIN
    IF p_user_id IS NULL OR p_allocation_public_id IS NULL OR
       jsonb_typeof(p_quote) IS DISTINCT FROM 'object' OR
       jsonb_typeof(p_inputs) IS DISTINCT FROM 'array' OR
       jsonb_typeof(p_outcomes) IS DISTINCT FROM 'array' THEN
        RAISE EXCEPTION 'invalid quote payload' USING ERRCODE='23514';
    END IF;
    IF jsonb_array_length(p_inputs) NOT BETWEEN 4 AND 10 OR
       jsonb_array_length(p_outcomes) NOT BETWEEN 1 AND 32767 THEN
        RAISE EXCEPTION 'invalid quote cardinality' USING ERRCODE='23514';
    END IF;
    SELECT array_agg(x.inventory_item_id) INTO input_ids
      FROM jsonb_to_recordset(p_inputs) AS x(inventory_item_id bigint);
    SELECT array_agg(x.candidate_inventory_item_id) INTO candidate_ids
      FROM jsonb_to_recordset(p_outcomes) AS x(candidate_inventory_item_id bigint);
    IF (SELECT count(DISTINCT id) FROM unnest(input_ids) AS x(id))<>cardinality(input_ids) OR
       (SELECT count(DISTINCT id) FROM unnest(candidate_ids) AS x(id))<>cardinality(candidate_ids) OR
       input_ids && candidate_ids THEN
        RAISE EXCEPTION 'quote requires unique distinct inputs and candidates' USING ERRCODE='23514';
    END IF;

    SELECT * INTO risk FROM public.risk_state WHERE singleton FOR UPDATE;
    PERFORM pg_advisory_xact_lock(p_user_id);
    SELECT * INTO allocation FROM public.seed_allocations
      WHERE public_id=p_allocation_public_id FOR UPDATE;
    IF NOT FOUND OR allocation.user_id IS DISTINCT FROM p_user_id THEN
        RAISE EXCEPTION 'allocation does not belong to calling user' USING ERRCODE='42501';
    END IF;
    IF allocation.released_at IS NOT NULL OR allocation.expires_at<=clock_timestamp() OR
       NOT EXISTS(SELECT 1 FROM public.user_active_operations
           WHERE user_id=p_user_id AND operation_kind='allocation' AND operation_id=allocation.id) THEN
        RAISE EXCEPTION 'allocation is no longer active' USING ERRCODE='23514';
    END IF;
    q := jsonb_populate_record(NULL::public.tradeup_quotes,
        p_quote - 'client_seed' - 'ordered_outcome_digest' - 'signature');
    q.client_seed := decode(p_quote->>'client_seed','hex');
    q.ordered_outcome_digest := decode(p_quote->>'ordered_outcome_digest','hex');
    q.signature := decode(p_quote->>'signature','hex');
    IF q.public_id IS NULL OR q.created_at IS NULL OR q.expires_at IS NULL OR
       q.created_at > clock_timestamp() OR q.expires_at<=clock_timestamp() OR
       q.selected_outcome_position IS NULL OR
       q.selected_outcome_position NOT BETWEEN 1 AND cardinality(candidate_ids) OR
       q.valuation_snapshot_id IS DISTINCT FROM risk.valuation_snapshot_id OR
       q.risk_policy_version_id IS DISTINCT FROM risk.risk_policy_version_id OR
       q.formula_version IS NULL OR q.formula_version='' OR
       q.client_seed IS NULL OR octet_length(q.client_seed) NOT BETWEEN 1 AND 1024 THEN
        RAISE EXCEPTION 'stale or invalid quote parameters' USING ERRCODE='23514';
    END IF;
    SELECT * INTO policy FROM public.risk_policy_versions
      WHERE id=q.risk_policy_version_id AND activated_at<=clock_timestamp();
    IF NOT FOUND OR NOT EXISTS(SELECT 1 FROM public.valuation_snapshots
           WHERE id=q.valuation_snapshot_id AND published_at IS NOT NULL) OR
       NOT EXISTS(SELECT 1 FROM public.stock_policy_versions
           WHERE id=q.stock_policy_version_id AND activated_at<=clock_timestamp() AND retired_at IS NULL) OR
       NOT EXISTS(SELECT 1 FROM public.quote_signing_keys
           WHERE id=q.signing_key_id AND activated_at<=clock_timestamp() AND retired_at IS NULL) THEN
        RAISE EXCEPTION 'quote policy or signing key is inactive' USING ERRCODE='23514';
    END IF;
    PERFORM pos.inventory_item_id FROM public.inventory_positions AS pos
      WHERE pos.inventory_item_id=ANY(input_ids || candidate_ids)
      ORDER BY pos.inventory_item_id FOR UPDATE;
    IF EXISTS(
        SELECT 1 FROM jsonb_to_recordset(p_inputs) AS x(
            inventory_item_id bigint,valuation_snapshot_item_id bigint,locked_position_version bigint)
        LEFT JOIN public.inventory_positions pos ON pos.inventory_item_id=x.inventory_item_id
        LEFT JOIN public.inventory_items item ON item.id=x.inventory_item_id
        LEFT JOIN public.valuation_snapshot_items val ON val.id=x.valuation_snapshot_item_id
        WHERE pos.owner_user_id IS DISTINCT FROM p_user_id OR pos.in_warehouse OR
          pos.version IS DISTINCT FROM x.locked_position_version OR item.retired_at IS NOT NULL OR
          val.snapshot_id IS DISTINCT FROM q.valuation_snapshot_id OR val.sku_id IS DISTINCT FROM item.sku_id OR
          EXISTS(SELECT 1 FROM public.inventory_item_locks WHERE inventory_item_id=x.inventory_item_id)
    ) THEN
        RAISE EXCEPTION 'quote inputs are unavailable or stale' USING ERRCODE='23514';
    END IF;
    IF EXISTS(
        SELECT 1 FROM jsonb_to_recordset(p_outcomes) AS x(
            sku_id bigint,candidate_inventory_item_id bigint,valuation_snapshot_item_id bigint,
            output_float numeric,buyback_microcredits bigint)
        LEFT JOIN public.inventory_positions pos ON pos.inventory_item_id=x.candidate_inventory_item_id
        LEFT JOIN public.inventory_items item ON item.id=x.candidate_inventory_item_id
        LEFT JOIN public.valuation_snapshot_items val ON val.id=x.valuation_snapshot_item_id
        LEFT JOIN public.skus sku ON sku.id=item.sku_id
        LEFT JOIN public.catalog_items catalog ON catalog.id=sku.catalog_item_id
        LEFT JOIN public.collections collection ON collection.id=catalog.collection_id
        WHERE pos.in_warehouse IS DISTINCT FROM true OR pos.owner_user_id IS NOT NULL OR
          item.sku_id IS DISTINCT FROM x.sku_id OR item.retired_at IS NOT NULL OR
          item.canonical_float IS DISTINCT FROM x.output_float OR
          val.snapshot_id IS DISTINCT FROM q.valuation_snapshot_id OR val.sku_id IS DISTINCT FROM x.sku_id OR
          sku.enabled IS DISTINCT FROM true OR catalog.enabled IS DISTINCT FROM true OR
          collection.enabled IS DISTINCT FROM true OR
          EXISTS(SELECT 1 FROM public.quote_candidate_reservations
              WHERE inventory_item_id=x.candidate_inventory_item_id) OR
          EXISTS(SELECT 1 FROM public.price_halts WHERE sku_id=x.sku_id AND lifted_at IS NULL)
    ) THEN
        RAISE EXCEPTION 'quote candidate is unavailable or stale' USING ERRCODE='23514';
    END IF;
    IF q.verified_input_value_microcredits IS DISTINCT FROM (
        SELECT sum(val.verified_price_microcredits)::bigint
        FROM jsonb_to_recordset(p_inputs) AS x(valuation_snapshot_item_id bigint)
        JOIN public.valuation_snapshot_items val ON val.id=x.valuation_snapshot_item_id
    ) THEN
        RAISE EXCEPTION 'quote input valuation mismatch' USING ERRCODE='23514';
    END IF;
    SELECT max(x.buyback_microcredits) INTO maximum_buyback
      FROM jsonb_to_recordset(p_outcomes) AS x(buyback_microcredits bigint);
    rebate := greatest(-q.adjustment_microcredits::numeric,0)::bigint;
    IF q.maximum_exposure_microcredits IS DISTINCT FROM maximum_buyback+rebate OR
       q.maximum_exposure_microcredits::numeric > risk.liquid_reserve_microcredits::numeric * policy.maximum_quote_reserve_ratio OR
       (risk.stressed_liability_microcredits::numeric+risk.outstanding_quote_exposure_microcredits+q.maximum_exposure_microcredits)*
           policy.minimum_coverage_ratio > risk.liquid_reserve_microcredits THEN
        RAISE EXCEPTION 'quote exceeds global risk capacity' USING ERRCODE='23514';
    END IF;

    -- Inputs and outcome array order are canonical signed positions.
    INSERT INTO public.tradeup_quotes(public_id,user_id,allocation_id,commitment_id,
        valuation_snapshot_id,stock_policy_version_id,risk_policy_version_id,signing_key_id,
        status_code,formula_version,client_seed,nonce,verified_input_value_microcredits,
        expected_buyback_microcredits,quote_total_microcredits,adjustment_microcredits,
        maximum_exposure_microcredits,ordered_outcome_digest,signature,selected_outcome_position,created_at,expires_at)
      VALUES(q.public_id,p_user_id,allocation.id,allocation.commitment_id,q.valuation_snapshot_id,
        q.stock_policy_version_id,q.risk_policy_version_id,q.signing_key_id,'active',q.formula_version,
        q.client_seed,q.nonce,q.verified_input_value_microcredits,q.expected_buyback_microcredits,
        q.quote_total_microcredits,q.adjustment_microcredits,q.maximum_exposure_microcredits,
        q.ordered_outcome_digest,q.signature,q.selected_outcome_position,q.created_at,q.expires_at)
      RETURNING id INTO q.id;
    INSERT INTO public.quote_inputs(quote_id,position,inventory_item_id,valuation_snapshot_item_id,locked_position_version)
      SELECT q.id,n::smallint,(x->>'inventory_item_id')::bigint,
        (x->>'valuation_snapshot_item_id')::bigint,(x->>'locked_position_version')::bigint
      FROM jsonb_array_elements(p_inputs) WITH ORDINALITY AS e(x,n);
    INSERT INTO public.inventory_item_locks(inventory_item_id,quote_id,expires_at)
      SELECT id,q.id,q.expires_at FROM unnest(input_ids) AS e(id);
    INSERT INTO public.quote_outcomes(quote_id,position,sku_id,candidate_inventory_item_id,
        valuation_snapshot_item_id,probability_numerator,probability_denominator,output_float,buyback_microcredits,is_selected)
      SELECT q.id,n::smallint,(x->>'sku_id')::bigint,(x->>'candidate_inventory_item_id')::bigint,
        (x->>'valuation_snapshot_item_id')::bigint,(x->>'probability_numerator')::bigint,
        (x->>'probability_denominator')::bigint,(x->>'output_float')::numeric,
        (x->>'buyback_microcredits')::bigint,n=q.selected_outcome_position
      FROM jsonb_array_elements(p_outcomes) WITH ORDINALITY AS e(x,n);
    INSERT INTO public.quote_candidate_reservations(quote_id,quote_outcome_id,inventory_item_id,sku_id,reserved_until)
      SELECT q.id,id,candidate_inventory_item_id,sku_id,q.expires_at
      FROM public.quote_outcomes WHERE quote_id=q.id;
    FOR r IN SELECT sku_id,count(*)::integer units,max(buyback_microcredits) liability
        FROM public.quote_outcomes WHERE quote_id=q.id GROUP BY sku_id ORDER BY sku_id LOOP
        UPDATE public.warehouse_stock SET reserved_units=reserved_units+r.units,
          version=version+1,updated_at=clock_timestamp()
          WHERE sku_id=r.sku_id AND available_units-reserved_units>=r.units;
        IF NOT FOUND THEN RAISE EXCEPTION 'insufficient candidate stock' USING ERRCODE='23514'; END IF;
        INSERT INTO public.risk_sku_exposures(sku_id) VALUES(r.sku_id) ON CONFLICT DO NOTHING;
        UPDATE public.risk_sku_exposures SET liability_microcredits=liability_microcredits+r.liability,
          reserved_units=reserved_units+r.units,version=version+1
          WHERE sku_id=r.sku_id AND liability_microcredits::numeric+r.liability <=
            risk.liquid_reserve_microcredits::numeric*policy.maximum_item_liability_ratio;
        IF NOT FOUND THEN RAISE EXCEPTION 'SKU risk capacity exceeded' USING ERRCODE='23514'; END IF;
    END LOOP;
    FOR r IN SELECT catalog.collection_id,max(outcome.buyback_microcredits) liability
        FROM public.quote_outcomes outcome JOIN public.skus sku ON sku.id=outcome.sku_id
        JOIN public.catalog_items catalog ON catalog.id=sku.catalog_item_id
        WHERE outcome.quote_id=q.id GROUP BY catalog.collection_id ORDER BY catalog.collection_id LOOP
        INSERT INTO public.risk_collection_exposures(collection_id) VALUES(r.collection_id) ON CONFLICT DO NOTHING;
        UPDATE public.risk_collection_exposures SET liability_microcredits=liability_microcredits+r.liability,version=version+1
          WHERE collection_id=r.collection_id AND liability_microcredits::numeric+r.liability <=
            risk.liquid_reserve_microcredits::numeric*policy.maximum_collection_liability_ratio;
        IF NOT FOUND THEN RAISE EXCEPTION 'collection risk capacity exceeded' USING ERRCODE='23514'; END IF;
    END LOOP;
    INSERT INTO public.quote_risk_exposures VALUES(q.id,maximum_buyback,rebate,q.maximum_exposure_microcredits);
    INSERT INTO public.atomic_quote_reservations VALUES(q.id);
    UPDATE public.risk_state SET outstanding_quote_exposure_microcredits=
      outstanding_quote_exposure_microcredits+q.maximum_exposure_microcredits,
      version=version+1,updated_at=clock_timestamp() WHERE singleton;
    UPDATE public.seed_allocations SET released_at=clock_timestamp() WHERE id=allocation.id;
    RETURN q.public_id;
END;
$function$;

-- Caller holds risk_state and quote locks. Acceptance has already consumed one
-- unit of selected stock; expiry/rejection/repricing have consumed none.
CREATE OR REPLACE FUNCTION release_atomic_quote_projections(p_quote_id bigint,p_consumed_sku_id bigint DEFAULT NULL)
RETURNS void LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog
AS $function$
DECLARE r record;
BEGIN
    DELETE FROM public.atomic_quote_reservations WHERE quote_id=p_quote_id;
    IF NOT FOUND THEN RETURN; END IF;
    FOR r IN SELECT sku_id,count(*)::integer units,max(buyback_microcredits) liability
        FROM public.quote_outcomes WHERE quote_id=p_quote_id GROUP BY sku_id ORDER BY sku_id LOOP
        UPDATE public.warehouse_stock SET reserved_units=reserved_units-r.units+
          CASE WHEN r.sku_id=p_consumed_sku_id THEN 1 ELSE 0 END,
          version=version+1,updated_at=clock_timestamp() WHERE sku_id=r.sku_id;
        UPDATE public.risk_sku_exposures SET liability_microcredits=liability_microcredits-r.liability,
          reserved_units=reserved_units-r.units,version=version+1 WHERE sku_id=r.sku_id;
        IF NOT FOUND THEN RAISE EXCEPTION 'missing SKU risk projection' USING ERRCODE='23514'; END IF;
    END LOOP;
    FOR r IN SELECT catalog.collection_id,max(outcome.buyback_microcredits) liability
        FROM public.quote_outcomes outcome JOIN public.skus sku ON sku.id=outcome.sku_id
        JOIN public.catalog_items catalog ON catalog.id=sku.catalog_item_id
        WHERE outcome.quote_id=p_quote_id GROUP BY catalog.collection_id ORDER BY catalog.collection_id LOOP
        UPDATE public.risk_collection_exposures SET liability_microcredits=liability_microcredits-r.liability,
          version=version+1 WHERE collection_id=r.collection_id;
        IF NOT FOUND THEN RAISE EXCEPTION 'missing collection risk projection' USING ERRCODE='23514'; END IF;
    END LOOP;
END;
$function$;

CREATE OR REPLACE FUNCTION release_quote_for_user(p_user_id bigint,p_quote_public_id uuid,p_reason text DEFAULT 'rejected')
RETURNS boolean LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog
AS $function$
DECLARE q public.tradeup_quotes%ROWTYPE;
BEGIN
    IF p_reason IS NULL OR p_reason NOT IN ('rejected','expired') THEN
        RAISE EXCEPTION 'invalid quote release reason' USING ERRCODE='23514';
    END IF;
    PERFORM 1 FROM public.risk_state WHERE singleton FOR UPDATE;
    SELECT * INTO q FROM public.tradeup_quotes WHERE public_id=p_quote_public_id FOR UPDATE;
    IF NOT FOUND OR q.user_id IS DISTINCT FROM p_user_id THEN
        RAISE EXCEPTION 'quote does not belong to calling user' USING ERRCODE='42501';
    END IF;
    IF q.status_code<>'active' THEN RETURN false; END IF;
    IF p_reason='expired' AND q.expires_at>clock_timestamp() THEN
        RAISE EXCEPTION 'quote has not expired' USING ERRCODE='23514';
    END IF;
    PERFORM public.release_atomic_quote_projections(q.id);
    DELETE FROM public.inventory_item_locks WHERE quote_id=q.id;
    DELETE FROM public.quote_candidate_reservations WHERE quote_id=q.id;
    DELETE FROM public.quote_risk_exposures WHERE quote_id=q.id;
    DELETE FROM public.user_active_operations WHERE user_id=q.user_id AND operation_kind='quote' AND operation_id=q.id;
    UPDATE public.risk_state SET outstanding_quote_exposure_microcredits=
      outstanding_quote_exposure_microcredits-q.maximum_exposure_microcredits,
      version=version+1,updated_at=clock_timestamp() WHERE singleton;
    UPDATE public.tradeup_quotes SET status_code=p_reason WHERE id=q.id;
    UPDATE public.seed_allocations SET released_at=COALESCE(released_at,clock_timestamp()) WHERE id=q.allocation_id;
    RETURN true;
END;
$function$;

CREATE OR REPLACE FUNCTION expire_quotes(p_limit integer DEFAULT 100)
RETURNS integer LANGUAGE plpgsql SECURITY DEFINER SET search_path=pg_catalog
AS $function$
DECLARE q record; expired_count integer:=0;
BEGIN
    IF p_limit IS NULL OR p_limit NOT BETWEEN 1 AND 1000 THEN
        RAISE EXCEPTION 'invalid expiry batch size' USING ERRCODE='23514';
    END IF;
    PERFORM 1 FROM public.risk_state WHERE singleton FOR UPDATE;
    FOR q IN SELECT user_id,public_id FROM public.tradeup_quotes
        WHERE status_code='active' AND expires_at<=clock_timestamp() ORDER BY id LIMIT p_limit LOOP
        IF public.release_quote_for_user(q.user_id,q.public_id,'expired') THEN expired_count:=expired_count+1; END IF;
    END LOOP;
    RETURN expired_count;
END;
$function$;

-- Preserve the deployed acceptance/repricing functions and their audit and
-- journal guards. Fail migration if their expected insertion points drift.
DO $block$
DECLARE definition text; updated text;
BEGIN
    SELECT pg_get_functiondef('public.finalize_contract(bigint,bigint[],uuid)'::regprocedure) INTO definition;
    updated:=definition;
    IF position('release_atomic_quote_projections' IN definition)=0 THEN
        updated:=replace(definition,
          '    DELETE FROM public.inventory_item_locks WHERE quote_id = p_quote_id;',
          '    PERFORM public.release_atomic_quote_projections(p_quote_id, selected_outcome.sku_id);' || chr(10) ||
          '    DELETE FROM public.inventory_item_locks WHERE quote_id = p_quote_id;');
        IF updated=definition THEN RAISE EXCEPTION 'finalize_contract lifecycle patch point missing'; END IF;
    END IF;
    -- Equal cardinality is required, not merely a distinct count within range.
    updated:=replace(updated,
      'requested_input(input_id)) NOT BETWEEN 4 AND 10',
      'requested_input(input_id)) <> cardinality(p_locked_input_ids)');
    IF position('quote inputs changed after reservation' IN updated)=0 THEN
        definition:=updated;
        updated:=replace(updated,
          '    IF NOT EXISTS (' || chr(10) || '        SELECT 1 FROM public.inventory_positions AS output_position',
          '    IF EXISTS (' || chr(10) ||
          '        SELECT 1 FROM public.quote_inputs AS input' || chr(10) ||
          '        LEFT JOIN public.inventory_positions AS position ON position.inventory_item_id=input.inventory_item_id' || chr(10) ||
          '        WHERE input.quote_id=p_quote_id AND (' || chr(10) ||
          '          position.owner_user_id IS DISTINCT FROM quote_row.user_id OR position.in_warehouse OR' || chr(10) ||
          '          position.version IS DISTINCT FROM input.locked_position_version)' || chr(10) ||
          '    ) THEN RAISE EXCEPTION ''quote inputs changed after reservation'' USING ERRCODE=''23514''; END IF;' || chr(10) ||
          '    IF NOT EXISTS (' || chr(10) || '        SELECT 1 FROM public.inventory_positions AS output_position');
        IF updated=definition THEN RAISE EXCEPTION 'finalize_contract ownership patch point missing'; END IF;
    END IF;
    EXECUTE updated;
    SELECT pg_get_functiondef('public.publish_valuation_snapshot(bigint)'::regprocedure) INTO definition;
    IF position('release_atomic_quote_projections' IN definition)=0 THEN
      updated:=replace(definition,
      '    DELETE FROM public.inventory_item_locks AS item_lock',
      '    PERFORM public.release_atomic_quote_projections(quote.id)' || chr(10) ||
      '      FROM public.tradeup_quotes AS quote WHERE quote.status_code = ''invalidated'';' || chr(10) ||
      '    UPDATE public.seed_allocations AS allocation SET released_at=COALESCE(allocation.released_at,clock_timestamp())' || chr(10) ||
      '      FROM public.tradeup_quotes AS quote WHERE quote.allocation_id=allocation.id AND quote.status_code=''invalidated'';' || chr(10) ||
      '    DELETE FROM public.inventory_item_locks AS item_lock');
      IF updated=definition THEN RAISE EXCEPTION 'snapshot lifecycle patch point missing'; END IF;
      EXECUTE updated;
    END IF;
END;
$block$;

REVOKE ALL ON FUNCTION create_quote_for_user(bigint,uuid,jsonb,jsonb,jsonb) FROM PUBLIC;
REVOKE ALL ON FUNCTION release_atomic_quote_projections(bigint,bigint) FROM PUBLIC;
REVOKE ALL ON FUNCTION release_quote_for_user(bigint,uuid,text) FROM PUBLIC;
REVOKE ALL ON FUNCTION expire_quotes(integer) FROM PUBLIC;
DO $block$
BEGIN
    IF EXISTS(SELECT 1 FROM pg_roles WHERE rolname='contracter_runtime') THEN
        GRANT EXECUTE ON FUNCTION create_quote_for_user(bigint,uuid,jsonb,jsonb,jsonb),
          release_quote_for_user(bigint,uuid,text),expire_quotes(integer) TO contracter_runtime;
        REVOKE ALL ON atomic_quote_reservations FROM contracter_runtime;
    END IF;
END;
$block$;
