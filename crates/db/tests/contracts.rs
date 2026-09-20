use std::str::FromStr;

use db::{
    ContractId, ContractOutcomeId, Database, DatabaseConfig, PublicId, QuoteId,
    find_contract_by_public_id, find_contract_by_quote_id, find_contract_outcome,
    list_contract_inputs,
};
use rust_decimal::Decimal;
use sqlx::{Executor, Postgres, Transaction};
use uuid::Uuid;

async fn test_database() -> Database {
    let url = std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL must be set");
    let database = Database::connect(&DatabaseConfig::new(url).expect("valid database URL"))
        .await
        .expect("connect to isolated PostgreSQL");
    database.migrate().await.expect("apply migrations");
    database
}

async fn isolated_transaction(database: &Database) -> Transaction<'_, Postgres> {
    let mut transaction = database.begin().await.expect("begin transaction");
    transaction
        .execute("SELECT pg_advisory_xact_lock(812_202_609_37)")
        .await
        .expect("serialize contract integration fixtures");
    transaction
}

struct ContractFixture {
    id: ContractId,
    public_id: PublicId,
    quote_id: QuoteId,
    input_item_ids: [i64; 2],
    candidate_item_id: i64,
    snapshot_item_id: i64,
    sku_id: i64,
}

async fn insert_inventory_item(
    transaction: &mut Transaction<'_, Postgres>,
    sku_id: i64,
    float: &str,
) -> i64 {
    sqlx::query_scalar(
        "INSERT INTO inventory_items (sku_id, canonical_float) VALUES ($1, $2) RETURNING id",
    )
    .bind(sku_id)
    .bind(Decimal::from_str(float).expect("valid item float"))
    .fetch_one(transaction.as_mut())
    .await
    .expect("insert inventory item")
}

