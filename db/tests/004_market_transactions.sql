\set ON_ERROR_STOP on
BEGIN;

-- These tests catch missing transfers, duplicate settlements, stale prices,
-- foreign-owner writes, and unsafe rounding at the bigint boundary.
DO $test$
DECLARE
    buyer bigint;
    stranger bigint;
    treasury bigint;
    wallet bigint;
    collection bigint;
    catalog bigint;
    sku bigint;
    sku_public uuid := gen_random_uuid();
    item bigint;
    item_public uuid := gen_random_uuid();
    snapshot bigint;
    valuation bigint;
    purchase_key uuid := gen_random_uuid();
    buyback_key uuid := gen_random_uuid();
    bought record;
    retried record;
    sold record;
BEGIN
    UPDATE price_sources SET enabled = true WHERE code = 'market_csgo';
    INSERT INTO users(login, password_hash) VALUES ('market_test_buyer', 'test') RETURNING id INTO buyer;
    INSERT INTO users(login, password_hash) VALUES ('market_test_stranger', 'test') RETURNING id INTO stranger;
    INSERT INTO ledger_accounts(kind_code, owner_user_id) VALUES ('user_credit', buyer) RETURNING id INTO wallet;
    INSERT INTO ledger_accounts(kind_code) VALUES ('system_treasury') ON CONFLICT DO NOTHING;
    SELECT id INTO treasury FROM ledger_accounts WHERE kind_code = 'system_treasury' AND owner_user_id IS NULL;
    PERFORM post_ledger_transaction('market_fixture', gen_random_uuid(), jsonb_build_array(
        jsonb_build_object('account_id', wallet, 'amount_microcredits', 1000),
        jsonb_build_object('account_id', treasury, 'amount_microcredits', -1000)));
    INSERT INTO collections(slug, display_name) VALUES ('market-test', 'Market test') RETURNING id INTO collection;
    INSERT INTO catalog_items(collection_id, rarity_code, stable_name, min_float, max_float)
        VALUES (collection, 'mil-spec', 'Market test', 0, 1) RETURNING id INTO catalog;
    INSERT INTO skus(public_id, catalog_item_id, wear_band_id)
        SELECT sku_public, catalog, id FROM wear_bands WHERE code = 'factory_new' RETURNING id INTO sku;
    INSERT INTO inventory_items(public_id, sku_id, canonical_float)
        VALUES (item_public, sku, 0.01) RETURNING id INTO item;
    INSERT INTO inventory_positions(inventory_item_id, in_warehouse) VALUES (item, true);
    INSERT INTO warehouse_stock(sku_id, available_units) VALUES (sku, 1);
    INSERT INTO valuation_snapshots(formula_version, snapshot_at)
        VALUES ('market-test', clock_timestamp()) RETURNING id INTO snapshot;
    INSERT INTO valuation_snapshot_items(snapshot_id, sku_id, verified_price_microcredits,
        source_code, window_days, valid_sale_count, evidence_cutoff_at, evidence_digest)
        VALUES (snapshot, sku, 101, 'market_csgo', 7, 20, clock_timestamp(), digest('market-test', 'sha256'))
        RETURNING id INTO valuation;
    PERFORM publish_valuation_snapshot(snapshot);

    SELECT * INTO bought FROM purchase_market_item_for_user(buyer, sku_public, purchase_key);
    IF bought.inventory_item_id <> item_public OR bought.amount_microcredits <> 101
       OR (SELECT owner_user_id FROM inventory_positions WHERE inventory_item_id = item) <> buyer
       OR (SELECT available_units FROM warehouse_stock WHERE sku_id = sku) <> 0
       OR (SELECT balance_microcredits FROM ledger_balances WHERE account_id = wallet) <> 899 THEN
        RAISE EXCEPTION 'purchase did not atomically settle price, stock, balance and owner';
    END IF;
    SELECT * INTO retried FROM purchase_market_item_for_user(buyer, sku_public, purchase_key);
    IF retried IS DISTINCT FROM bought THEN RAISE EXCEPTION 'purchase retry changed result'; END IF;
    BEGIN
        PERFORM purchase_market_item_for_user(stranger, sku_public, purchase_key);
        RAISE EXCEPTION 'purchase key accepted another owner';
    EXCEPTION WHEN unique_violation THEN NULL; END;
    BEGIN
        PERFORM purchase_market_item_for_user(buyer, gen_random_uuid(), purchase_key);
        RAISE EXCEPTION 'purchase key accepted another SKU';
    EXCEPTION WHEN unique_violation THEN NULL; END;
    BEGIN
        PERFORM purchase_market_item_for_user(buyer, sku_public, gen_random_uuid());
        RAISE EXCEPTION 'sold-out stock could be purchased';
    EXCEPTION WHEN check_violation THEN NULL; END;
    BEGIN
        PERFORM buyback_market_item_for_user(stranger, item_public, gen_random_uuid());
        RAISE EXCEPTION 'foreign-owned item could be sold';
    EXCEPTION WHEN insufficient_privilege THEN NULL; END;
    BEGIN
        PERFORM buyback_market_item_for_user(buyer, item_public, purchase_key);
        RAISE EXCEPTION 'idempotency key reused across market actions';
    EXCEPTION WHEN unique_violation THEN NULL; END;

    SELECT * INTO sold FROM buyback_market_item_for_user(buyer, item_public, buyback_key);
    IF sold.inventory_item_id <> item_public OR sold.amount_microcredits <> 85
       OR NOT (SELECT in_warehouse FROM inventory_positions WHERE inventory_item_id = item)
       OR (SELECT available_units FROM warehouse_stock WHERE sku_id = sku) <> 1
       OR (SELECT balance_microcredits FROM ledger_balances WHERE account_id = wallet) <> 984 THEN
        RAISE EXCEPTION 'buyback did not use floor(101 * 85 / 100) and atomically settle';
    END IF;
    SELECT * INTO retried FROM buyback_market_item_for_user(buyer, item_public, buyback_key);
    IF retried IS DISTINCT FROM sold THEN RAISE EXCEPTION 'buyback retry changed result'; END IF;
    IF (SELECT count(*) FROM inventory_transfer_events WHERE inventory_item_id = item) <> 2
       OR (SELECT count(*) FROM market_purchase_events WHERE user_id = buyer) <> 1
       OR (SELECT count(*) FROM market_buyback_events WHERE user_id = buyer) <> 1 THEN
        RAISE EXCEPTION 'market retries duplicated journals';
    END IF;
    BEGIN
        UPDATE market_purchase_events SET amount_microcredits = 1 WHERE user_id = buyer;
        RAISE EXCEPTION 'purchase journal can be rewritten';
    EXCEPTION WHEN object_not_in_prerequisite_state THEN NULL; END;
    BEGIN
        DELETE FROM market_buyback_events WHERE user_id = buyer;
        RAISE EXCEPTION 'buyback journal can be deleted';
    EXCEPTION WHEN object_not_in_prerequisite_state THEN NULL; END;

    -- New published prices must be server-selected; huge multiplication must
    -- use numeric before multiplying, not overflowing bigint first.
    INSERT INTO valuation_snapshots(formula_version, snapshot_at)
        VALUES ('market-huge', clock_timestamp()) RETURNING id INTO snapshot;
    INSERT INTO valuation_snapshot_items(snapshot_id, sku_id, verified_price_microcredits,
        source_code, window_days, valid_sale_count, evidence_cutoff_at, evidence_digest)
        VALUES (snapshot, sku, 9223372036854775807, 'market_csgo', 7, 20, clock_timestamp(), digest('huge', 'sha256'));
    PERFORM publish_valuation_snapshot(snapshot);
    BEGIN
        PERFORM purchase_market_item_for_user(buyer, sku_public, gen_random_uuid());
        RAISE EXCEPTION 'insufficient balance accepted';
    EXCEPTION WHEN check_violation THEN NULL; END;
    IF (SELECT available_units FROM warehouse_stock WHERE sku_id = sku) <> 1
       OR (SELECT balance_microcredits FROM ledger_balances WHERE account_id = wallet) <> 984 THEN
        RAISE EXCEPTION 'failed purchase changed stock or funds';
    END IF;
    UPDATE inventory_positions SET owner_user_id = buyer, in_warehouse = false WHERE inventory_item_id = item;
    UPDATE warehouse_stock SET available_units = 0 WHERE sku_id = sku;
    SELECT * INTO sold FROM buyback_market_item_for_user(buyer, item_public, gen_random_uuid());
    IF sold.amount_microcredits <> 7839866231326559435 THEN RAISE EXCEPTION 'large buyback rounded incorrectly'; END IF;

    PERFORM set_config('market_test.buyer', buyer::text, true);
    PERFORM set_config('market_test.sku', sku_public::text, true);
    PERFORM set_config('market_test.purchase_key', purchase_key::text, true);
END;
$test$;

-- Execute a genuine idempotent result under the limited production role.
SET LOCAL ROLE contracter_runtime;
SELECT * FROM purchase_market_item_for_user(
    current_setting('market_test.buyer')::bigint,
    current_setting('market_test.sku')::uuid,
    current_setting('market_test.purchase_key')::uuid);
RESET ROLE;
DO $test$
BEGIN
    IF has_table_privilege('contracter_runtime', 'market_purchase_events', 'INSERT,UPDATE,DELETE,SELECT')
       OR has_table_privilege('contracter_runtime', 'market_buyback_events', 'INSERT,UPDATE,DELETE,SELECT')
       OR has_function_privilege('contracter_runtime', 'post_ledger_transaction(text,uuid,jsonb)', 'EXECUTE') THEN
        RAISE EXCEPTION 'runtime has generic market or ledger privileges';
    END IF;
END;
$test$;
SELECT '004_market_transactions: ok' AS result;
ROLLBACK;
