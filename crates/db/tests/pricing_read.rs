use db::{
    Database, DatabaseConfig, PriceHaltId, PublicId, SkuId, ValuationSnapshotId,
    find_current_valuation, find_valuation_snapshot, list_active_price_halts,
    list_snapshot_valuations, list_tradeable_current_valuations,
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
        .execute("SELECT pg_advisory_xact_lock(812_202_609_24)")
        .await
        .expect("serialize pricing integration fixtures");
    transaction
}

async fn seed_lookups(transaction: &mut Transaction<'_, Postgres>) {
    transaction
        .execute(
            "INSERT INTO rarities (code, rank, is_covert) \
             VALUES ('pricing_test', 92, false) ON CONFLICT (code) DO NOTHING; \
             INSERT INTO wear_bands (code, lower_bound, upper_bound, includes_upper_bound) \
             VALUES ('pricing_test', 0, 1, true) ON CONFLICT (code) DO NOTHING; \
             INSERT INTO price_sources (code, display_name, enabled) VALUES \
                 ('pricing_enabled', 'Pricing enabled', true), \
                 ('pricing_disabled', 'Pricing disabled', false) \
             ON CONFLICT (code) DO UPDATE SET enabled = EXCLUDED.enabled; \
             INSERT INTO price_halt_reasons (code, description) \
             VALUES ('pricing_test', 'Pricing test') ON CONFLICT (code) DO NOTHING",
        )
        .await
        .expect("seed pricing lookups");
}

async fn insert_sku(
    transaction: &mut Transaction<'_, Postgres>,
    collection_enabled: bool,
    item_enabled: bool,
    sku_enabled: bool,
) -> SkuId {
    let suffix = Uuid::new_v4().simple().to_string();
    let collection_id: i64 = sqlx::query_scalar(
        "INSERT INTO collections (slug, display_name, enabled) \
         VALUES ($1, 'Pricing Test', $2) RETURNING id",
    )
    .bind(format!("pricing-{suffix}"))
    .bind(collection_enabled)
    .fetch_one(transaction.as_mut())
    .await
    .expect("insert collection");
    let item_id: i64 = sqlx::query_scalar(
        "INSERT INTO catalog_items \
            (collection_id, rarity_code, stable_name, min_float, max_float, enabled) \
         VALUES ($1, 'pricing_test', $2, 0, 1, $3) RETURNING id",
    )
    .bind(collection_id)
    .bind(format!("pricing-item-{suffix}"))
    .bind(item_enabled)
    .fetch_one(transaction.as_mut())
    .await
    .expect("insert catalog item");
    sqlx::query_scalar(
        "INSERT INTO skus (catalog_item_id, wear_band_id, enabled) \
         SELECT $1, id, $2 FROM wear_bands WHERE code = 'pricing_test' RETURNING id",
    )
    .bind(item_id)
    .bind(sku_enabled)
    .fetch_one(transaction.as_mut())
    .await
    .expect("insert SKU")
}

async fn insert_snapshot(
    transaction: &mut Transaction<'_, Postgres>,
    published: bool,
) -> (ValuationSnapshotId, PublicId) {
    let public_id = PublicId::new(Uuid::new_v4());
    // A single `snapshot_clock` reading feeds created_at/snapshot_at/
    // published_at: this table's CHECK (published_at IS NULL OR
    // published_at >= created_at) is otherwise a tight, zero-margin race
    // between the explicit clock_timestamp() calls here and the separate
    // one `created_at`'s column DEFAULT would otherwise evaluate (same
    // clock-drift hazard fixed for seed_allocations in inventory.rs).
    let id = sqlx::query_scalar(
        "WITH snapshot_clock AS (SELECT clock_timestamp() AS now) \
         INSERT INTO valuation_snapshots \
            (public_id, formula_version, snapshot_at, created_at, published_at) \
         SELECT $1, 'pricing-test-v1', snapshot_clock.now, snapshot_clock.now, \
                CASE WHEN $2 THEN snapshot_clock.now ELSE NULL END \
           FROM snapshot_clock \
         RETURNING id",
    )
    .bind(public_id)
    .bind(published)
    .fetch_one(transaction.as_mut())
    .await
    .expect("insert valuation snapshot");
    (id, public_id)
}

