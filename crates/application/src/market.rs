//! Public market reads. This layer deliberately exposes stock as a boolean
//! and keeps operator risk details in the database boundary.

use db::{Database, DatabaseError, MarketPriceHalt, MarketValuation, PublicId};
use thiserror::Error;

use crate::pagination::{Cursor, Page, page_size};

#[derive(Debug, Error)]
pub enum MarketError {
    #[error("the cursor is not valid")]
    InvalidCursor,
    #[error(transparent)]
    Database(#[from] DatabaseError),
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
