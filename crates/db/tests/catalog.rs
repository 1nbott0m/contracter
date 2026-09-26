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

/// Catalog browsing pages forward by public id, never by the sequential
/// internal id -- a cursor is handed to clients, and a sequential one
/// would tell them how many rows exist and let them walk rows they were
/// never shown.
#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn listing_collections_pages_by_public_id_and_hides_disabled_rows() {
    let database = test_database().await;
    let mut transaction = isolated_transaction(&database).await;

    let mut expected = Vec::new();
    for index in 0..5 {
        let collection = insert_collection(&mut transaction, &format!("paged-{index}"), true).await;
        expected.push(collection.public_id);
    }
    let hidden = insert_collection(&mut transaction, "paged-disabled", false).await;
    expected.sort();

    let first = db::list_collections(transaction.as_mut(), None, 2)
        .await
        .expect("first page");
    assert_eq!(first.len(), 2, "a page is capped at the requested limit");

    // Walk in pages of two, collecting what this fixture inserted, and stop
    // once past the largest id it could possibly have inserted.
    //
    // Stopping early matters because the integration suites commit their
    // fixtures, so the collections table grows with every run and a full
    // walk costs a round trip per two rows forever. It is still a complete
    // check: the listing is ordered by public id, so no row of ours can
    // appear after the largest of them -- including the disabled one, which
    // is why that id is part of the bound.
    let upper_bound = expected
        .last()
        .copied()
        .expect("the fixture inserted rows")
        .max(hidden.public_id);
    let mut seen = Vec::new();
    let mut cursor = None;
    loop {
        let page = db::list_collections(transaction.as_mut(), cursor, 2)
            .await
            .expect("page");
        let Some(last) = page.last() else { break };
        let past_our_rows = last.public_id > upper_bound;
        cursor = Some(last.public_id);
        seen.extend(page.into_iter().map(|row| row.public_id));
        if past_our_rows {
            break;
        }
    }

    // In walk order, not sorted first. Sorting both sides before comparing
    // proves nothing was skipped or repeated, but it cannot see an order
    // that is consistently wrong -- an ORDER BY and cursor comparison
    // reversed together would still pass.
    assert!(
        seen.windows(2).all(|pair| pair[0] < pair[1]),
        "the listing must be strictly ascending by public id"
    );
    let mine: Vec<_> = seen
        .iter()
        .copied()
        .filter(|id| expected.contains(id))
        .collect();
    assert_eq!(
        mine, expected,
        "every enabled row appears exactly once, in ascending order"
    );
    assert!(
        !seen.contains(&hidden.public_id),
        "a disabled collection is not part of the public catalog"
    );

    transaction.rollback().await.expect("rollback fixture");
}

/// A SKU is only browsable when its whole chain is enabled, and the
/// projection carries the public context a client needs so no caller has
/// to issue a query per row.
#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn listing_skus_joins_public_context_and_respects_the_enabled_chain() {
    let database = test_database().await;
    let mut transaction = isolated_transaction(&database).await;
    ensure_lookup_rows(&mut transaction).await;
    let (factory_new, minimal_wear) = wear_band_ids(&mut transaction).await;

    let collection = insert_collection(&mut transaction, "sku-listing", true).await;
    let item = insert_catalog_item(
        &mut transaction,
        collection.id,
        "mil-spec",
        "Listed Item",
        true,
        "0",
        "1",
    )
    .await;
    let visible = insert_sku(&mut transaction, item.id, factory_new, true).await;
    let disabled_sku = insert_sku(&mut transaction, item.id, minimal_wear, false).await;

    let disabled_item = insert_catalog_item(
        &mut transaction,
        collection.id,
        "mil-spec",
        "Disabled Item",
        false,
        "0",
        "1",
    )
    .await;
    let sku_of_disabled_item =
        insert_sku(&mut transaction, disabled_item.id, factory_new, true).await;

    let disabled_collection = insert_collection(&mut transaction, "sku-hidden", false).await;
    let item_in_disabled_collection = insert_catalog_item(
        &mut transaction,
        disabled_collection.id,
        "mil-spec",
        "Hidden Item",
        true,
        "0",
        "1",
    )
    .await;
    let sku_in_disabled_collection = insert_sku(
        &mut transaction,
        item_in_disabled_collection.id,
        factory_new,
        true,
    )
    .await;

    let listed = db::list_catalog_skus(transaction.as_mut(), None, None, None, 200)
        .await
        .expect("list skus");
    let found = listed
        .iter()
        .find(|row| row.sku_public_id == visible.public_id)
        .expect("the fully enabled sku is listed");

    assert_eq!(found.collection_public_id, collection.public_id);
    assert_eq!(found.collection_display_name, "sku-listing");
    assert_eq!(found.catalog_item_public_id, item.public_id);
    assert_eq!(found.stable_name, "Listed Item");
    assert_eq!(found.rarity_code, "mil-spec");
    assert_eq!(found.wear_band_code, "factory_new");

    for hidden in [
        disabled_sku.public_id,
        sku_of_disabled_item.public_id,
        sku_in_disabled_collection.public_id,
    ] {
        assert!(
            !listed.iter().any(|row| row.sku_public_id == hidden),
            "a disabled link anywhere in the chain hides the sku"
        );
    }

    // Filters narrow rather than widen: an unrelated collection's skus
    // never appear under this collection's filter.
    let filtered = db::list_catalog_skus(
        transaction.as_mut(),
        Some(collection.public_id),
        Some("mil-spec"),
        None,
        200,
    )
    .await
    .expect("filtered list");
    assert!(
        filtered
            .iter()
            .all(|row| row.collection_public_id == collection.public_id),
        "the collection filter is applied in SQL, not hoped for"
    );
    assert!(
        filtered
            .iter()
            .any(|row| row.sku_public_id == visible.public_id)
    );

    let empty = db::list_catalog_skus(
        transaction.as_mut(),
        Some(collection.public_id),
        Some("no-such-rarity"),
        None,
        200,
    )
    .await
    .expect("filtered list");
    assert!(empty.is_empty(), "an unmatched filter returns nothing");

    transaction.rollback().await.expect("rollback fixture");
}
