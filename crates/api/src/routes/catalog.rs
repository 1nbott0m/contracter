use application::{
    catalog::{self, SkuFilters},
    pagination::{Cursor, Page},
};
use axum::{
    Json,
    extract::{Query, State},
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{error::ApiError, state::AppState};

/// The envelope every list endpoint returns.
///
/// A bare JSON array has nowhere to put a cursor, and retrofitting one
/// later is a breaking change for every client. `next_cursor` is absent
/// rather than `null` at the end of a walk, so "there is more" is a
/// question about a field's presence and not about its value.
#[derive(Debug, Serialize)]
pub struct PageResponse<T> {
    pub items: Vec<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

impl<T> PageResponse<T> {
    pub(crate) fn from_page<S>(page: Page<S>, transform: impl Fn(S) -> T) -> Self {
        let page = page.map(transform);
        Self {
            items: page.items,
            next_cursor: page.next_cursor.map(|cursor| cursor.to_string()),
        }
    }
}

/// Query parameters shared by every list endpoint.
///
/// `deny_unknown_fields` on purpose: a typo'd `?colection=` would
/// otherwise be silently ignored and answered with the unfiltered list,
/// which for a filter is the most dangerous possible default.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogQuery {
    pub cursor: Option<String>,
    pub limit: Option<u16>,
    pub collection: Option<Uuid>,
    pub item: Option<Uuid>,
    pub rarity: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PageQuery {
    pub cursor: Option<String>,
    pub limit: Option<u16>,
}

#[derive(Debug, Serialize)]
pub struct CollectionResponse {
    pub collection_id: Uuid,
    pub slug: String,
    pub display_name: String,
}

/// One browsable SKU. Every identifier is a public UUID; the float bounds
/// are strings because they are exact decimals in the database and a JSON
/// number would be read back as a binary float by most clients.
#[derive(Debug, Serialize)]
pub struct SkuResponse {
    pub sku_id: Uuid,
    pub item_id: Uuid,
    pub collection_id: Uuid,
    pub collection_slug: String,
    pub collection_display_name: String,
    pub stable_name: String,
    pub rarity: String,
    pub rarity_rank: i16,
    pub wear_band: String,
    pub min_float: String,
    pub max_float: String,
    pub is_stattrak: bool,
    pub is_souvenir: bool,
}

/// `GET /api/v1/catalog/collections` -- the public collection list.
/// Unauthenticated: nothing here is account-specific.
pub async fn collections(
    State(state): State<AppState>,
    Query(query): Query<PageQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let cursor = parse_cursor(query.cursor.as_deref())?;
    let page = catalog::list_collections(state.database(), cursor.as_ref(), query.limit).await?;

    Ok(Json(PageResponse::from_page(page, |row| {
        CollectionResponse {
            collection_id: row.public_id.get(),
            slug: row.slug,
            display_name: row.display_name,
        }
    })))
}

/// `GET /api/v1/catalog/skus` -- browsable SKUs, optionally narrowed.
pub async fn skus(
    State(state): State<AppState>,
    Query(query): Query<CatalogQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let cursor = parse_cursor(query.cursor.as_deref())?;
    let filters = SkuFilters {
        collection: query.collection.map(db::PublicId::new),
        catalog_item: query.item.map(db::PublicId::new),
        rarity: query.rarity,
    };

    let page = catalog::list_skus(state.database(), &filters, cursor.as_ref(), query.limit).await?;

    Ok(Json(PageResponse::from_page(page, |row| SkuResponse {
        sku_id: row.sku_public_id.get(),
        item_id: row.catalog_item_public_id.get(),
        collection_id: row.collection_public_id.get(),
        collection_slug: row.collection_slug,
        collection_display_name: row.collection_display_name,
        stable_name: row.stable_name,
        rarity: row.rarity_code,
        rarity_rank: row.rarity_rank,
        wear_band: row.wear_band_code,
        min_float: row.min_float.to_string(),
        max_float: row.max_float.to_string(),
        is_stattrak: row.is_stattrak,
        is_souvenir: row.is_souvenir,
    })))
}

/// Turns a present-but-empty `?cursor=` into the same rejection as a
/// corrupt one, rather than treating it as "no cursor" -- a client that
/// sent the parameter meant to continue a walk.
pub(crate) fn parse_cursor(raw: Option<&str>) -> Result<Option<Cursor>, ApiError> {
    raw.map(str::parse::<Cursor>)
        .transpose()
        .map_err(|_| ApiError::BadRequest("The cursor is not valid".to_owned()))
}
