use db::{
    Database, DatabaseConfig, SkuId, StockPolicyVersionId, find_active_stock_policy_bands,
    find_active_stock_policy_version, find_risk_state, find_warehouse_stock,
};
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
        .execute("SELECT pg_advisory_xact_lock(812_202_609_31)")
        .await
        .expect("serialize stock integration fixtures");
    transaction
}

async fn ensure_lookup_rows(transaction: &mut Transaction<'_, Postgres>) {
    transaction
        .execute(
            "INSERT INTO rarities (code, rank, is_covert) VALUES \
                ('stock-test-a', 91, false), \
                ('stock-test-b', 92, false) \
             ON CONFLICT (code) DO NOTHING",
        )
        .await
        .expect("seed rarities");
    transaction
        .execute(
            "INSERT INTO wear_bands (code, lower_bound, upper_bound, includes_upper_bound) \
             VALUES ('stock-test', 0, 1, true) \
             ON CONFLICT (code) DO NOTHING",
        )
        .await
        .expect("seed wear band");
}

async fn insert_sku(transaction: &mut Transaction<'_, Postgres>, rarity_code: &str) -> SkuId {
    let suffix = Uuid::new_v4().simple().to_string();
    let collection_id: i64 = sqlx::query_scalar(
        "INSERT INTO collections (slug, display_name) VALUES ($1, 'Stock Test') RETURNING id",
    )
    .bind(format!("stock-{suffix}"))
    .fetch_one(transaction.as_mut())
    .await
    .expect("insert collection");
    let catalog_item_id: i64 = sqlx::query_scalar(
        "INSERT INTO catalog_items \
            (collection_id, rarity_code, stable_name, min_float, max_float) \
         VALUES ($1, $2, $3, 0, 1) RETURNING id",
    )
    .bind(collection_id)
    .bind(rarity_code)
    .bind(format!("stock-item-{suffix}"))
    .fetch_one(transaction.as_mut())
    .await
    .expect("insert catalog item");
    sqlx::query_scalar(
        "INSERT INTO skus (catalog_item_id, wear_band_id) \
         SELECT $1, id FROM wear_bands WHERE code = 'stock-test' RETURNING id",
    )
    .bind(catalog_item_id)
    .fetch_one(transaction.as_mut())
    .await
    .expect("insert SKU")
}

async fn insert_stock_policy_version(
    transaction: &mut Transaction<'_, Postgres>,
    version: i32,
    activated: bool,
    retired: bool,
) -> StockPolicyVersionId {
    // A seeded active policy (version 1) may already exist with `activated_at`
    // at or before now. A retired fixture's `activated_at` only needs to
    // precede its own `retired_at`, but a still-active fixture must sort
    // ahead of any pre-existing active seed row, so it gets a future
    // timestamp instead of a past one.
    sqlx::query_scalar(
        "INSERT INTO stock_policy_versions (version, activated_at, retired_at) \
         VALUES ( \
             $1, \
             CASE WHEN $3 THEN clock_timestamp() - interval '2 hours' \
                  WHEN $2 THEN clock_timestamp() + interval '1 hour' \
                  ELSE NULL END, \
             CASE WHEN $3 THEN clock_timestamp() ELSE NULL END \
         ) RETURNING id",
    )
    .bind(version)
    .bind(activated)
    .bind(retired)
    .fetch_one(transaction.as_mut())
    .await
    .expect("insert stock policy version")
}

