use application::inventory::{self, InventoryFilters};
use axum::{
    Json,
    extract::{Path, Query, State},
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    error::ApiError,
    extract::CurrentUser,
    routes::catalog::{PageResponse, parse_cursor},
    state::AppState,
};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InventoryQuery {
    pub cursor: Option<String>,
    pub limit: Option<u16>,
    pub collection: Option<Uuid>,
    pub rarity: Option<String>,
}

/// One owned item.
///
/// `locked` is reported rather than the item being hidden: an item that
/// disappears from its owner's view while it is reserved in a quote
/// reads as theft, and the owner has a legitimate interest in knowing
/// which of their items are currently spoken for.
///
/// `canonical_float` is a string because it is an exact decimal in the
/// database; a JSON number would be read back as a binary float by most
/// clients, and float is the one thing this item's identity turns on.
#[derive(Debug, Serialize)]
pub struct InventoryItemResponse {
    pub item_id: Uuid,
    pub sku_id: Uuid,
    pub catalog_item_id: Uuid,
    pub collection_id: Uuid,
    pub collection_display_name: String,
    pub stable_name: String,
    pub rarity: String,
    pub wear_band: String,
    pub canonical_float: String,
    pub is_stattrak: bool,
    pub is_souvenir: bool,
    pub locked: bool,
    pub acquired_at: String,
}

fn into_response_item(row: db::OwnedInventoryItem) -> InventoryItemResponse {
    InventoryItemResponse {
        item_id: row.public_id.get(),
        sku_id: row.sku_public_id.get(),
        catalog_item_id: row.catalog_item_public_id.get(),
        collection_id: row.collection_public_id.get(),
        collection_display_name: row.collection_display_name,
        stable_name: row.stable_name,
        rarity: row.rarity_code,
        wear_band: row.wear_band_code,
        canonical_float: row.canonical_float.to_string(),
        is_stattrak: row.is_stattrak,
        is_souvenir: row.is_souvenir,
        locked: row.locked,
        acquired_at: row.created_at.to_rfc3339(),
    }
}

/// `GET /api/v1/me/inventory` -- the caller's own items.
///
/// Addressed as `/me`, not `/users/{id}/inventory`: with no owner in the
/// path there is nothing for a client to tamper with, and the owner comes
/// from `CurrentUser`, which has no constructor taking a client-supplied
/// id.
pub async fn list(
    State(state): State<AppState>,
    CurrentUser(caller): CurrentUser,
    Query(query): Query<InventoryQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let cursor = parse_cursor(query.cursor.as_deref())?;
    let filters = InventoryFilters {
        collection: query.collection.map(db::PublicId::new),
        rarity: query.rarity,
    };

    let page = inventory::list_owned(
        state.database(),
        caller.user_id,
        &filters,
        cursor.as_ref(),
        query.limit,
    )
    .await?;

    Ok(Json(PageResponse::from_page(page, into_response_item)))
}

/// `GET /api/v1/me/inventory/{item_id}` -- one of the caller's own items.
///
/// The path carries an item UUID, which is a thing a client can tamper
/// with; ownership is therefore a predicate in the query rather than a
/// check performed afterwards. Another account's item is `404`, exactly
/// as a nonexistent one is.
pub async fn detail(
    State(state): State<AppState>,
    CurrentUser(caller): CurrentUser,
    Path(item_id): Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let item =
        inventory::find_owned(state.database(), caller.user_id, db::PublicId::new(item_id)).await?;

    Ok(Json(into_response_item(item)))
}
