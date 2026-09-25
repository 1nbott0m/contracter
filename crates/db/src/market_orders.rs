use crate::{DatabaseError, PublicId, UserId};
use sqlx::{Executor, Postgres};

#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::FromRow)]
pub struct MarketOrderResult {
    pub operation_id: PublicId,
    pub inventory_item_id: PublicId,
    pub amount_microcredits: i64,
}

pub async fn purchase_market_item<'e, E>(
    executor: E,
    user_id: UserId,
    sku_id: PublicId,
    idempotency_key: uuid::Uuid,
) -> Result<MarketOrderResult, DatabaseError>
where
    E: Executor<'e, Database = Postgres>,
{
    Ok(sqlx::query_as(
        "SELECT operation_id, inventory_item_id, amount_microcredits \
         FROM purchase_market_item_for_user($1, $2, $3)",
    )
    .bind(user_id)
    .bind(sku_id)
    .bind(idempotency_key)
    .fetch_one(executor)
    .await?)
}

pub async fn buyback_market_item<'e, E>(
    executor: E,
    user_id: UserId,
    inventory_item_id: PublicId,
    idempotency_key: uuid::Uuid,
) -> Result<MarketOrderResult, DatabaseError>
where
    E: Executor<'e, Database = Postgres>,
{
    Ok(sqlx::query_as(
        "SELECT operation_id, inventory_item_id, amount_microcredits \
         FROM buyback_market_item_for_user($1, $2, $3)",
    )
    .bind(user_id)
    .bind(inventory_item_id)
    .bind(idempotency_key)
    .fetch_one(executor)
    .await?)
}