async fn insert_stock_policy_band(
    transaction: &mut Transaction<'_, Postgres>,
    stock_policy_version_id: StockPolicyVersionId,
    rarity_code: &str,
    minimum_units: i32,
    target_units: i32,
    maximum_units: i32,
) -> i64 {
    sqlx::query_scalar(
        "INSERT INTO stock_policy_bands \
            (stock_policy_version_id, rarity_code, minimum_units, target_units, maximum_units) \
         VALUES ($1, $2, $3, $4, $5) RETURNING id",
    )
    .bind(stock_policy_version_id)
    .bind(rarity_code)
    .bind(minimum_units)
    .bind(target_units)
    .bind(maximum_units)
    .fetch_one(transaction.as_mut())
    .await
    .expect("insert stock policy band")
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn warehouse_stock_is_found_for_known_sku_and_none_for_unknown() {
    let database = test_database().await;
    let mut transaction = isolated_transaction(&database).await;
    ensure_lookup_rows(&mut transaction).await;
    let sku_id = insert_sku(&mut transaction, "stock-test-a").await;

    sqlx::query(
        "INSERT INTO warehouse_stock (sku_id, available_units, reserved_units) \
         VALUES ($1, 10, 3)",
    )
    .bind(sku_id)
    .execute(transaction.as_mut())
    .await
    .expect("insert warehouse stock");

    let found = find_warehouse_stock(transaction.as_mut(), sku_id)
        .await
        .expect("query warehouse stock")
        .expect("warehouse stock exists");
    assert_eq!(found.sku_id, sku_id);
    assert_eq!(found.available_units, 10);
    assert_eq!(found.reserved_units, 3);
    assert_eq!(found.version, 0);

    let unknown_sku_id = SkuId::new(sku_id.get() + 1_000_000);
    assert!(
        find_warehouse_stock(transaction.as_mut(), unknown_sku_id)
            .await
            .expect("query warehouse stock for unknown sku")
            .is_none()
    );

    transaction.rollback().await.expect("rollback fixture");
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn active_stock_policy_version_excludes_retired_and_pending_versions() {
    let database = test_database().await;
    let mut transaction = isolated_transaction(&database).await;
    let base_version = (Uuid::new_v4().as_u128() % 1_000_000) as i32 + 1;

    let active_id = insert_stock_policy_version(&mut transaction, base_version, true, false).await;
    let _pending_id =
        insert_stock_policy_version(&mut transaction, base_version + 1, false, false).await;
    let _retired_id =
        insert_stock_policy_version(&mut transaction, base_version + 2, true, true).await;

    let found = find_active_stock_policy_version(transaction.as_mut())
        .await
        .expect("query active stock policy version")
        .expect("an active stock policy version exists");
    assert_eq!(found.id, active_id);
    assert_eq!(found.version, base_version);
    assert!(found.activated_at.is_some());
    assert!(found.retired_at.is_none());

    transaction.rollback().await.expect("rollback fixture");
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn active_stock_policy_bands_are_scoped_to_the_active_version_and_stably_ordered() {
    let database = test_database().await;
    let mut transaction = isolated_transaction(&database).await;
    ensure_lookup_rows(&mut transaction).await;
    let base_version = (Uuid::new_v4().as_u128() % 1_000_000) as i32 + 1;

    let active_id = insert_stock_policy_version(&mut transaction, base_version, true, false).await;
    let retired_id =
        insert_stock_policy_version(&mut transaction, base_version + 1, true, true).await;

    let band_first =
        insert_stock_policy_band(&mut transaction, active_id, "stock-test-b", 0, 100, 150).await;
    let band_second =
        insert_stock_policy_band(&mut transaction, active_id, "stock-test-a", 0, 50, 75).await;
    let _band_under_retired_version =
        insert_stock_policy_band(&mut transaction, retired_id, "stock-test-a", 0, 1, 1).await;

    let bands = find_active_stock_policy_bands(transaction.as_mut())
        .await
        .expect("query active stock policy bands");

    assert_eq!(
        bands.iter().map(|band| band.id.get()).collect::<Vec<_>>(),
        vec![band_first, band_second]
    );
    assert!(
        bands
            .iter()
            .all(|band| band.stock_policy_version_id == active_id)
    );

    transaction.rollback().await.expect("rollback fixture");
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn risk_state_singleton_reflects_current_values() {
    let database = test_database().await;
    let mut transaction = isolated_transaction(&database).await;
    transaction
        .execute(
            "UPDATE risk_state SET valuation_snapshot_id = NULL, risk_policy_version_id = NULL, \
             liquid_reserve_microcredits = 0, stressed_liability_microcredits = 0, \
             outstanding_quote_exposure_microcredits = 0, version = 0 WHERE singleton",
        )
        .await
        .expect("reset risk state fixture");

    let initial = find_risk_state(transaction.as_mut())
        .await
        .expect("query initial risk state");
    assert_eq!(initial.liquid_reserve_microcredits, 0);
    assert_eq!(initial.stressed_liability_microcredits, 0);
    assert_eq!(initial.outstanding_quote_exposure_microcredits, 0);
    assert_eq!(initial.valuation_snapshot_id, None);
    assert_eq!(initial.risk_policy_version_id, None);
    let initial_version = initial.version;

    sqlx::query(
        "UPDATE risk_state \
            SET liquid_reserve_microcredits = 500000000, \
                stressed_liability_microcredits = 200000000, \
                outstanding_quote_exposure_microcredits = 10000000, \
                version = version + 1, \
                updated_at = clock_timestamp() \
          WHERE singleton",
    )
    .execute(transaction.as_mut())
    .await
    .expect("update risk state");

    let updated = find_risk_state(transaction.as_mut())
        .await
        .expect("query updated risk state");
    assert_eq!(updated.liquid_reserve_microcredits, 500_000_000);
    assert_eq!(updated.stressed_liability_microcredits, 200_000_000);
    assert_eq!(updated.outstanding_quote_exposure_microcredits, 10_000_000);
    assert_eq!(updated.version, initial_version + 1);

    transaction.rollback().await.expect("rollback fixture");
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn runtime_role_can_execute_stock_and_risk_read_api() {
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

    for relation in [
        "stock_policy_versions",
        "stock_policy_bands",
        "warehouse_stock",
        "risk_state",
    ] {
        let can_select: bool =
            sqlx::query_scalar("SELECT has_table_privilege('contracter_runtime', $1, 'SELECT')")
                .bind(relation)
                .fetch_one(database.pool())
                .await
                .expect("inspect stock/risk privilege");
        assert!(can_select, "runtime role cannot select {relation}");
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
    find_active_stock_policy_version(transaction.as_mut())
        .await
        .expect("runtime role reads active stock policy version");
    find_active_stock_policy_bands(transaction.as_mut())
        .await
        .expect("runtime role reads active stock policy bands");
    find_risk_state(transaction.as_mut())
        .await
        .expect("runtime role reads risk state");
    transaction.rollback().await.expect("rollback role check");
}
