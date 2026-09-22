\set ON_ERROR_STOP on
BEGIN;
-- Real database regressions: lost ownership checks, partial reservation writes,
-- double release, and unselected-stock leakage must all break these assertions.
DO $test$
DECLARE
    u bigint; other_u bigint; c bigint; catalog bigint; sku bigint;
    snapshot bigint; valuation bigint; stock_policy bigint; risk_policy bigint; key_id bigint;
    item bigint; allocation uuid; q uuid; qid bigint; contract_id bigint;
    input_ids bigint[] := '{}'; inputs jsonb := '[]'; outcomes jsonb := '[]'; payload jsonb;
    i integer; failed boolean; idem uuid := gen_random_uuid();
    newer_snapshot bigint; newer_valuation bigint; expired_count integer; expired_again integer;
BEGIN
    INSERT INTO users(login,password_hash) VALUES ('atomic_owner','test') RETURNING id INTO u;
    INSERT INTO users(login,password_hash) VALUES ('atomic_other','test') RETURNING id INTO other_u;
    INSERT INTO collections(slug,display_name) VALUES ('atomic-lifecycle','Atomic') RETURNING id INTO c;
    INSERT INTO catalog_items(collection_id,rarity_code,stable_name,min_float,max_float)
      VALUES (c,'consumer','atomic',0,1) RETURNING id INTO catalog;
    INSERT INTO skus(catalog_item_id,wear_band_id)
      SELECT catalog,id FROM wear_bands ORDER BY id LIMIT 1 RETURNING id INTO sku;
    INSERT INTO valuation_snapshots(formula_version,snapshot_at)
      VALUES ('atomic-v1',clock_timestamp()) RETURNING id INTO snapshot;
    INSERT INTO valuation_snapshot_items(snapshot_id,sku_id,verified_price_microcredits,
        source_code,window_days,valid_sale_count,evidence_cutoff_at,evidence_digest)
      VALUES(snapshot,sku,100,'market_csgo',7,20,clock_timestamp(),decode(repeat('11',32),'hex'))
      RETURNING id INTO valuation;
    PERFORM publish_valuation_snapshot(snapshot);
    INSERT INTO stock_policy_versions(version,activated_at)
      VALUES (3001,clock_timestamp()) RETURNING id INTO stock_policy;
    INSERT INTO risk_policy_versions(version,minimum_coverage_ratio,maximum_quote_reserve_ratio,
        maximum_item_liability_ratio,maximum_collection_liability_ratio,
        minimum_notional_microcredits,maximum_dispersion_ratio,activated_at)
      VALUES (3001,1,1,1,1,0,1,clock_timestamp()) RETURNING id INTO risk_policy;
    INSERT INTO quote_signing_keys(public_key,activated_at)
      VALUES(decode(repeat('22',32),'hex'),clock_timestamp()) RETURNING id INTO key_id;
    UPDATE risk_state SET risk_policy_version_id=risk_policy,liquid_reserve_microcredits=1000000;
    INSERT INTO warehouse_stock(sku_id,available_units) VALUES(sku,2);
    FOR i IN 1..6 LOOP
        INSERT INTO inventory_items(sku_id,canonical_float) VALUES(sku,0.02) RETURNING id INTO item;
        INSERT INTO inventory_positions(inventory_item_id,owner_user_id,in_warehouse)
          VALUES(item,CASE WHEN i<=4 THEN u END,i>4);
        IF i<=4 THEN
            input_ids := array_append(input_ids,item);
            inputs := inputs || jsonb_build_object('inventory_item_id',item,
                'valuation_snapshot_item_id',valuation,'locked_position_version',1);
        ELSE
            outcomes := outcomes || jsonb_build_object('sku_id',sku,
                'candidate_inventory_item_id',item,'valuation_snapshot_item_id',valuation,
                'probability_numerator',1,'probability_denominator',2,
                'output_float',0.02,'buyback_microcredits',100);
        END IF;
    END LOOP;
    allocation := allocate_seed_for_user(u,decode(repeat('33',32),'hex'),'v1',
        decode(repeat('44',24),'hex'),decode(repeat('55',48),'hex'));
    payload := jsonb_build_object('public_id',gen_random_uuid(),'valuation_snapshot_id',snapshot,
        'stock_policy_version_id',stock_policy,'risk_policy_version_id',risk_policy,
        'signing_key_id',key_id,'formula_version','atomic-v1','client_seed','abcd','nonce',0,
        'verified_input_value_microcredits',400,'expected_buyback_microcredits',100,
        'quote_total_microcredits',400,'adjustment_microcredits',0,
        'maximum_exposure_microcredits',100,'ordered_outcome_digest',repeat('66',32),
        'signature',repeat('77',64),'selected_outcome_position',1,
        'created_at',clock_timestamp(),'expires_at',clock_timestamp()+interval '45 seconds');

    failed := false;
    BEGIN
        PERFORM create_quote_for_user(other_u,allocation,payload,inputs,outcomes);
    EXCEPTION WHEN insufficient_privilege THEN failed := true;
    END;
    IF NOT failed THEN RAISE EXCEPTION 'foreign allocation accepted'; END IF;
    failed := false;
    BEGIN
        PERFORM create_quote_for_user(u,allocation,payload,inputs || inputs->0,outcomes);
    EXCEPTION WHEN check_violation THEN failed := true;
    END;
    IF NOT failed THEN RAISE EXCEPTION 'duplicate input accepted'; END IF;
    failed := false;
    BEGIN
        PERFORM create_quote_for_user(u,allocation,payload,inputs,
            jsonb_set(outcomes,'{1,candidate_inventory_item_id}',to_jsonb(input_ids[1])));
    EXCEPTION WHEN check_violation THEN failed := true;
    END;
    IF NOT failed THEN RAISE EXCEPTION 'nonwarehouse candidate accepted'; END IF;
    UPDATE warehouse_stock SET available_units=1 WHERE sku_id=sku;
    failed := false;
    BEGIN
        PERFORM create_quote_for_user(u,allocation,payload,inputs,outcomes);
    EXCEPTION WHEN check_violation THEN failed := true;
    END;
    IF NOT failed THEN RAISE EXCEPTION 'insufficient physical stock accepted'; END IF;
    UPDATE warehouse_stock SET available_units=2 WHERE sku_id=sku;
    UPDATE risk_state SET liquid_reserve_microcredits=99;
    failed := false;
    BEGIN
        PERFORM create_quote_for_user(u,allocation,payload,inputs,outcomes);
    EXCEPTION WHEN check_violation THEN failed := true;
    END;
    IF NOT failed THEN RAISE EXCEPTION 'insufficient global risk capacity accepted'; END IF;
    UPDATE risk_state SET liquid_reserve_microcredits=1000000;
    INSERT INTO risk_sku_exposures(sku_id,liability_microcredits) VALUES(sku,999901);
    failed := false;
    BEGIN
        PERFORM create_quote_for_user(u,allocation,payload,inputs,outcomes);
    EXCEPTION WHEN check_violation THEN failed := true;
    END;
    IF NOT failed THEN RAISE EXCEPTION 'insufficient SKU risk capacity accepted'; END IF;
    UPDATE risk_sku_exposures SET liability_microcredits=0 WHERE sku_id=sku;
    INSERT INTO risk_collection_exposures(collection_id,liability_microcredits) VALUES(c,999901);
    failed := false;
    BEGIN
        PERFORM create_quote_for_user(u,allocation,payload,inputs,outcomes);
    EXCEPTION WHEN check_violation THEN failed := true;
    END;
    IF NOT failed THEN RAISE EXCEPTION 'insufficient collection risk capacity accepted'; END IF;
    UPDATE risk_collection_exposures SET liability_microcredits=0 WHERE collection_id=c;
    IF EXISTS(SELECT 1 FROM tradeup_quotes WHERE user_id=u) OR
       EXISTS(SELECT 1 FROM inventory_item_locks WHERE inventory_item_id=ANY(input_ids)) OR
       (SELECT reserved_units FROM warehouse_stock WHERE sku_id=sku)<>0 OR
       (SELECT released_at FROM seed_allocations WHERE public_id=allocation) IS NOT NULL THEN
        RAISE EXCEPTION 'failed quote left partial writes';
    END IF;

    q := create_quote_for_user(u,allocation,payload,inputs,outcomes);
    SELECT id INTO qid FROM tradeup_quotes WHERE public_id=q;
    IF (SELECT count(*) FROM inventory_item_locks WHERE quote_id=qid)<>4 OR
       (SELECT count(*) FROM quote_candidate_reservations WHERE quote_id=qid)<>2 OR
       (SELECT reserved_units FROM warehouse_stock WHERE sku_id=sku)<>2 OR
       (SELECT outstanding_quote_exposure_microcredits FROM risk_state)<>100 OR
       (SELECT liability_microcredits FROM risk_sku_exposures WHERE sku_id=sku)<>100 OR
       (SELECT reserved_units FROM risk_sku_exposures WHERE sku_id=sku)<>2 OR
       (SELECT liability_microcredits FROM risk_collection_exposures WHERE collection_id=c)<>100 THEN
        RAISE EXCEPTION 'quote reservation projections incorrect';
    END IF;
    failed := false;
    BEGIN
        PERFORM release_quote_for_user(other_u,q,'rejected');
    EXCEPTION WHEN insufficient_privilege THEN failed := true;
    END;
    IF NOT failed THEN RAISE EXCEPTION 'foreign quote release accepted'; END IF;
    IF NOT release_quote_for_user(u,q,'rejected') OR release_quote_for_user(u,q,'rejected') THEN
        RAISE EXCEPTION 'release was not exactly once';
    END IF;
    IF (SELECT reserved_units FROM warehouse_stock WHERE sku_id=sku)<>0 OR
       (SELECT outstanding_quote_exposure_microcredits FROM risk_state)<>0 OR
       (SELECT liability_microcredits FROM risk_sku_exposures WHERE sku_id=sku)<>0 OR
       (SELECT reserved_units FROM risk_sku_exposures WHERE sku_id=sku)<>0 OR
       (SELECT liability_microcredits FROM risk_collection_exposures WHERE collection_id=c)<>0 OR
       EXISTS(SELECT 1 FROM user_active_operations WHERE user_id=u) THEN
        RAISE EXCEPTION 'release leaked reservations';
    END IF;
    allocation := allocate_seed_for_user(u,decode(repeat('89',32),'hex'),'v1',
        decode(repeat('44',24),'hex'),decode(repeat('55',48),'hex'));
    payload := payload || jsonb_build_object('public_id',gen_random_uuid());
    q := create_quote_for_user(u,allocation,payload,inputs,outcomes);
    UPDATE tradeup_quotes SET created_at=clock_timestamp()-interval '2 minutes',
        expires_at=clock_timestamp()-interval '90 seconds' WHERE public_id=q;
    expired_count := expire_quotes(10);
    expired_again := expire_quotes(10);
    IF expired_count<>1 OR expired_again<>0 OR
       (SELECT status_code FROM tradeup_quotes WHERE public_id=q)<>'expired' OR
       (SELECT reserved_units FROM warehouse_stock WHERE sku_id=sku)<>0 OR
       (SELECT outstanding_quote_exposure_microcredits FROM risk_state)<>0 OR
       (SELECT liability_microcredits FROM risk_sku_exposures WHERE sku_id=sku)<>0 OR
       (SELECT liability_microcredits FROM risk_collection_exposures WHERE collection_id=c)<>0 THEN
        RAISE EXCEPTION 'expiry leaked or double-released reservations';
    END IF;
    allocation := allocate_seed_for_user(u,decode(repeat('90',32),'hex'),'v1',
        decode(repeat('44',24),'hex'),decode(repeat('55',48),'hex'));
    payload := payload || jsonb_build_object('public_id',gen_random_uuid());
    q := create_quote_for_user(u,allocation,payload,inputs,outcomes);
    INSERT INTO valuation_snapshots(parent_snapshot_id,formula_version,snapshot_at)
      VALUES(snapshot,'atomic-v1',clock_timestamp()) RETURNING id INTO newer_snapshot;
    INSERT INTO valuation_snapshot_items(snapshot_id,sku_id,verified_price_microcredits,
        source_code,window_days,valid_sale_count,evidence_cutoff_at,evidence_digest)
      VALUES(newer_snapshot,sku,101,'market_csgo',7,20,clock_timestamp(),decode(repeat('12',32),'hex'))
      RETURNING id INTO newer_valuation;
    PERFORM publish_valuation_snapshot(newer_snapshot);
    IF (SELECT status_code FROM tradeup_quotes WHERE public_id=q)<>'invalidated' OR
       (SELECT reserved_units FROM warehouse_stock WHERE sku_id=sku)<>0 OR
       (SELECT outstanding_quote_exposure_microcredits FROM risk_state)<>0 OR
       (SELECT liability_microcredits FROM risk_sku_exposures WHERE sku_id=sku)<>0 OR
       (SELECT liability_microcredits FROM risk_collection_exposures WHERE collection_id=c)<>0 OR
       EXISTS(SELECT 1 FROM user_active_operations WHERE user_id=u) THEN
        RAISE EXCEPTION 'repricing leaked reservations';
    END IF;
    payload := payload || jsonb_build_object('valuation_snapshot_id',newer_snapshot,'verified_input_value_microcredits',404);
    SELECT jsonb_agg(x || jsonb_build_object('valuation_snapshot_item_id',newer_valuation)) INTO inputs
      FROM jsonb_array_elements(inputs) AS e(x);
    SELECT jsonb_agg(x || jsonb_build_object('valuation_snapshot_item_id',newer_valuation)) INTO outcomes
      FROM jsonb_array_elements(outcomes) AS e(x);
    allocation := allocate_seed_for_user(u,decode(repeat('88',32),'hex'),'v1',
        decode(repeat('44',24),'hex'),decode(repeat('55',48),'hex'));
    payload := payload || jsonb_build_object('public_id',gen_random_uuid());
    q := create_quote_for_user(u,allocation,payload,inputs,outcomes);
    SELECT id INTO qid FROM tradeup_quotes WHERE public_id=q;
    -- Ownership/version can change after quote creation; acceptance must
    -- recheck both under the already-held inventory position locks.
    UPDATE inventory_positions SET owner_user_id=other_u,version=version+1 WHERE inventory_item_id=input_ids[1];
    failed := false;
    BEGIN
        PERFORM finalize_contract_for_user(u,qid,input_ids,idem);
    EXCEPTION WHEN check_violation THEN failed := true;
    END;
    IF NOT failed THEN RAISE EXCEPTION 'acceptance consumed reassigned input'; END IF;
    UPDATE inventory_positions SET owner_user_id=u,version=1 WHERE inventory_item_id=input_ids[1];
    failed := false;
    BEGIN
        PERFORM finalize_contract_for_user(u,qid,input_ids || input_ids[1],idem);
    EXCEPTION WHEN check_violation THEN failed := true;
    END;
    IF NOT failed THEN RAISE EXCEPTION 'acceptance accepted duplicate input'; END IF;
    contract_id := finalize_contract_for_user(u,qid,input_ids,idem);
    IF contract_id IS NULL OR finalize_contract_for_user(u,qid,input_ids,idem)<>contract_id THEN
        RAISE EXCEPTION 'acceptance idempotency failed';
    END IF;
    IF EXISTS(SELECT 1 FROM pg_roles WHERE rolname='contracter_runtime') AND
       (has_function_privilege('contracter_runtime','public.release_atomic_quote_projections(bigint,bigint)','EXECUTE') OR
        has_table_privilege('contracter_runtime','public.atomic_quote_reservations','INSERT')) THEN
        RAISE EXCEPTION 'runtime has direct private reservation mutation permission';
    END IF;
    IF (SELECT reserved_units FROM warehouse_stock WHERE sku_id=sku)<>0 OR
       (SELECT available_units FROM warehouse_stock WHERE sku_id=sku)<>5 OR
       (SELECT outstanding_quote_exposure_microcredits FROM risk_state)<>0 OR
       (SELECT liability_microcredits FROM risk_sku_exposures WHERE sku_id=sku)<>0 OR
       (SELECT reserved_units FROM risk_sku_exposures WHERE sku_id=sku)<>0 OR
       (SELECT liability_microcredits FROM risk_collection_exposures WHERE collection_id=c)<>0 OR
       EXISTS(SELECT 1 FROM quote_candidate_reservations WHERE quote_id=qid) THEN
        RAISE EXCEPTION 'acceptance leaked unselected or risk reservations';
    END IF;
    RAISE NOTICE '003_atomic_quote_lifecycle: ok';
END;
$test$;
ROLLBACK;
