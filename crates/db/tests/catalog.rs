use std::str::FromStr;

use db::{
    CatalogItemId, CollectionId, Database, DatabaseConfig, PublicId, SkuId, WearBandId,
    find_active_skus, find_catalog_item_by_public_id, find_collection_by_public_id,
    find_sku_by_public_id,
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
        .execute("SELECT pg_advisory_xact_lock(812_202_609_23)")
        .await
        .expect("serialize catalog integration fixtures");
    transaction
}

async fn ensure_lookup_rows(transaction: &mut Transaction<'_, Postgres>) {
    transaction
        .execute(
            "INSERT INTO rarities (code, rank, is_covert) VALUES ('mil-spec', 2, false) \
             ON CONFLICT (code) DO NOTHING",
        )
        .await
        .expect("seed rarity");
    transaction
        .execute(
            "INSERT INTO wear_bands (code, lower_bound, upper_bound, includes_upper_bound) \
             VALUES ('factory_new', 0, 0.07, false), \
                    ('minimal_wear', 0.07, 0.15, false) \
             ON CONFLICT (code) DO NOTHING",
        )
        .await
        .expect("seed wear bands");
}

async fn wear_band_ids(transaction: &mut Transaction<'_, Postgres>) -> (WearBandId, WearBandId) {
    let factory_new: WearBandId =
        sqlx::query_scalar("SELECT id FROM wear_bands WHERE code = 'factory_new'")
            .fetch_one(transaction.as_mut())
            .await
            .expect("read factory_new wear band id");
    let minimal_wear: WearBandId =
        sqlx::query_scalar("SELECT id FROM wear_bands WHERE code = 'minimal_wear'")
            .fetch_one(transaction.as_mut())
            .await
            .expect("read minimal_wear wear band id");
    (factory_new, minimal_wear)
}

struct CollectionFixture {
    id: CollectionId,
    public_id: PublicId,
}

async fn insert_collection(
    transaction: &mut Transaction<'_, Postgres>,
    slug: &str,
    enabled: bool,
) -> CollectionFixture {
    let public_id = PublicId::new(Uuid::new_v4());
    let id: CollectionId = sqlx::query_scalar(
        "INSERT INTO collections (public_id, slug, display_name, enabled) \
         VALUES ($1, $2, $3, $4) RETURNING id",
    )
    .bind(public_id)
    .bind(slug)
    .bind(slug)
    .bind(enabled)
    .fetch_one(transaction.as_mut())
    .await
    .expect("insert collection");
    CollectionFixture { id, public_id }
}

struct CatalogItemFixture {
    id: CatalogItemId,
    public_id: PublicId,
}

async fn insert_catalog_item(
    transaction: &mut Transaction<'_, Postgres>,
    collection_id: CollectionId,
    rarity_code: &str,
    stable_name: &str,
    enabled: bool,
    min_float: &str,
    max_float: &str,
) -> CatalogItemFixture {
    let public_id = PublicId::new(Uuid::new_v4());
    let min_float = Decimal::from_str(min_float).expect("valid min_float");
    let max_float = Decimal::from_str(max_float).expect("valid max_float");
    let id: CatalogItemId = sqlx::query_scalar(
        "INSERT INTO catalog_items \
             (public_id, collection_id, rarity_code, stable_name, min_float, max_float, enabled) \
         VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING id",
    )
    .bind(public_id)
    .bind(collection_id)
    .bind(rarity_code)
    .bind(stable_name)
    .bind(min_float)
    .bind(max_float)
    .bind(enabled)
    .fetch_one(transaction.as_mut())
    .await
    .expect("insert catalog item");
    CatalogItemFixture { id, public_id }
}

struct SkuFixture {
    id: SkuId,
    public_id: PublicId,
}

