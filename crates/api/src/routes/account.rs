use application::auth;
use axum::{Json, extract::State, response::IntoResponse};
use serde::Serialize;
use uuid::Uuid;

use crate::{error::ApiError, extract::CurrentUser, state::AppState};

#[derive(Debug, Serialize)]
pub struct AccountResponse {
    pub user_id: Uuid,
    pub login: String,
    pub created_at: String,
}

#[derive(Debug, Serialize)]
pub struct BalanceResponse {
    /// Integer microcredits, matching the ledger exactly. Never a float
    /// and never rounded here -- display formatting is a client concern.
    pub balance_microcredits: i64,
}

/// `GET /api/v1/me` -- the caller's own account. There is no
/// `/api/v1/users/{id}` counterpart in this milestone, so there is no id
/// for a client to tamper with.
pub async fn me(
    State(state): State<AppState>,
    CurrentUser(caller): CurrentUser,
) -> Result<impl IntoResponse, ApiError> {
    let account = auth::account(state.database(), caller.user_public_id).await?;

    Ok(Json(AccountResponse {
        user_id: account.user_public_id.get(),
        login: account.login,
        created_at: account.created_at.to_rfc3339(),
    }))
}

/// `GET /api/v1/me/balance` -- the caller's own ledger-derived balance.
/// The account is taken from the session, so one caller can never read
/// another's balance.
pub async fn balance(
    State(state): State<AppState>,
    CurrentUser(caller): CurrentUser,
) -> Result<impl IntoResponse, ApiError> {
    let balance_microcredits = auth::balance(state.database(), caller.user_public_id).await?;

    Ok(Json(BalanceResponse {
        balance_microcredits,
    }))
}
