use db::{
    Database, DatabaseConfig, PublicId, QuoteId, UserId, find_active_quote_for_user,
    find_tradeup_quote, list_quote_inputs, list_quote_outcomes,
};
use rust_decimal::Decimal;
use sqlx::{Executor, Postgres, Transaction};
use std::str::FromStr;
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
        .execute("SELECT pg_advisory_xact_lock(812_202_609_25)")
        .await
        .expect("serialize quote integration fixtures");
    transaction
}

struct QuoteFixture {
    id: QuoteId,
    public_id: PublicId,
    user_id: UserId,
    sku_id: i64,
    snapshot_item_id: i64,
}

async fn seed_quote(
    transaction: &mut Transaction<'_, Postgres>,
    status_code: &str,
    expired: bool,
) -> QuoteFixture {
    transaction
        .execute(
            "INSERT INTO rarities (code, rank, is_covert) \
             VALUES ('quote_test', 93, false) ON CONFLICT (code) DO NOTHING; \
             INSERT INTO wear_bands (code, lower_bound, upper_bound, includes_upper_bound) \
             VALUES ('quote_test', 0, 1, true) ON CONFLICT (code) DO NOTHING; \
             INSERT INTO quote_statuses (code, is_terminal, description) VALUES \
                 ('active', false, 'Active'), ('accepted', true, 'Accepted') \
             ON CONFLICT (code) DO NOTHING; \
             INSERT INTO price_sources (code, display_name, enabled) \
             VALUES ('quote_test', 'Quote test', true) ON CONFLICT (code) DO NOTHING",
        )
        .await
        .expect("seed quote lookups");

    let suffix = Uuid::new_v4().simple().to_string();
    let user_id: UserId = sqlx::query_scalar(
        "INSERT INTO users (login, password_hash) VALUES ($1, 'argon2id-test-hash') RETURNING id",
    )
    .bind(format!("quote_user_{suffix}"))
    .fetch_one(transaction.as_mut())
    .await
    .expect("insert quote user");
    let collection_id: i64 = sqlx::query_scalar(
        "INSERT INTO collections (slug, display_name) VALUES ($1, 'Quote Test') RETURNING id",
    )
    .bind(format!("quote-{suffix}"))
    .fetch_one(transaction.as_mut())
    .await
    .expect("insert quote collection");
    let item_id: i64 = sqlx::query_scalar(
        "INSERT INTO catalog_items \
            (collection_id, rarity_code, stable_name, min_float, max_float) \
         VALUES ($1, 'quote_test', $2, 0, 1) RETURNING id",
    )
    .bind(collection_id)
    .bind(format!("quote-item-{suffix}"))
    .fetch_one(transaction.as_mut())
    .await
    .expect("insert quote item");
    let sku_id: i64 = sqlx::query_scalar(
        "INSERT INTO skus (catalog_item_id, wear_band_id) \
         SELECT $1, id FROM wear_bands WHERE code = 'quote_test' RETURNING id",
    )
    .bind(item_id)
    .fetch_one(transaction.as_mut())
    .await
    .expect("insert quote SKU");

    let sequence_number = (Uuid::new_v4().as_u128() % 8_000_000_000) as i64 + 1_000_000;
    let commitment_id: i64 = sqlx::query_scalar(
        "INSERT INTO seed_commitments \
            (sequence_number, commitment_hash, encoding_version) \
         VALUES ($1, digest($2, 'sha256'), 'quote-test-v1') RETURNING id",
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
    // allocated_at + 15s" CHECK. Capture one reading and derive both
    // columns from it instead of relying on two separate clock reads.
    let allocation_id: i64 = sqlx::query_scalar(
        "WITH allocation_clock AS (SELECT clock_timestamp() AS now) \
         INSERT INTO seed_allocations (commitment_id, user_id, allocated_at, expires_at) \
         SELECT $1, $2, allocation_clock.now, allocation_clock.now + interval '15 seconds' \
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
         VALUES ('quote-test-v1', clock_timestamp()) RETURNING id",
    )
    .fetch_one(transaction.as_mut())
    .await
    .expect("insert valuation snapshot");
    let snapshot_item_id: i64 = sqlx::query_scalar(
        "INSERT INTO valuation_snapshot_items ( \
            snapshot_id, sku_id, verified_price_microcredits, source_code, window_days, \
            valid_sale_count, evidence_cutoff_at, evidence_digest \
         ) VALUES ($1, $2, 20000000, 'quote_test', 7, 20, clock_timestamp(), \
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
    .bind((sequence_number % 2_000_000_000) as i32 + 10_000)
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
    .bind((sequence_number % 2_000_000_000) as i32 + 20_000)
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

    // As in seed_allocations above: created_at and expires_at each called
    // clock_timestamp() independently (four times total across both CASE
    // branches), risking drift past the 60-second window CHECK -- sharpest
    // for the `expired` branch, whose offsets are exactly 60 seconds apart.
    // Capture one reading in a CTE and derive every branch from it.
    let public_id = PublicId::new(Uuid::new_v4());
    let id: QuoteId = sqlx::query_scalar(
        "WITH quote_clock AS (SELECT clock_timestamp() AS now) \
         INSERT INTO tradeup_quotes ( \
            public_id, user_id, allocation_id, commitment_id, valuation_snapshot_id, \
            stock_policy_version_id, risk_policy_version_id, signing_key_id, status_code, \
            formula_version, client_seed, nonce, verified_input_value_microcredits, \
            expected_buyback_microcredits, quote_total_microcredits, adjustment_microcredits, \
            maximum_exposure_microcredits, ordered_outcome_digest, signature, created_at, expires_at \
         ) \
         SELECT $1, $2, $3, $4, $5, $6, $7, $8, $9, 'quote-test-v1', \
                decode(repeat('99', 32), 'hex'), 7, 10000000, 8500000, 10000000, 0, \
                8500000, decode(repeat('aa', 32), 'hex'), decode(repeat('bb', 64), 'hex'), \
                CASE WHEN $10 THEN quote_clock.now - interval '2 minutes' \
                     ELSE quote_clock.now END, \
                CASE WHEN $10 THEN quote_clock.now - interval '1 minute' \
                     ELSE quote_clock.now + interval '30 seconds' END \
         FROM quote_clock \
         RETURNING id",
    )
    .bind(public_id)
    .bind(user_id)
    .bind(allocation_id)
    .bind(commitment_id)
    .bind(snapshot_id)
    .bind(stock_policy_id)
    .bind(risk_policy_id)
    .bind(signing_key_id)
    .bind(status_code)
    .bind(expired)
    .fetch_one(transaction.as_mut())
    .await
    .expect("insert trade-up quote");

    QuoteFixture {
        id,
        public_id,
        user_id,
        sku_id,
        snapshot_item_id,
    }
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

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn quote_is_found_by_public_id_and_unknown_id_returns_none() {
    let database = test_database().await;
    let mut transaction = isolated_transaction(&database).await;
    let fixture = seed_quote(&mut transaction, "active", false).await;

    let quote = find_tradeup_quote(transaction.as_mut(), fixture.public_id)
        .await
        .expect("find quote")
        .expect("quote exists");
    assert_eq!(quote.id, fixture.id);
    assert_eq!(quote.user_id, fixture.user_id);
    assert_eq!(quote.status_code, "active");
    assert_eq!(quote.nonce, 7);
    assert_eq!(quote.quote_total_microcredits, 10_000_000);
    assert!(
        find_tradeup_quote(transaction.as_mut(), PublicId::new(Uuid::new_v4()))
            .await
            .expect("find unknown quote")
            .is_none()
    );
    transaction.rollback().await.expect("rollback fixture");
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn active_quote_lookup_excludes_expired_and_terminal_quotes() {
    let database = test_database().await;
    let mut transaction = isolated_transaction(&database).await;
    let active = seed_quote(&mut transaction, "active", false).await;
    let expired = seed_quote(&mut transaction, "active", true).await;
    let accepted = seed_quote(&mut transaction, "accepted", false).await;

    assert_eq!(
        find_active_quote_for_user(transaction.as_mut(), active.user_id)
            .await
            .expect("find active quote")
            .expect("active quote exists")
            .id,
        active.id
    );
    assert!(
        find_active_quote_for_user(transaction.as_mut(), expired.user_id)
            .await
            .expect("find expired quote")
            .is_none()
    );
    assert!(
        find_active_quote_for_user(transaction.as_mut(), accepted.user_id)
            .await
            .expect("find terminal quote")
            .is_none()
    );
    transaction.rollback().await.expect("rollback fixture");
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn quote_inputs_and_outcomes_are_ordered_by_position_with_exact_output_float() {
    let database = test_database().await;
    let mut transaction = isolated_transaction(&database).await;
    let quote = seed_quote(&mut transaction, "active", false).await;
    let first_input = insert_inventory_item(&mut transaction, quote.sku_id, "0.10000000").await;
    let second_input = insert_inventory_item(&mut transaction, quote.sku_id, "0.20000000").await;
    for (position, item_id) in [(2_i16, second_input), (1_i16, first_input)] {
        sqlx::query(
            "INSERT INTO quote_inputs ( \
                quote_id, position, inventory_item_id, valuation_snapshot_item_id, \
                locked_position_version \
             ) VALUES ($1, $2, $3, $4, 1)",
        )
        .bind(quote.id)
        .bind(position)
        .bind(item_id)
        .bind(quote.snapshot_item_id)
        .execute(transaction.as_mut())
        .await
        .expect("insert quote input");
    }
    let first_candidate = insert_inventory_item(&mut transaction, quote.sku_id, "0.33333333").await;
    let second_candidate =
        insert_inventory_item(&mut transaction, quote.sku_id, "0.87654321").await;
    for (position, item_id, output_float) in [
        (2_i16, second_candidate, "0.87654321"),
        (1_i16, first_candidate, "0.33333333"),
    ] {
        sqlx::query(
            "INSERT INTO quote_outcomes ( \
                quote_id, position, sku_id, candidate_inventory_item_id, \
                valuation_snapshot_item_id, probability_numerator, probability_denominator, \
                output_float, buyback_microcredits \
             ) VALUES ($1, $2, $3, $4, $5, 1, 2, $6, 850000)",
        )
        .bind(quote.id)
        .bind(position)
        .bind(quote.sku_id)
        .bind(item_id)
        .bind(quote.snapshot_item_id)
        .bind(Decimal::from_str(output_float).expect("valid output float"))
        .execute(transaction.as_mut())
        .await
        .expect("insert quote outcome");
    }

    let inputs = list_quote_inputs(transaction.as_mut(), quote.id)
        .await
        .expect("list quote inputs");
    assert_eq!(
        inputs
            .iter()
            .map(|input| input.position)
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert_eq!(inputs[0].inventory_item_id.get(), first_input);
    let outcomes = list_quote_outcomes(transaction.as_mut(), quote.id)
        .await
        .expect("list quote outcomes");
    assert_eq!(
        outcomes
            .iter()
            .map(|outcome| outcome.position)
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert_eq!(
        outcomes[0].output_float,
        Decimal::from_str("0.33333333").expect("valid decimal")
    );
    assert_eq!(
        outcomes[1].output_float,
        Decimal::from_str("0.87654321").expect("valid decimal")
    );
    transaction.rollback().await.expect("rollback fixture");
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn runtime_role_can_execute_every_quote_read_api() {
    let database = test_database().await;
    let runtime_role_required = std::env::var_os("TEST_RUNTIME_ROLE_REQUIRED").is_some();
    let role_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'contracter_runtime')",
    )
    .fetch_one(database.pool())
    .await
    .expect("inspect runtime role");
    if !role_exists {
        assert!(!runtime_role_required, "contracter_runtime must exist");
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
        return;
    }

    let mut transaction = isolated_transaction(&database).await;
    transaction
        .execute("SET LOCAL ROLE contracter_runtime")
        .await
        .expect("assume runtime role");
    find_tradeup_quote(transaction.as_mut(), PublicId::new(Uuid::new_v4()))
        .await
        .expect("runtime finds quote");
    find_active_quote_for_user(transaction.as_mut(), UserId::new(-1))
        .await
        .expect("runtime finds active user quote");
    let inputs = list_quote_inputs(transaction.as_mut(), QuoteId::new(-1))
        .await
        .expect("runtime lists quote inputs");
    assert!(inputs.is_empty());
    let outcomes = list_quote_outcomes(transaction.as_mut(), QuoteId::new(-1))
        .await
        .expect("runtime lists quote outcomes");
    assert!(outcomes.is_empty());
    transaction.rollback().await.expect("rollback role check");
}