async fn insert_sku(
    transaction: &mut Transaction<'_, Postgres>,
    catalog_item_id: CatalogItemId,
    wear_band_id: WearBandId,
    enabled: bool,
) -> SkuFixture {
    let public_id = PublicId::new(Uuid::new_v4());
    let id: SkuId = sqlx::query_scalar(
        "INSERT INTO skus (public_id, catalog_item_id, wear_band_id, enabled) \
         VALUES ($1, $2, $3, $4) RETURNING id",
    )
    .bind(public_id)
    .bind(catalog_item_id)
    .bind(wear_band_id)
    .bind(enabled)
    .fetch_one(transaction.as_mut())
    .await
    .expect("insert sku");
    SkuFixture { id, public_id }
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn existing_collection_item_and_sku_are_found_by_public_id() {
    let database = test_database().await;
    let mut transaction = isolated_transaction(&database).await;
    ensure_lookup_rows(&mut transaction).await;
    let (factory_new, _minimal_wear) = wear_band_ids(&mut transaction).await;

    let collection = insert_collection(&mut transaction, "found-by-id", true).await;
    let item = insert_catalog_item(
        &mut transaction,
        collection.id,
        "mil-spec",
        "Found Item",
        true,
        "0",
        "1",
    )
    .await;
    let sku = insert_sku(&mut transaction, item.id, factory_new, true).await;

    let found_collection = find_collection_by_public_id(transaction.as_mut(), collection.public_id)
        .await
        .expect("query collection")
        .expect("collection exists");
    assert_eq!(found_collection.id, collection.id);
    assert_eq!(found_collection.slug, "found-by-id");
    assert!(found_collection.enabled);

    let found_item = find_catalog_item_by_public_id(transaction.as_mut(), item.public_id)
        .await
        .expect("query catalog item")
        .expect("catalog item exists");
    assert_eq!(found_item.id, item.id);
    assert_eq!(found_item.collection_id, collection.id);
    assert_eq!(found_item.rarity_code, "mil-spec");

    let found_sku = find_sku_by_public_id(transaction.as_mut(), sku.public_id)
        .await
        .expect("query sku")
        .expect("sku exists");
    assert_eq!(found_sku.id, sku.id);
    assert_eq!(found_sku.catalog_item_id, item.id);
    assert_eq!(found_sku.wear_band_id, factory_new);

    transaction.rollback().await.expect("rollback fixture");
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn unknown_public_id_returns_none() {
    let database = test_database().await;
    let mut transaction = isolated_transaction(&database).await;

    assert!(
        find_collection_by_public_id(transaction.as_mut(), PublicId::new(Uuid::new_v4()))
            .await
            .expect("query collection")
            .is_none()
    );
    assert!(
        find_catalog_item_by_public_id(transaction.as_mut(), PublicId::new(Uuid::new_v4()))
            .await
            .expect("query catalog item")
            .is_none()
    );
    assert!(
        find_sku_by_public_id(transaction.as_mut(), PublicId::new(Uuid::new_v4()))
            .await
            .expect("query sku")
            .is_none()
    );

    transaction.rollback().await.expect("rollback fixture");
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn disabled_collection_item_and_sku_are_excluded_from_active_selection() {
    let database = test_database().await;
    let mut transaction = isolated_transaction(&database).await;
    ensure_lookup_rows(&mut transaction).await;
    let (factory_new, minimal_wear) = wear_band_ids(&mut transaction).await;

    let enabled_collection = insert_collection(&mut transaction, "active-selection", true).await;
    let disabled_collection =
        insert_collection(&mut transaction, "disabled-collection", false).await;

    let active_item = insert_catalog_item(
        &mut transaction,
        enabled_collection.id,
        "mil-spec",
        "Active Item",
        true,
        "0",
        "1",
    )
    .await;
    let disabled_item = insert_catalog_item(
        &mut transaction,
        enabled_collection.id,
        "mil-spec",
        "Disabled Item",
        false,
        "0",
        "1",
    )
    .await;
    let item_in_disabled_collection = insert_catalog_item(
        &mut transaction,
        disabled_collection.id,
        "mil-spec",
        "Item In Disabled Collection",
        true,
        "0",
        "1",
    )
    .await;

    let active_sku = insert_sku(&mut transaction, active_item.id, factory_new, true).await;
    let _disabled_sku = insert_sku(&mut transaction, active_item.id, minimal_wear, false).await;
    let _sku_under_disabled_item =
        insert_sku(&mut transaction, disabled_item.id, factory_new, true).await;
    let _sku_under_disabled_collection = insert_sku(
        &mut transaction,
        item_in_disabled_collection.id,
        factory_new,
        true,
    )
    .await;

    let active_results = find_active_skus(transaction.as_mut(), enabled_collection.id, "mil-spec")
        .await
        .expect("query active skus");
    assert_eq!(active_results.len(), 1);
    assert_eq!(active_results[0].id, active_sku.id);

    let disabled_collection_results =
        find_active_skus(transaction.as_mut(), disabled_collection.id, "mil-spec")
            .await
            .expect("query active skus for disabled collection");
    assert!(disabled_collection_results.is_empty());

    transaction.rollback().await.expect("rollback fixture");
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn active_skus_are_stably_ordered_by_sku_id() {
    let database = test_database().await;
    let mut transaction = isolated_transaction(&database).await;
    ensure_lookup_rows(&mut transaction).await;
    let (factory_new, minimal_wear) = wear_band_ids(&mut transaction).await;

    let collection = insert_collection(&mut transaction, "stable-order", true).await;
    let item_a = insert_catalog_item(
        &mut transaction,
        collection.id,
        "mil-spec",
        "Order Item A",
        true,
        "0",
        "1",
    )
    .await;
    let item_b = insert_catalog_item(
        &mut transaction,
        collection.id,
        "mil-spec",
        "Order Item B",
        true,
        "0",
        "1",
    )
    .await;

    let sku_first = insert_sku(&mut transaction, item_b.id, factory_new, true).await;
    let sku_second = insert_sku(&mut transaction, item_a.id, factory_new, true).await;
    let sku_third = insert_sku(&mut transaction, item_a.id, minimal_wear, true).await;

    let results = find_active_skus(transaction.as_mut(), collection.id, "mil-spec")
        .await
        .expect("query active skus");

    assert_eq!(
        results.iter().map(|sku| sku.id).collect::<Vec<_>>(),
        vec![sku_first.id, sku_second.id, sku_third.id]
    );

    transaction.rollback().await.expect("rollback fixture");
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn catalog_item_float_bounds_read_as_decimal_without_precision_loss() {
    let database = test_database().await;
    let mut transaction = isolated_transaction(&database).await;
    ensure_lookup_rows(&mut transaction).await;

    let collection = insert_collection(&mut transaction, "decimal-precision", true).await;
    let item = insert_catalog_item(
        &mut transaction,
        collection.id,
        "mil-spec",
        "Precision Item",
        true,
        "0.00000001",
        "0.99999999",
    )
    .await;

    let found = find_catalog_item_by_public_id(transaction.as_mut(), item.public_id)
        .await
        .expect("query catalog item")
        .expect("catalog item exists");

    assert_eq!(
        found.min_float,
        Decimal::from_str("0.00000001").expect("valid decimal")
    );
    assert_eq!(
        found.max_float,
        Decimal::from_str("0.99999999").expect("valid decimal")
    );

    transaction.rollback().await.expect("rollback fixture");
}
