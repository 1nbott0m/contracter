use crate::extract::CurrentUser;
use application::{market, pagination::Page};
use axum::{
    Json,
    extract::{Path, Query, State},
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};

use crate::{
    INTERNAL_CURRENCY_CODE,
    error::ApiError,
    routes::catalog::{PageResponse, parse_cursor},
    state::AppState,
};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MarketQuery {
    pub cursor: Option<String>,
    pub limit: Option<u16>,
}

#[derive(Debug, Serialize)]
pub struct MarketValuationResponse {
    pub sku_id: uuid::Uuid,
    pub price_microcredits: i64,
    pub currency_code: &'static str,
    pub updated_at: String,
    pub available: bool,
}

#[derive(Debug, Serialize)]
pub struct MarketPriceHaltResponse {
    pub sku_id: uuid::Uuid,
    pub halted_at: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MarketOrderRequest {
    pub idempotency_key: uuid::Uuid,
}

#[derive(Debug, Serialize)]
pub struct MarketOrderResponse {
    pub operation_id: uuid::Uuid,
    pub inventory_item_id: uuid::Uuid,
    pub amount_microcredits: i64,
    pub currency_code: &'static str,
}

pub async fn purchase(
    State(state): State<AppState>,
    CurrentUser(caller): CurrentUser,
    Path(sku_id): Path<uuid::Uuid>,
    Json(request): Json<MarketOrderRequest>,
) -> Result<Json<MarketOrderResponse>, ApiError> {
    let order = market::purchase(
        state.database(),
        caller.user_id,
        db::PublicId::new(sku_id),
        request.idempotency_key,
    )
    .await?;
    Ok(Json(MarketOrderResponse {
        operation_id: order.operation_id.get(),
        inventory_item_id: order.inventory_item_id.get(),
        amount_microcredits: order.amount_microcredits,
        currency_code: INTERNAL_CURRENCY_CODE,
    }))
}

pub async fn buyback(
    State(state): State<AppState>,
    CurrentUser(caller): CurrentUser,
    Path(item_id): Path<uuid::Uuid>,
    Json(request): Json<MarketOrderRequest>,
) -> Result<Json<MarketOrderResponse>, ApiError> {
    let order = market::buyback(
        state.database(),
        caller.user_id,
        db::PublicId::new(item_id),
        request.idempotency_key,
    )
    .await?;
    Ok(Json(MarketOrderResponse {
        operation_id: order.operation_id.get(),
        inventory_item_id: order.inventory_item_id.get(),
        amount_microcredits: order.amount_microcredits,
        currency_code: INTERNAL_CURRENCY_CODE,
    }))
}

/// `GET /api/v1/market/valuations` -- current public prices. Quantities and
/// reservations are intentionally reduced to `available`.
pub async fn valuations(
    State(state): State<AppState>,
    Query(query): Query<MarketQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let cursor = parse_cursor(query.cursor.as_deref())?;
    let page = market::list_valuations(state.database(), cursor.as_ref(), query.limit).await?;

    Ok(Json(page_response(page, |row| MarketValuationResponse {
        sku_id: row.sku_public_id.get(),
        price_microcredits: row.verified_price_microcredits,
        currency_code: INTERNAL_CURRENCY_CODE,
        updated_at: row.updated_at.to_rfc3339(),
        available: row.available,
    })))
}

/// `GET /api/v1/market/price-halts` -- active public suspensions. Reasons
/// and anomaly ratios stay internal so this endpoint cannot expose the risk
/// model behind a halt.
pub async fn price_halts(
    State(state): State<AppState>,
    Query(query): Query<MarketQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let cursor = parse_cursor(query.cursor.as_deref())?;
    let page = market::list_price_halts(state.database(), cursor.as_ref(), query.limit).await?;

    Ok(Json(page_response(page, |row| MarketPriceHaltResponse {
        sku_id: row.sku_public_id.get(),
        halted_at: row.halted_at.to_rfc3339(),
    })))
}

fn page_response<S, T>(page: Page<S>, transform: impl Fn(S) -> T) -> PageResponse<T> {
    PageResponse::from_page(page, transform)
}
