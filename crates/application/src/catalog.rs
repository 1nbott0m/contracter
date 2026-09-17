//! Catalog browsing: what exists in the game world, independent of who
//! owns it.
//!
//! Read-only and unauthenticated by design. Nothing here is
//! account-specific, so nothing here needs an identity -- and an endpoint
//! that needs no identity cannot leak one account's data to another.

use db::{CatalogSku, Collection, Database, DatabaseError, PublicId};
use thiserror::Error;

use crate::pagination::{Cursor, Page, page_size};

/// The longest a rarity code may be before it is refused unexamined.
///
/// The code goes to the database as a bound parameter either way, so this
/// is not an injection defence; it stops a caller shipping a megabyte of
/// text per request and making PostgreSQL compare it row by row.
const MAX_RARITY_CODE_LENGTH: usize = 64;

#[derive(Debug, Error)]
pub enum CatalogError {
    #[error("the cursor is not valid")]
    InvalidCursor,
    #[error("the filter is not valid")]
    InvalidFilter,
    #[error(transparent)]
    Database(#[from] DatabaseError),
}

/// Filters for a SKU listing. Every field is optional and narrows the
/// result; none of them can widen it beyond what is publicly browsable.
#[derive(Debug, Clone, Default)]
pub struct SkuFilters {
    pub collection: Option<PublicId>,
    pub catalog_item: Option<PublicId>,
    pub rarity: Option<String>,
}

/// One page of enabled collections.
pub async fn list_collections(
    database: &Database,
    cursor: Option<&Cursor>,
    limit: Option<u16>,
) -> Result<Page<Collection>, CatalogError> {
    let limit = page_size(limit);
    let after = decode_public_id_cursor(cursor)?;

    let rows = db::list_collections(database.pool(), after, i64::from(limit)).await?;
    Ok(Page::new(rows, limit, |row| {
        Cursor::from_public_id(row.public_id)
    }))
}

/// One page of browsable SKUs, narrowed by `filters`.
pub async fn list_skus(
    database: &Database,
    filters: &SkuFilters,
    cursor: Option<&Cursor>,
    limit: Option<u16>,
) -> Result<Page<CatalogSku>, CatalogError> {
    let limit = page_size(limit);
    let after = decode_public_id_cursor(cursor)?;

    if let Some(rarity) = &filters.rarity
        && (rarity.is_empty() || rarity.len() > MAX_RARITY_CODE_LENGTH)
    {
        return Err(CatalogError::InvalidFilter);
    }

    let rows = db::list_catalog_skus_after(
        database.pool(),
        filters.collection,
        filters.rarity.as_deref(),
        filters.catalog_item,
        after,
        i64::from(limit),
    )
    .await?;

    Ok(Page::new(rows, limit, |row| {
        Cursor::from_public_id(row.sku_public_id)
    }))
}

fn decode_public_id_cursor(cursor: Option<&Cursor>) -> Result<Option<PublicId>, CatalogError> {
    cursor
        .map(Cursor::decode_public_id)
        .transpose()
        .map_err(|_| CatalogError::InvalidCursor)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_corrupt_cursor_is_refused_before_any_query_runs() {
        let cursor: Cursor = "not-a-real-cursor".parse().expect("non-empty");
        assert!(matches!(
            decode_public_id_cursor(Some(&cursor)),
            Err(CatalogError::InvalidCursor)
        ));
        assert!(matches!(decode_public_id_cursor(None), Ok(None)));
    }
}
