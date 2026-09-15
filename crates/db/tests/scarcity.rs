use db::{
    CollectionId, Database, DatabaseConfig, find_current_collection_scarcity,
    list_collection_scarcity_history, publish_collection_scarcity_snapshot,
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
        .execute("SELECT pg_advisory_xact_lock(812_202_609_43)")
        .await
        .expect("serialize scarcity integration fixtures");
    transaction
}

async fn insert_collection(
    transaction: &mut Transaction<'_, Postgres>,
    slug: &str,
) -> CollectionId {
    sqlx::query_scalar(
        "INSERT INTO collections (slug, display_name) VALUES ($1, 'Scarcity Test') \
         RETURNING id",
    )
    .bind(slug)
    .fetch_one(transaction.as_mut())
    .await
    .expect("insert collection")
}

/// Rarity + SKU under `collection_id`. `rarity_code` is created on demand
/// (`ON CONFLICT DO NOTHING`) so callers can use a covered or an
/// intentionally uncovered rarity in the same fixture.
async fn insert_sku(
    transaction: &mut Transaction<'_, Postgres>,
    collection_id: CollectionId,
    rarity_code: &str,
    rarity_rank: i16,
) -> i64 {
    sqlx::query(
        "INSERT INTO rarities (code, rank, is_covert) VALUES ($1, $2, false) \
         ON CONFLICT (code) DO NOTHING",
    )
    .bind(rarity_code)
    .bind(rarity_rank)
    .execute(transaction.as_mut())
    .await
    .expect("seed rarity");
    transaction
        .execute(
            "INSERT INTO wear_bands (code, lower_bound, upper_bound, includes_upper_bound) \
             VALUES ('scarcity-test', 0, 1, true) ON CONFLICT (code) DO NOTHING",
        )
        .await
        .expect("seed wear band");

    let suffix = Uuid::new_v4().simple().to_string();
    let catalog_item_id: i64 = sqlx::query_scalar(
        "INSERT INTO catalog_items \
            (collection_id, rarity_code, stable_name, min_float, max_float) \
         VALUES ($1, $2, $3, 0, 1) RETURNING id",
    )
    .bind(collection_id)
    .bind(rarity_code)
    .bind(format!("scarcity-item-{suffix}"))
    .fetch_one(transaction.as_mut())
    .await
    .expect("insert catalog item");
    sqlx::query_scalar(
        "INSERT INTO skus (catalog_item_id, wear_band_id) \
         SELECT $1, id FROM wear_bands WHERE code = 'scarcity-test' RETURNING id",
    )
    .bind(catalog_item_id)
    .fetch_one(transaction.as_mut())
    .await
    .expect("insert SKU")
}

async fn set_warehouse_stock(
    transaction: &mut Transaction<'_, Postgres>,
    sku_id: i64,
    available_units: i32,
) {
    sqlx::query(
        "INSERT INTO warehouse_stock (sku_id, available_units) VALUES ($1, $2) \
         ON CONFLICT (sku_id) DO UPDATE SET available_units = EXCLUDED.available_units, \
                                             version = warehouse_stock.version + 1",
    )
    .bind(sku_id)
    .bind(available_units)
    .execute(transaction.as_mut())
    .await
    .expect("upsert warehouse stock");
}

