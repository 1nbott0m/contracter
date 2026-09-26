use rust_decimal::Decimal;
use sqlx::{
    Executor, Postgres,
    types::chrono::{DateTime, Utc},
};

use crate::{DatabaseError, InventoryItemId, PublicId, SkuId, UserId};

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct InventoryItem {
    pub id: InventoryItemId,
    pub public_id: PublicId,
    pub sku_id: SkuId,
    pub canonical_float: Decimal,
    pub created_at: DateTime<Utc>,
    pub retired_at: Option<DateTime<Utc>>,
    pub owner_user_id: Option<UserId>,
    pub in_warehouse: bool,
    pub position_version: i64,
    pub position_updated_at: DateTime<Utc>,
}

pub async fn find_inventory_item<'e, E>(
    executor: E,
    public_id: PublicId,
) -> Result<Option<InventoryItem>, DatabaseError>
where
    E: Executor<'e, Database = Postgres>,
{
    Ok(sqlx::query_as(
        "SELECT item.id, item.public_id, item.sku_id, item.canonical_float, \
                item.created_at, item.retired_at, position.owner_user_id, \
                position.in_warehouse, position.version AS position_version, \
                position.updated_at AS position_updated_at \
         FROM inventory_items AS item \
         JOIN inventory_positions AS position ON position.inventory_item_id = item.id \
         WHERE item.public_id = $1",
    )
    .bind(public_id)
    .fetch_optional(executor)
    .await?)
}

pub async fn list_available_user_inventory<'e, E>(
    executor: E,
    owner_user_id: UserId,
) -> Result<Vec<InventoryItem>, DatabaseError>
where
    E: Executor<'e, Database = Postgres>,
{
    Ok(sqlx::query_as(
        "SELECT id, public_id, sku_id, canonical_float, created_at, retired_at, \
                owner_user_id, in_warehouse, position_version, position_updated_at \
         FROM available_user_inventory \
         WHERE owner_user_id = $1 \
         ORDER BY id",
    )
    .bind(owner_user_id)
    .fetch_all(executor)
    .await?)
}

pub async fn list_available_warehouse_inventory<'e, E>(
    executor: E,
) -> Result<Vec<InventoryItem>, DatabaseError>
where
    E: Executor<'e, Database = Postgres>,
{
    Ok(sqlx::query_as(
        "SELECT id, public_id, sku_id, canonical_float, created_at, retired_at, \
                owner_user_id, in_warehouse, position_version, position_updated_at \
         FROM available_warehouse_inventory \
         ORDER BY id",
    )
    .fetch_all(executor)
    .await?)
}

/// One item as its owner sees it, with the public catalog context joined
/// in and no internal identifier anywhere in the projection.
///
/// `locked` rather than absence: `available_user_inventory` hides a
/// locked item, which is correct for "what can be spent" and wrong for
/// "what do I own" -- an item silently vanishing while it is reserved in
/// a quote reads as theft. Retired items are excluded outright, because
/// those are gone rather than reserved.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct OwnedInventoryItem {
    pub public_id: PublicId,
    pub created_at: DateTime<Utc>,
    pub canonical_float: Decimal,
    pub locked: bool,
    pub sku_public_id: PublicId,
    pub catalog_item_public_id: PublicId,
    pub collection_public_id: PublicId,
    pub collection_display_name: String,
    pub stable_name: String,
    pub rarity_code: String,
    pub wear_band_code: String,
    pub is_stattrak: bool,
    pub is_souvenir: bool,
}

/// The SELECT list shared by the owner's listing and the owner's
/// single-item read, so the two can never disagree about what an owned
/// item is or which columns it exposes.
///
/// Reads `runtime_owned_inventory` (migration 0056) rather than joining the base
/// tables here. The `locked` flag needs `inventory_item_locks`, which
/// `contracter_runtime` deliberately cannot read -- and an `EXISTS`
/// subquery in that role's own statement is still subject to that role's
/// privileges. Only a view runs with its owner's rights, so the view is
/// what keeps the guard table unreadable while still reporting the flag.
///
/// A macro rather than a `const`, because SQLx 0.9 accepts only
/// `&'static str` -- a deliberate guardrail against queries assembled at
/// runtime. `concat!` over macro-expanded literals keeps the shared text
/// in one place *and* keeps every query a compile-time constant, so the
/// guardrail stays on rather than being waved away with `AssertSqlSafe`.
///
/// `$1` is reserved for the owner. A caller adding a filter starts at
/// `$2`.
macro_rules! owned_inventory_projection {
    () => {
        "SELECT public_id, \
                created_at, \
                canonical_float, \
                locked, \
                sku_public_id, \
                catalog_item_public_id, \
                collection_public_id, \
                collection_display_name, \
                stable_name, \
                rarity_code, \
                wear_band_code, \
                is_stattrak, \
                is_souvenir \
         FROM runtime_owned_inventory \
         WHERE owner_user_id = $1"
    };
}

/// One page of the caller's own inventory, newest first.
///
/// The owner is a bound predicate in the query, never a filter applied
/// afterwards, so there is no code path where a caller who knows another
/// account's item UUID reads it. Filters are compared against `NULL` to
/// mean "unfiltered", so none of this SQL is assembled from caller input.
///
/// Paging is keyset on `(created_at, public_id)` descending. `created_at`
/// alone is not unique -- several items can be created in one transaction
/// and share a timestamp exactly -- so the public id breaks the tie and
/// makes the order total. Without that, a page boundary landing inside a
/// group of equal timestamps would skip or repeat rows.
pub async fn list_owned_inventory<'e, E>(
    executor: E,
    owner_user_id: UserId,
    collection_public_id: Option<PublicId>,
    rarity_code: Option<&str>,
    after: Option<(DateTime<Utc>, PublicId)>,
    limit: i64,
) -> Result<Vec<OwnedInventoryItem>, DatabaseError>
where
    E: Executor<'e, Database = Postgres>,
{
    let (after_created_at, after_public_id) = match after {
        Some((created_at, public_id)) => (Some(created_at), Some(public_id)),
        None => (None, None),
    };

    Ok(sqlx::query_as(concat!(
        owned_inventory_projection!(),
        " AND ($2::uuid IS NULL OR collection_public_id = $2) \
           AND ($3::text IS NULL OR rarity_code = $3) \
           AND ( \
               $4::timestamptz IS NULL \
               OR (created_at, public_id) < ($4, $5) \
           ) \
         ORDER BY created_at DESC, public_id DESC \
         LIMIT $6"
    ))
    .bind(owner_user_id)
    .bind(collection_public_id)
    .bind(rarity_code)
    .bind(after_created_at)
    .bind(after_public_id)
    .bind(limit)
    .fetch_all(executor)
    .await?)
}

/// One of the caller's own items by its public id.
///
/// Returns `None` for an item that belongs to someone else, exactly as
/// for one that does not exist: knowing a UUID is not authorization, and
/// the two cases must be indistinguishable or the endpoint becomes an
/// ownership oracle.
pub async fn find_owned_inventory_item<'e, E>(
    executor: E,
    owner_user_id: UserId,
    public_id: PublicId,
) -> Result<Option<OwnedInventoryItem>, DatabaseError>
where
    E: Executor<'e, Database = Postgres>,
{
    Ok(sqlx::query_as(concat!(
        owned_inventory_projection!(),
        " AND public_id = $2"
    ))
    .bind(owner_user_id)
    .bind(public_id)
    .fetch_optional(executor)
    .await?)
}
