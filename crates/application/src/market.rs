//! Public market reads. This layer deliberately exposes stock as a boolean
//! and keeps operator risk details in the database boundary.

use db::{Database, DatabaseError, MarketPriceHalt, MarketValuation, PublicId};
use thiserror::Error;

use crate::pagination::{Cursor, Page, page_size};

#[derive(Debug, Error)]
pub enum MarketError {
    #[error("the cursor is not valid")]
    InvalidCursor,
    #[error("market operation could not be completed")]
    OperationRejected,
    #[error(transparent)]
    Database(#[from] DatabaseError),
}

pub struct MarketOrder {
    pub operation_id: PublicId,
    pub inventory_item_id: PublicId,
    pub amount_microcredits: i64,
}

pub async fn purchase(
    database: &Database,
    user_id: db::UserId,
    sku_id: PublicId,
    idempotency_key: uuid::Uuid,
) -> Result<MarketOrder, MarketError> {
    let result = db::purchase_market_item(database.pool(), user_id, sku_id, idempotency_key)
        .await
        .map_err(map_operation_error)?;
    Ok(MarketOrder {
        operation_id: result.operation_id,
        inventory_item_id: result.inventory_item_id,
        amount_microcredits: result.amount_microcredits,
    })
}

pub async fn buyback(
    database: &Database,
    user_id: db::UserId,
    inventory_item_id: PublicId,
    idempotency_key: uuid::Uuid,
) -> Result<MarketOrder, MarketError> {
    let result =
        db::buyback_market_item(database.pool(), user_id, inventory_item_id, idempotency_key)
            .await
            .map_err(map_operation_error)?;
    Ok(MarketOrder {
        operation_id: result.operation_id,
        inventory_item_id: result.inventory_item_id,
        amount_microcredits: result.amount_microcredits,
    })
}

fn map_operation_error(error: DatabaseError) -> MarketError {
    match error.database_code().as_deref() {
        Some("23505" | "23514" | "42501" | "40001") => MarketError::OperationRejected,
        _ => MarketError::Database(error),
    }
}

pub async fn list_valuations(
    database: &Database,
    cursor: Option<&Cursor>,
    limit: Option<u16>,
) -> Result<Page<MarketValuation>, MarketError> {
    let limit = page_size(limit);
    let after = decode_public_id_cursor(cursor)?;
    let rows = db::list_market_valuations_after(database.pool(), after, i64::from(limit)).await?;

    Ok(Page::new(rows, limit, |row| {
        Cursor::from_public_id(row.sku_public_id)
    }))
}

pub async fn list_price_halts(
    database: &Database,
    cursor: Option<&Cursor>,
    limit: Option<u16>,
) -> Result<Page<MarketPriceHalt>, MarketError> {
    let limit = page_size(limit);
    let after = decode_public_id_cursor(cursor)?;
    let rows = db::list_market_price_halts_after(database.pool(), after, i64::from(limit)).await?;

    Ok(Page::new(rows, limit, |row| {
        Cursor::from_public_id(row.sku_public_id)
    }))
}

fn decode_public_id_cursor(cursor: Option<&Cursor>) -> Result<Option<PublicId>, MarketError> {
    cursor
        .map(Cursor::decode_public_id)
        .transpose()
        .map_err(|_| MarketError::InvalidCursor)
}
