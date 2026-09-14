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
                min_float, max_float, enabled, is_stattrak, is_souvenir \
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