async fn insert_valuation(
    transaction: &mut Transaction<'_, Postgres>,
    snapshot_id: ValuationSnapshotId,
    sku_id: SkuId,
    source_code: &str,
    price: i64,
) {
    let snapshot_item_id: i64 = sqlx::query_scalar(
        "INSERT INTO valuation_snapshot_items ( \
            snapshot_id, sku_id, verified_price_microcredits, source_code, \
            window_days, valid_sale_count, evidence_cutoff_at, evidence_digest \
         ) VALUES ( \
            $1, $2, $3, $4, 7, 20, clock_timestamp(), \
            decode(repeat('77', 32), 'hex') \
         ) RETURNING id",
    )
    .bind(snapshot_id)
    .bind(sku_id)
    .bind(price)
    .bind(source_code)
    .fetch_one(transaction.as_mut())
    .await
    .expect("insert snapshot valuation");
    sqlx::query(
        "INSERT INTO current_valuations \
            (sku_id, snapshot_id, snapshot_item_id, verified_price_microcredits) \
         VALUES ($1, $2, $3, $4)",
    )
    .bind(sku_id)
    .bind(snapshot_id)
    .bind(snapshot_item_id)
    .bind(price)
    .execute(transaction.as_mut())
    .await
    .expect("insert current valuation");
}