async fn seed_contract(transaction: &mut Transaction<'_, Postgres>) -> ContractFixture {
    transaction
        .execute(
            "INSERT INTO rarities (code, rank, is_covert) \
             VALUES ('contract_test', 94, false) ON CONFLICT (code) DO NOTHING; \
             INSERT INTO wear_bands (code, lower_bound, upper_bound, includes_upper_bound) \
             VALUES ('contract_test', 0, 1, true) ON CONFLICT (code) DO NOTHING; \
             INSERT INTO quote_statuses (code, is_terminal, description) \
             VALUES ('accepted', true, 'Accepted') ON CONFLICT (code) DO NOTHING; \
             INSERT INTO contract_statuses (code, is_terminal, description) \
             VALUES ('completed', true, 'Completed') ON CONFLICT (code) DO NOTHING; \
             INSERT INTO price_sources (code, display_name, enabled) \
             VALUES ('contract_test', 'Contract test', true) ON CONFLICT (code) DO NOTHING",
        )
        .await
        .expect("seed contract lookups");

    let suffix = Uuid::new_v4().simple().to_string();
    let user_id: i64 = sqlx::query_scalar(
        "INSERT INTO users (login, password_hash) VALUES ($1, 'argon2id-test-hash') RETURNING id",
    )
    .bind(format!("contract_user_{suffix}"))
    .fetch_one(transaction.as_mut())
    .await
    .expect("insert user");
    let collection_id: i64 = sqlx::query_scalar(
        "INSERT INTO collections (slug, display_name) VALUES ($1, 'Contract Test') RETURNING id",
    )
    .bind(format!("contract-{suffix}"))
    .fetch_one(transaction.as_mut())
    .await
    .expect("insert collection");
    let item_id: i64 = sqlx::query_scalar(
        "INSERT INTO catalog_items \
            (collection_id, rarity_code, stable_name, min_float, max_float) \
         VALUES ($1, 'contract_test', $2, 0, 1) RETURNING id",
    )
    .bind(collection_id)
    .bind(format!("contract-item-{suffix}"))
    .fetch_one(transaction.as_mut())
    .await
    .expect("insert catalog item");
    let sku_id: i64 = sqlx::query_scalar(
        "INSERT INTO skus (catalog_item_id, wear_band_id) \
         SELECT $1, id FROM wear_bands WHERE code = 'contract_test' RETURNING id",
    )
    .bind(item_id)
    .fetch_one(transaction.as_mut())
    .await
    .expect("insert SKU");

    let sequence_number = (Uuid::new_v4().as_u128() % 8_000_000_000) as i64 + 1_000_000;
    let commitment_id: i64 = sqlx::query_scalar(
        "INSERT INTO seed_commitments (sequence_number, commitment_hash, encoding_version) \
         VALUES ($1, digest($2, 'sha256'), 'contract-test-v1') RETURNING id",
    )
    .bind(sequence_number)
    .bind(&suffix)
    .fetch_one(transaction.as_mut())
    .await
    .expect("insert seed commitment");
    // clock_timestamp() is volatile and can return a different instant on
    // each call within the same statement, so allocated_at's column DEFAULT
    // and an independent `expires_at` expression can drift by a few
    // microseconds and intermittently violate the "expires_at <=
    // allocated_at + 15s" CHECK. Capture one reading and derive
    // allocated_at/expires_at from it; released_at only needs to be
    // >= allocated_at, so reusing the same reading there is exact rather
    // than merely safe.
    let allocation_id: i64 = sqlx::query_scalar(
        "WITH allocation_clock AS (SELECT clock_timestamp() AS now) \
         INSERT INTO seed_allocations \
             (commitment_id, user_id, allocated_at, expires_at, released_at) \
         SELECT $1, $2, allocation_clock.now, \
                allocation_clock.now + interval '15 seconds', allocation_clock.now \
         FROM allocation_clock \
         RETURNING id",
    )
    .bind(commitment_id)
    .bind(user_id)
    .fetch_one(transaction.as_mut())
    .await
    .expect("insert seed allocation");
    let snapshot_id: i64 = sqlx::query_scalar(
        "INSERT INTO valuation_snapshots (formula_version, snapshot_at) \
         VALUES ('contract-test-v1', clock_timestamp()) RETURNING id",
    )
    .fetch_one(transaction.as_mut())
    .await
    .expect("insert valuation snapshot");
    let snapshot_item_id: i64 = sqlx::query_scalar(
        "INSERT INTO valuation_snapshot_items ( \
            snapshot_id, sku_id, verified_price_microcredits, source_code, window_days, \
            valid_sale_count, evidence_cutoff_at, evidence_digest \
         ) VALUES ($1, $2, 1000000, 'contract_test', 7, 20, clock_timestamp(), \
                   decode(repeat('88', 32), 'hex')) RETURNING id",
    )
    .bind(snapshot_id)
    .bind(sku_id)
    .fetch_one(transaction.as_mut())
    .await
    .expect("insert snapshot item");
    sqlx::query("UPDATE valuation_snapshots SET published_at = clock_timestamp() WHERE id = $1")
        .bind(snapshot_id)
        .execute(transaction.as_mut())
        .await
        .expect("publish nonempty valuation snapshot");
    let stock_policy_id: i64 = sqlx::query_scalar(
        "INSERT INTO stock_policy_versions (version, activated_at) \
         VALUES ($1, clock_timestamp()) RETURNING id",
    )
    .bind((sequence_number % 2_000_000_000) as i32 + 30_000)
    .fetch_one(transaction.as_mut())
    .await
    .expect("insert stock policy");
    let risk_policy_id: i64 = sqlx::query_scalar(
        "INSERT INTO risk_policy_versions ( \
            version, minimum_coverage_ratio, maximum_quote_reserve_ratio, \
            maximum_item_liability_ratio, maximum_collection_liability_ratio, \
            minimum_notional_microcredits, maximum_dispersion_ratio, activated_at \
         ) VALUES ($1, 1, 0.1, 0.1, 0.2, 0, 1, clock_timestamp()) RETURNING id",
    )
    .bind((sequence_number % 2_000_000_000) as i32 + 40_000)
    .fetch_one(transaction.as_mut())
    .await
    .expect("insert risk policy");
    let signing_key_id: i64 = sqlx::query_scalar(
        "INSERT INTO quote_signing_keys (public_key, activated_at) \
         VALUES (digest($1, 'sha256'), clock_timestamp()) RETURNING id",
    )
    .bind(&suffix)
    .fetch_one(transaction.as_mut())
    .await
    .expect("insert signing key");

    // As above: created_at and expires_at each called clock_timestamp()
    // independently, so they could drift enough to violate the 60-second
    // window CHECK. Capture one reading in a CTE for both.
    let quote_public_id = PublicId::new(Uuid::new_v4());
    let quote_id: QuoteId = sqlx::query_scalar(
        "WITH quote_clock AS (SELECT clock_timestamp() AS now) \
         INSERT INTO tradeup_quotes ( \
            public_id, user_id, allocation_id, commitment_id, valuation_snapshot_id, \
            stock_policy_version_id, risk_policy_version_id, signing_key_id, status_code, \
            formula_version, client_seed, nonce, verified_input_value_microcredits, \
            expected_buyback_microcredits, quote_total_microcredits, adjustment_microcredits, \
            maximum_exposure_microcredits, ordered_outcome_digest, signature, \
            selected_outcome_position, created_at, expires_at \
         ) \
         SELECT $1, $2, $3, $4, $5, $6, $7, $8, 'accepted', 'contract-test-v1', \
                decode(repeat('99', 32), 'hex'), 3, 10000000, 8500000, 10000000, 0, 8500000, \
                decode(repeat('aa', 32), 'hex'), decode(repeat('bb', 64), 'hex'), 1, \
                quote_clock.now - interval '2 minutes', quote_clock.now - interval '1 minute' \
         FROM quote_clock \
         RETURNING id",
    )
    .bind(quote_public_id)
    .bind(user_id)
    .bind(allocation_id)
    .bind(commitment_id)
    .bind(snapshot_id)
    .bind(stock_policy_id)
    .bind(risk_policy_id)
    .bind(signing_key_id)
    .fetch_one(transaction.as_mut())
    .await
    .expect("insert trade-up quote");

    let first_input_id = insert_inventory_item(transaction, sku_id, "0.10000000").await;
    let second_input_id = insert_inventory_item(transaction, sku_id, "0.20000000").await;
    let candidate_item_id = insert_inventory_item(transaction, sku_id, "0.55555555").await;

    let quote_outcome_id: i64 = sqlx::query_scalar(
        "INSERT INTO quote_outcomes ( \
            quote_id, position, sku_id, candidate_inventory_item_id, \
            valuation_snapshot_item_id, probability_numerator, probability_denominator, \
            output_float, buyback_microcredits, is_selected \
         ) VALUES ($1, 1, $2, $3, $4, 1, 2, $5, 850000, true) RETURNING id",
    )
    .bind(quote_id)
    .bind(sku_id)
    .bind(candidate_item_id)
    .bind(snapshot_item_id)
    .bind(Decimal::from_str("0.55555555").expect("valid output float"))
    .fetch_one(transaction.as_mut())
    .await
    .expect("insert quote outcome");

    let contract_public_id = PublicId::new(Uuid::new_v4());
    let contract_id: ContractId = sqlx::query_scalar(
        "INSERT INTO contracts ( \
            public_id, quote_id, user_id, status_code, valuation_snapshot_id, \
            stock_policy_version_id, formula_version \
         ) VALUES ($1, $2, $3, 'completed', $4, $5, 'contract-test-v1') RETURNING id",
    )
    .bind(contract_public_id)
    .bind(quote_id)
    .bind(user_id)
    .bind(snapshot_id)
    .bind(stock_policy_id)
    .fetch_one(transaction.as_mut())
    .await
    .expect("insert contract");

    for (position, item_id, input_float) in [
        (2_i16, second_input_id, "0.20000000"),
        (1_i16, first_input_id, "0.10000000"),
    ] {
        sqlx::query(
            "INSERT INTO contract_inputs \
                (contract_id, position, inventory_item_id, valuation_snapshot_item_id, \
                 input_float) \
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(contract_id)
        .bind(position)
        .bind(item_id)
        .bind(snapshot_item_id)
        .bind(Decimal::from_str(input_float).expect("valid input float"))
        .execute(transaction.as_mut())
        .await
        .expect("insert contract input");
    }

    let _outcome_id: ContractOutcomeId = sqlx::query_scalar(
        "INSERT INTO contract_outcomes ( \
            contract_id, quote_outcome_id, inventory_item_id, sku_id, \
            valuation_snapshot_item_id, output_float, probability_numerator, \
            probability_denominator, buyback_microcredits \
         ) VALUES ($1, $2, $3, $4, $5, $6, 1, 2, 850000) RETURNING id",
    )
    .bind(contract_id)
    .bind(quote_outcome_id)
    .bind(candidate_item_id)
    .bind(sku_id)
    .bind(snapshot_item_id)
    .bind(Decimal::from_str("0.55555555").expect("valid output float"))
    .fetch_one(transaction.as_mut())
    .await
    .expect("insert contract outcome");

    ContractFixture {
        id: contract_id,
        public_id: contract_public_id,
        quote_id,
        input_item_ids: [first_input_id, second_input_id],
        candidate_item_id,
        snapshot_item_id,
        sku_id,
    }
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn contract_is_found_by_public_id_and_by_quote_id_and_unknown_ids_return_none() {
    let database = test_database().await;
    let mut transaction = isolated_transaction(&database).await;
    let fixture = seed_contract(&mut transaction).await;

    let by_public_id = find_contract_by_public_id(transaction.as_mut(), fixture.public_id)
        .await
        .expect("find contract by public id")
        .expect("contract exists");
    assert_eq!(by_public_id.id, fixture.id);
    assert_eq!(by_public_id.quote_id, fixture.quote_id);
    assert_eq!(by_public_id.status_code, "completed");
    assert_eq!(by_public_id.ledger_transaction_id, None);

    let by_quote_id = find_contract_by_quote_id(transaction.as_mut(), fixture.quote_id)
        .await
        .expect("find contract by quote id")
        .expect("contract exists");
    assert_eq!(by_quote_id.id, fixture.id);

    assert!(
        find_contract_by_public_id(transaction.as_mut(), PublicId::new(Uuid::new_v4()))
            .await
            .expect("find unknown contract by public id")
            .is_none()
    );
    assert!(
        find_contract_by_quote_id(transaction.as_mut(), QuoteId::new(-1))
            .await
            .expect("find unknown contract by quote id")
            .is_none()
    );

    transaction.rollback().await.expect("rollback fixture");
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn contract_inputs_are_ordered_by_position_with_exact_decimal_float() {
    let database = test_database().await;
    let mut transaction = isolated_transaction(&database).await;
    let fixture = seed_contract(&mut transaction).await;

    let inputs = list_contract_inputs(transaction.as_mut(), fixture.id)
        .await
        .expect("list contract inputs");
    assert_eq!(
        inputs
            .iter()
            .map(|input| input.position)
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert_eq!(inputs[0].inventory_item_id.get(), fixture.input_item_ids[0]);
    assert_eq!(inputs[1].inventory_item_id.get(), fixture.input_item_ids[1]);
    assert_eq!(
        inputs[0].input_float,
        Decimal::from_str("0.10000000").expect("valid decimal")
    );
    assert_eq!(
        inputs[1].input_float,
        Decimal::from_str("0.20000000").expect("valid decimal")
    );
    assert_eq!(
        inputs[0].valuation_snapshot_item_id.get(),
        fixture.snapshot_item_id
    );

    assert!(
        list_contract_inputs(transaction.as_mut(), ContractId::new(-1))
            .await
            .expect("list inputs for unknown contract")
            .is_empty()
    );

    transaction.rollback().await.expect("rollback fixture");
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn contract_outcome_is_the_single_result_with_exact_decimal_float() {
    let database = test_database().await;
    let mut transaction = isolated_transaction(&database).await;
    let fixture = seed_contract(&mut transaction).await;

    let outcome = find_contract_outcome(transaction.as_mut(), fixture.id)
        .await
        .expect("find contract outcome")
        .expect("contract outcome exists");
    assert_eq!(outcome.contract_id, fixture.id);
    assert_eq!(outcome.inventory_item_id.get(), fixture.candidate_item_id);
    assert_eq!(outcome.sku_id.get(), fixture.sku_id);
    assert_eq!(
        outcome.output_float,
        Decimal::from_str("0.55555555").expect("valid decimal")
    );
    assert_eq!(outcome.buyback_microcredits, 850_000);

    assert!(
        find_contract_outcome(transaction.as_mut(), ContractId::new(-1))
            .await
            .expect("find outcome for unknown contract")
            .is_none()
    );

    transaction.rollback().await.expect("rollback fixture");
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn runtime_role_can_execute_every_contract_read_api() {
    let database = test_database().await;
    let runtime_role_required = std::env::var_os("TEST_RUNTIME_ROLE_REQUIRED").is_some();
    let role_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'contracter_runtime')",
    )
    .fetch_one(database.pool())
    .await
    .expect("inspect runtime role");
    if !role_exists {
        assert!(
            !runtime_role_required,
            "contracter_runtime is required but does not exist"
        );
        eprintln!("contracter_runtime does not exist; runtime-role execution check skipped");
        return;
    }
    let can_set_role: bool =
        sqlx::query_scalar("SELECT pg_has_role(current_user, 'contracter_runtime', 'SET')")
            .fetch_one(database.pool())
            .await
            .expect("inspect SET ROLE membership");
    if !can_set_role {
        assert!(
            !runtime_role_required,
            "migration user cannot SET ROLE contracter_runtime"
        );
        eprintln!("migration user cannot SET ROLE contracter_runtime; execution check skipped");
        return;
    }

    let mut transaction = isolated_transaction(&database).await;
    transaction
        .execute("SET LOCAL ROLE contracter_runtime")
        .await
        .expect("assume runtime role");
    find_contract_by_public_id(transaction.as_mut(), PublicId::new(Uuid::new_v4()))
        .await
        .expect("runtime finds contract by public id");
    find_contract_by_quote_id(transaction.as_mut(), QuoteId::new(-1))
        .await
        .expect("runtime finds contract by quote id");
    let inputs = list_contract_inputs(transaction.as_mut(), ContractId::new(-1))
        .await
        .expect("runtime lists contract inputs");
    assert!(inputs.is_empty());
    let outcome = find_contract_outcome(transaction.as_mut(), ContractId::new(-1))
        .await
        .expect("runtime finds contract outcome");
    assert!(outcome.is_none());
    transaction.rollback().await.expect("rollback role check");
}
