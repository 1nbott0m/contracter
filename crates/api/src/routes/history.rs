use crate::{error::ApiError, extract::CurrentUser, state::AppState};
use axum::{
    Json,
    extract::{Query, State},
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoryQuery {
    pub cursor: Option<Uuid>,
    pub limit: Option<u16>,
}
fn limit(q: &HistoryQuery) -> i64 {
    i64::from(q.limit.unwrap_or(50).clamp(1, 200))
}
#[derive(Debug, Serialize)]
pub struct ContractItem {
    pub contract_id: Uuid,
    pub status: String,
    pub created_at: String,
    pub ledger_transaction_id: Option<i64>,
}
#[derive(Debug, Serialize)]
pub struct LedgerItem {
    pub transaction_id: Uuid,
    pub operation: String,
    pub amount_microcredits: i64,
    pub occurred_at: String,
}
#[derive(Debug, Serialize)]
pub struct InventoryEventItem {
    pub event_id: Uuid,
    pub inventory_item_id: Uuid,
    pub event_kind: String,
    pub operation_id: Uuid,
    pub occurred_at: String,
}
pub async fn contracts(
    State(s): State<AppState>,
    CurrentUser(u): CurrentUser,
    Query(q): Query<HistoryQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let rows = db::list_contract_history(s.database().pool(), u.user_id, q.cursor, limit(&q))
        .await
        .map_err(|_| ApiError::Internal)?;
    Ok(Json(
        rows.into_iter()
            .map(|r| ContractItem {
                contract_id: r.public_id,
                status: r.status_code,
                created_at: r.created_at.to_rfc3339(),
                ledger_transaction_id: r.ledger_transaction_id,
            })
            .collect::<Vec<_>>(),
    ))
}
pub async fn ledger(
    State(s): State<AppState>,
    CurrentUser(u): CurrentUser,
    Query(q): Query<HistoryQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let rows = db::list_ledger_history(s.database().pool(), u.user_id, q.cursor, limit(&q))
        .await
        .map_err(|_| ApiError::Internal)?;
    Ok(Json(
        rows.into_iter()
            .map(|r| LedgerItem {
                transaction_id: r.public_id,
                operation: r.operation_kind,
                amount_microcredits: r.amount_microcredits,
                occurred_at: r.occurred_at.to_rfc3339(),
            })
            .collect::<Vec<_>>(),
    ))
}
pub async fn inventory_events(
    State(s): State<AppState>,
    CurrentUser(u): CurrentUser,
    Query(q): Query<HistoryQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let rows =
        db::list_inventory_event_history(s.database().pool(), u.user_id, q.cursor, limit(&q))
            .await
            .map_err(|_| ApiError::Internal)?;
    Ok(Json(
        rows.into_iter()
            .map(|r| InventoryEventItem {
                event_id: r.public_id,
                inventory_item_id: r.inventory_item_id,
                event_kind: r.event_kind_code,
                operation_id: r.operation_public_id,
                occurred_at: r.occurred_at.to_rfc3339(),
            })
            .collect::<Vec<_>>(),
    ))
}
