use rust_decimal::Decimal;
use sqlx::{Executor, Postgres};

use crate::{CatalogItemId, CollectionId, DatabaseError, PublicId, SkuId, WearBandId};

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct Collection {
    pub id: CollectionId,
    pub public_id: PublicId,
    pub slug: String,
    pub display_name: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct CatalogItem {
    pub id: CatalogItemId,
    pub public_id: PublicId,
    pub collection_id: CollectionId,
    pub rarity_code: String,
    pub stable_name: String,
    pub min_float: Decimal,
    pub max_float: Decimal,
    pub enabled: bool,
    pub is_stattrak: bool,
    pub is_souvenir: bool,
    pub canonical_skin_id: Option<String>,
    pub weapon_name: Option<String>,
    pub skin_name: Option<String>,
    pub canonical_image_url: Option<String>,
    pub available_wears: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct Sku {
    pub id: SkuId,
    pub public_id: PublicId,
    pub catalog_item_id: CatalogItemId,
    pub wear_band_id: WearBandId,
    pub enabled: bool,
}

pub async fn find_collection_by_public_id<'e, E>(
    executor: E,
    public_id: PublicId,
) -> Result<Option<Collection>, DatabaseError>
where
    E: Executor<'e, Database = Postgres>,
{
    Ok(sqlx::query_as(
        "SELECT id, public_id, slug, display_name, enabled \
         FROM collections WHERE public_id = $1",
    )
    .bind(public_id)
    .fetch_optional(executor)
    .await?)
}

pub async fn find_catalog_item_by_public_id<'e, E>(
    executor: E,
    public_id: PublicId,
) -> Result<Option<CatalogItem>, DatabaseError>
where
    E: Executor<'e, Database = Postgres>,
{
    Ok(sqlx::query_as(
        "SELECT id, public_id, collection_id, rarity_code, stable_name, \
                min_float, max_float, enabled, is_stattrak, is_souvenir, \
                canonical_skin_id, weapon_name, skin_name, canonical_image_url, available_wears \
         FROM catalog_items WHERE public_id = $1",
    )
    .bind(public_id)
    .fetch_optional(executor)
    .await?)
}

pub async fn find_sku_by_public_id<'e, E>(
    executor: E,
    public_id: PublicId,
) -> Result<Option<Sku>, DatabaseError>
where
    E: Executor<'e, Database = Postgres>,
{
    Ok(sqlx::query_as(
        "SELECT id, public_id, catalog_item_id, wear_band_id, enabled \
         FROM skus WHERE public_id = $1",
    )
    .bind(public_id)
    .fetch_optional(executor)
    .await?)
}

/// Active SKUs of one collection and rarity, ordered stably by `sku.id`.
/// A SKU is active only when its collection, catalog item, and SKU row are
/// all enabled.
pub async fn find_active_skus<'e, E>(
    executor: E,
    collection_id: CollectionId,
    rarity_code: &str,
) -> Result<Vec<Sku>, DatabaseError>
where
    E: Executor<'e, Database = Postgres>,
{
    Ok(sqlx::query_as(
        "SELECT skus.id, skus.public_id, skus.catalog_item_id, skus.wear_band_id, skus.enabled \
         FROM skus \
         JOIN catalog_items ON catalog_items.id = skus.catalog_item_id \
         JOIN collections ON collections.id = catalog_items.collection_id \
         WHERE collections.id = $1 \
           AND catalog_items.rarity_code = $2 \
           AND collections.enabled \
           AND catalog_items.enabled \
           AND skus.enabled \
         ORDER BY skus.id",
    )
    .bind(collection_id)
    .bind(rarity_code)
    .fetch_all(executor)
    .await?)
}

/// One browsable SKU with the public context a client needs to render it.
///
/// Flattened on purpose: a list endpoint would otherwise issue a query
/// per row to resolve its item, collection, and wear band. Every
/// identifier here is a public UUID -- no internal sequential id appears
/// in this projection at all, so nothing downstream can leak one by
/// forgetting to map it.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct CatalogSku {
    pub sku_public_id: PublicId,
    pub catalog_item_public_id: PublicId,
    pub collection_public_id: PublicId,
    pub collection_slug: String,
    pub collection_display_name: String,
    pub stable_name: String,
    pub rarity_code: String,
    pub rarity_rank: i16,
    pub wear_band_code: String,
    pub min_float: Decimal,
    pub max_float: Decimal,
    pub is_stattrak: bool,
    pub is_souvenir: bool,
    pub canonical_skin_id: Option<String>,
    pub weapon_name: Option<String>,
    pub skin_name: Option<String>,
    pub canonical_image_url: Option<String>,
    pub available_wears: serde_json::Value,
}

