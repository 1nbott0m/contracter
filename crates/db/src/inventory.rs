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