async fn insert_halt(
    transaction: &mut Transaction<'_, Postgres>,
    sku_id: SkuId,
    observed_ratio: &str,
    active: bool,
) -> PriceHaltId {
    sqlx::query_scalar(
        "INSERT INTO price_halts \
            (sku_id, reason_code, observed_ratio, lifted_at) \
         VALUES ($1, 'pricing_test', $2, \
                 CASE WHEN $3 THEN NULL ELSE clock_timestamp() END) \
         RETURNING id",
    )
    .bind(sku_id)
    .bind(Decimal::from_str(observed_ratio).expect("valid observed ratio"))
    .bind(active)
    .fetch_one(transaction.as_mut())
    .await
    .expect("insert price halt")
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn snapshot_and_current_valuation_are_found_and_unknown_ids_return_none() {
    let database = test_database().await;
    let mut transaction = isolated_transaction(&database).await;
    seed_lookups(&mut transaction).await;
    let sku_id = insert_sku(&mut transaction, true, true, true).await;
    let (snapshot_id, public_id) = insert_snapshot(&mut transaction, true).await;
    insert_valuation(
        &mut transaction,
        snapshot_id,
        sku_id,
        "pricing_enabled",
        1_234_567,
    )
    .await;

    let snapshot = find_valuation_snapshot(transaction.as_mut(), public_id)
        .await
        .expect("find snapshot")
        .expect("snapshot exists");
    assert_eq!(snapshot.id, snapshot_id);
    assert_eq!(snapshot.formula_version, "pricing-test-v1");
    assert!(snapshot.published_at.is_some());

    let valuation = find_current_valuation(transaction.as_mut(), sku_id)
        .await
        .expect("find current valuation")
        .expect("current valuation exists");
    assert_eq!(valuation.snapshot_id, snapshot_id);
    assert_eq!(valuation.verified_price_microcredits, 1_234_567);

    assert!(
        find_valuation_snapshot(transaction.as_mut(), PublicId::new(Uuid::new_v4()))
            .await
            .expect("find unknown snapshot")
            .is_none()
    );
    assert!(
        find_current_valuation(transaction.as_mut(), SkuId::new(-1))
            .await
            .expect("find unknown valuation")
            .is_none()
    );
    transaction.rollback().await.expect("rollback fixture");
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn snapshot_valuations_are_returned_in_stable_sku_order() {
    let database = test_database().await;
    let mut transaction = isolated_transaction(&database).await;
    seed_lookups(&mut transaction).await;
    let first_sku = insert_sku(&mut transaction, true, true, true).await;
    let second_sku = insert_sku(&mut transaction, true, true, true).await;
    let (snapshot_id, _) = insert_snapshot(&mut transaction, true).await;
    insert_valuation(
        &mut transaction,
        snapshot_id,
        second_sku,
        "pricing_enabled",
        2_000_000,
    )
    .await;
    insert_valuation(
        &mut transaction,
        snapshot_id,
        first_sku,
        "pricing_enabled",
        1_000_000,
    )
    .await;

    let items = list_snapshot_valuations(transaction.as_mut(), snapshot_id)
        .await
        .expect("list snapshot valuations");
    assert_eq!(
        items.iter().map(|item| item.sku_id).collect::<Vec<_>>(),
        vec![first_sku, second_sku]
    );
    assert_eq!(items[0].verified_price_microcredits, 1_000_000);
    assert_eq!(items[1].verified_price_microcredits, 2_000_000);
    transaction.rollback().await.expect("rollback fixture");
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn tradeable_valuations_exclude_disabled_catalog_source_and_active_halts() {
    let database = test_database().await;
    let mut transaction = isolated_transaction(&database).await;
    seed_lookups(&mut transaction).await;
    let first_sku = insert_sku(&mut transaction, true, true, true).await;
    let second_sku = insert_sku(&mut transaction, true, true, true).await;
    let disabled_sku = insert_sku(&mut transaction, true, true, false).await;
    let disabled_item_sku = insert_sku(&mut transaction, true, false, true).await;
    let disabled_collection_sku = insert_sku(&mut transaction, false, true, true).await;
    let disabled_source_sku = insert_sku(&mut transaction, true, true, true).await;
    let halted_sku = insert_sku(&mut transaction, true, true, true).await;
    let lifted_halt_sku = insert_sku(&mut transaction, true, true, true).await;
    let (snapshot_id, _) = insert_snapshot(&mut transaction, true).await;
    for (sku_id, source_code, price) in [
        (second_sku, "pricing_enabled", 2_000_000),
        (first_sku, "pricing_enabled", 1_000_000),
        (disabled_sku, "pricing_enabled", 3_000_000),
        (disabled_item_sku, "pricing_enabled", 3_500_000),
        (disabled_collection_sku, "pricing_enabled", 3_750_000),
        (disabled_source_sku, "pricing_disabled", 4_000_000),
        (halted_sku, "pricing_enabled", 5_000_000),
        (lifted_halt_sku, "pricing_enabled", 6_000_000),
    ] {
        insert_valuation(&mut transaction, snapshot_id, sku_id, source_code, price).await;
    }
    insert_halt(&mut transaction, halted_sku, "0.25000000", true).await;
    insert_halt(&mut transaction, lifted_halt_sku, "0.20000000", false).await;

    let items = list_tradeable_current_valuations(transaction.as_mut())
        .await
        .expect("list tradeable valuations");
    assert_eq!(
        items.iter().map(|item| item.sku_id).collect::<Vec<_>>(),
        vec![first_sku, second_sku, lifted_halt_sku]
    );
    transaction.rollback().await.expect("rollback fixture");
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn active_halts_preserve_decimal_ratio_and_exclude_lifted_rows_in_stable_order() {
    let database = test_database().await;
    let mut transaction = isolated_transaction(&database).await;
    seed_lookups(&mut transaction).await;
    let first_sku = insert_sku(&mut transaction, true, true, true).await;
    let second_sku = insert_sku(&mut transaction, true, true, true).await;
    let lifted_sku = insert_sku(&mut transaction, true, true, true).await;
    let second_id = insert_halt(&mut transaction, second_sku, "0.87654321", true).await;
    let first_id = insert_halt(&mut transaction, first_sku, "0.12345678", true).await;
    insert_halt(&mut transaction, lifted_sku, "0.50000000", false).await;

    let halts = list_active_price_halts(transaction.as_mut())
        .await
        .expect("list active price halts");
    assert_eq!(
        halts.iter().map(|halt| halt.id).collect::<Vec<_>>(),
        vec![first_id, second_id]
    );
    assert_eq!(
        halts[0].observed_ratio,
        Some(Decimal::from_str("0.12345678").expect("valid decimal"))
    );
    assert_eq!(
        halts[1].observed_ratio,
        Some(Decimal::from_str("0.87654321").expect("valid decimal"))
    );
    transaction.rollback().await.expect("rollback fixture");
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn runtime_role_can_execute_every_pricing_read_api() {
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
        eprintln!("contracter_runtime does not exist; pricing role check skipped");
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
        eprintln!("migration user cannot SET ROLE contracter_runtime; pricing check skipped");
        return;
    }

    let mut transaction = isolated_transaction(&database).await;
    transaction
        .execute("SET LOCAL ROLE contracter_runtime")
        .await
        .expect("assume runtime role");
    find_valuation_snapshot(transaction.as_mut(), PublicId::new(Uuid::new_v4()))
        .await
        .expect("runtime role finds snapshot");
    find_current_valuation(transaction.as_mut(), SkuId::new(-1))
        .await
        .expect("runtime role finds current valuation");
    list_snapshot_valuations(transaction.as_mut(), ValuationSnapshotId::new(-1))
        .await
        .expect("runtime role lists snapshot valuations");
    list_tradeable_current_valuations(transaction.as_mut())
        .await
        .expect("runtime role lists tradeable valuations");
    list_active_price_halts(transaction.as_mut())
        .await
        .expect("runtime role lists active halts");
    transaction.rollback().await.expect("rollback role check");
}