/// A collection as the public catalog exposes it.
///
/// Separate from `Collection` because that struct carries the internal
/// `CollectionId`, which has no business crossing the application
/// boundary. `CatalogSku` and `OwnedInventoryItem` are built the same
/// way: if a projection contains no internal id, nothing downstream can
/// leak one by forgetting to map it.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct CatalogCollection {
    pub public_id: PublicId,
    pub slug: String,
    pub display_name: String,
}

/// One page of the public collection list, ordered and paged by
/// `public_id`.
///
/// Keyset pagination on the public UUID rather than `OFFSET` or the
/// sequential `id`: `OFFSET` skips or repeats rows when the underlying
/// data changes between pages, and a sequential cursor would hand
/// clients a row count and a way to walk rows they were never shown.
/// `public_id` is already unique and indexed, so this is also the cheaper
/// plan.
pub async fn list_collections<'e, E>(
    executor: E,
    after: Option<PublicId>,
    limit: i64,
) -> Result<Vec<CatalogCollection>, DatabaseError>
where
    E: Executor<'e, Database = Postgres>,
{
    Ok(sqlx::query_as(
        "SELECT public_id, slug, display_name \
         FROM collections \
         WHERE enabled \
           AND ($1::uuid IS NULL OR public_id > $1) \
         ORDER BY public_id \
         LIMIT $2",
    )
    .bind(after)
    .bind(limit)
    .fetch_all(executor)
    .await?)
}

/// One page of browsable SKUs, optionally narrowed by collection, rarity,
/// or catalog item.
///
/// A SKU is browsable only when its collection, catalog item, and SKU row
/// are all enabled -- a disabled link anywhere in the chain hides it,
/// which is why this is one query with the whole chain joined rather than
/// a filter applied afterwards in Rust.
///
/// Every filter is a bound parameter compared against `NULL` to mean
/// "unfiltered", so no part of this SQL is ever assembled from caller
/// input.
pub async fn list_catalog_skus<'e, E>(
    executor: E,
    collection_public_id: Option<PublicId>,
    rarity_code: Option<&str>,
    catalog_item_public_id: Option<PublicId>,
    limit: i64,
) -> Result<Vec<CatalogSku>, DatabaseError>
where
    E: Executor<'e, Database = Postgres>,
{
    list_catalog_skus_after(
        executor,
        collection_public_id,
        rarity_code,
        catalog_item_public_id,
        None,
        limit,
    )
    .await
}

/// `list_catalog_skus`, continued from a cursor.
pub async fn list_catalog_skus_after<'e, E>(
    executor: E,
    collection_public_id: Option<PublicId>,
    rarity_code: Option<&str>,
    catalog_item_public_id: Option<PublicId>,
    after: Option<PublicId>,
    limit: i64,
) -> Result<Vec<CatalogSku>, DatabaseError>
where
    E: Executor<'e, Database = Postgres>,
{
    Ok(sqlx::query_as(
        "SELECT skus.public_id AS sku_public_id, catalog_items.public_id AS catalog_item_public_id, collections.public_id AS collection_public_id, collections.slug AS collection_slug, collections.display_name AS collection_display_name, catalog_items.stable_name, catalog_items.rarity_code, rarities.rank AS rarity_rank, wear_bands.code AS wear_band_code, catalog_items.min_float, catalog_items.max_float, catalog_items.is_stattrak, catalog_items.is_souvenir, catalog_items.canonical_skin_id, catalog_items.weapon_name, catalog_items.skin_name, catalog_items.canonical_image_url, catalog_items.available_wears FROM skus JOIN catalog_items ON catalog_items.id = skus.catalog_item_id JOIN collections ON collections.id = catalog_items.collection_id JOIN rarities ON rarities.code = catalog_items.rarity_code JOIN wear_bands ON wear_bands.id = skus.wear_band_id WHERE collections.enabled AND catalog_items.enabled AND skus.enabled AND ($1::uuid IS NULL OR collections.public_id = $1) AND ($2::text IS NULL OR catalog_items.rarity_code = $2) AND ($3::uuid IS NULL OR catalog_items.public_id = $3) AND ($4::uuid IS NULL OR skus.public_id > $4) ORDER BY skus.public_id LIMIT $5",
    )
    .bind(collection_public_id)
    .bind(rarity_code)
    .bind(catalog_item_public_id)
    .bind(after)
    .bind(limit)
    .fetch_all(executor)
    .await?)
}