/// One clearly active stock policy version plus a target-band for
/// `rarity_code`. A seeded `version = 1` policy may already be active in a
/// fully-seeded database, so this fixture's `activated_at` is set an hour
/// into the future to guarantee it sorts as the most recent active version
/// (same fix applied in `crates/db/tests/stock.rs`).
async fn insert_active_target(
    transaction: &mut Transaction<'_, Postgres>,
    rarity_code: &str,
    target_units: i32,
) {
    let version = (Uuid::new_v4().as_u128() % 1_000_000) as i32 + 1;
    let stock_policy_version_id: i64 = sqlx::query_scalar(
        "INSERT INTO stock_policy_versions (version, activated_at) \
         VALUES ($1, clock_timestamp() + interval '1 hour') RETURNING id",
    )
    .bind(version)
    .fetch_one(transaction.as_mut())
    .await
    .expect("insert active stock policy version");
    sqlx::query(
        "INSERT INTO stock_policy_bands \
            (stock_policy_version_id, rarity_code, minimum_units, target_units, maximum_units) \
         VALUES ($1, $2, 0, $3, $3)",
    )
    .bind(stock_policy_version_id)
    .bind(rarity_code)
    .bind(target_units)
    .execute(transaction.as_mut())
    .await
    .expect("insert stock policy band");
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn publishing_a_snapshot_computes_the_exact_scarcity_multiplier() {
    let database = test_database().await;
    let mut transaction = isolated_transaction(&database).await;

    let stocked_collection = insert_collection(&mut transaction, "scarcity-stocked").await;
    let stocked_sku = insert_sku(&mut transaction, stocked_collection, "scarcity-a", 95).await;
    set_warehouse_stock(&mut transaction, stocked_sku, 40).await;
    insert_active_target(&mut transaction, "scarcity-a", 100).await;

    let empty_collection = insert_collection(&mut transaction, "scarcity-empty").await;
    let empty_sku = insert_sku(&mut transaction, empty_collection, "scarcity-b", 96).await;
    // No warehouse_stock row at all for this SKU: available defaults to 0.
    insert_active_target(&mut transaction, "scarcity-b", 50).await;
    let _ = empty_sku;

    let snapshot_id = publish_collection_scarcity_snapshot(transaction.as_mut(), "scarcity-v1")
        .await
        .expect("publish scarcity snapshot");
    assert!(snapshot_id.get() > 0);

    let stocked = find_current_collection_scarcity(transaction.as_mut(), stocked_collection)
        .await
        .expect("query stocked collection scarcity")
        .expect("stocked collection has a scarcity row");
    assert_eq!(stocked.weight_multiplier_numerator, 40);
    assert_eq!(stocked.weight_multiplier_denominator, 100);

    let empty = find_current_collection_scarcity(transaction.as_mut(), empty_collection)
        .await
        .expect("query empty collection scarcity")
        .expect("empty collection still has a scarcity row (target is covered)");
    assert_eq!(empty.weight_multiplier_numerator, 0);
    assert_eq!(empty.weight_multiplier_denominator, 50);

    let history = list_collection_scarcity_history(transaction.as_mut(), stocked_collection)
        .await
        .expect("list scarcity history");
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].available_units_total, 40);
    assert_eq!(history[0].target_units_total, 100);

    transaction.rollback().await.expect("rollback fixture");
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn collection_without_stock_policy_coverage_has_no_current_scarcity_row() {
    let database = test_database().await;
    let mut transaction = isolated_transaction(&database).await;

    let uncovered_collection = insert_collection(&mut transaction, "scarcity-uncovered").await;
    let uncovered_sku = insert_sku(
        &mut transaction,
        uncovered_collection,
        "scarcity-uncovered",
        97,
    )
    .await;
    set_warehouse_stock(&mut transaction, uncovered_sku, 5).await;
    // Deliberately no `insert_active_target` call: no stock_policy_bands row
    // exists for rarity "scarcity-uncovered".

    publish_collection_scarcity_snapshot(transaction.as_mut(), "scarcity-v1")
        .await
        .expect("publish scarcity snapshot");

    assert!(
        find_current_collection_scarcity(transaction.as_mut(), uncovered_collection)
            .await
            .expect("query uncovered collection scarcity")
            .is_none(),
        "a collection outside stock-policy coverage must not get a damping row"
    );

    transaction.rollback().await.expect("rollback fixture");
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn scarcity_history_accumulates_across_multiple_publishes() {
    let database = test_database().await;
    let mut transaction = isolated_transaction(&database).await;

    let collection = insert_collection(&mut transaction, "scarcity-history").await;
    let sku = insert_sku(&mut transaction, collection, "scarcity-history", 98).await;
    insert_active_target(&mut transaction, "scarcity-history", 100).await;

    set_warehouse_stock(&mut transaction, sku, 20).await;
    publish_collection_scarcity_snapshot(transaction.as_mut(), "scarcity-v1")
        .await
        .expect("publish first snapshot");

    set_warehouse_stock(&mut transaction, sku, 60).await;
    publish_collection_scarcity_snapshot(transaction.as_mut(), "scarcity-v1")
        .await
        .expect("publish second snapshot");

    let history = list_collection_scarcity_history(transaction.as_mut(), collection)
        .await
        .expect("list scarcity history");
    assert_eq!(
        history
            .iter()
            .map(|item| item.available_units_total)
            .collect::<Vec<_>>(),
        vec![20, 60]
    );

    let current = find_current_collection_scarcity(transaction.as_mut(), collection)
        .await
        .expect("query current scarcity")
        .expect("collection has a scarcity row");
    assert_eq!(current.weight_multiplier_numerator, 60);
    assert_eq!(current.weight_multiplier_denominator, 100);

    transaction.rollback().await.expect("rollback fixture");
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn runtime_role_can_execute_every_scarcity_read_api() {
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
        "collection_scarcity_snapshots",
        "collection_scarcity_snapshot_items",
        "current_collection_scarcity",
    ] {
        let can_select: bool =
            sqlx::query_scalar("SELECT has_table_privilege('contracter_runtime', $1, 'SELECT')")
                .bind(relation)
                .fetch_one(database.pool())
                .await
                .expect("inspect scarcity privilege");
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
    find_current_collection_scarcity(transaction.as_mut(), CollectionId::new(-1))
        .await
        .expect("runtime finds collection scarcity");
    let history = list_collection_scarcity_history(transaction.as_mut(), CollectionId::new(-1))
        .await
        .expect("runtime lists scarcity history");
    assert!(history.is_empty());
    transaction.rollback().await.expect("rollback role check");
}
