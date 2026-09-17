//! The caller's own inventory.
//!
//! Every function here takes a `UserId` that the HTTP layer can only
//! obtain from a resolved session. There is no variant that accepts an
//! owner from the request, so "read someone else's inventory" is not an
//! operation this module can express.

use db::{Database, DatabaseError, OwnedInventoryItem, PublicId, UserId};
use thiserror::Error;

use crate::pagination::{Cursor, Page, page_size};

const MAX_RARITY_CODE_LENGTH: usize = 64;

#[derive(Debug, Error)]
pub enum InventoryError {
    #[error("the cursor is not valid")]
    InvalidCursor,
    #[error("the filter is not valid")]
    InvalidFilter,
    #[error("the item does not exist")]
    NotFound,
    #[error(transparent)]
    Database(#[from] DatabaseError),
}

#[derive(Debug, Clone, Default)]
pub struct InventoryFilters {
    pub collection: Option<PublicId>,
    pub rarity: Option<String>,
}

/// One page of the caller's own items, newest first.
pub async fn list_owned(
    database: &Database,
    owner: UserId,
    filters: &InventoryFilters,
    cursor: Option<&Cursor>,
    limit: Option<u16>,
) -> Result<Page<OwnedInventoryItem>, InventoryError> {
    let limit = page_size(limit);

    let after = cursor
        .map(Cursor::decode_timestamped)
        .transpose()
        .map_err(|_| InventoryError::InvalidCursor)?;

    if let Some(rarity) = &filters.rarity
        && (rarity.is_empty() || rarity.len() > MAX_RARITY_CODE_LENGTH)
    {
        return Err(InventoryError::InvalidFilter);
    }

    let rows = db::list_owned_inventory(
        database.pool(),
        owner,
        filters.collection,
        filters.rarity.as_deref(),
        after,
        i64::from(limit),
    )
    .await?;

    Ok(Page::new(rows, limit, |row| {
        Cursor::from_timestamped(row.created_at, row.public_id)
    }))
}

/// One of the caller's own items.
///
/// An item owned by someone else yields `NotFound`, identical to one that
/// never existed. Distinguishing them would turn this endpoint into an
/// oracle for which UUIDs are real, and there is nothing a caller can
/// legitimately do with that answer.
pub async fn find_owned(
    database: &Database,
    owner: UserId,
    public_id: PublicId,
) -> Result<OwnedInventoryItem, InventoryError> {
    db::find_owned_inventory_item(database.pool(), owner, public_id)
        .await?
        .ok_or(InventoryError::NotFound)
}
