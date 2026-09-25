use application::auth;
use axum::{Json, extract::State, response::IntoResponse};
use db;
use serde::Serialize;
use uuid::Uuid;

use crate::{error::ApiError, extract::CurrentUser, state::AppState};

#[derive(Debug, Serialize)]
pub struct AdminMeResponse {
    pub user_id: Uuid,
    pub is_admin: bool,
    pub totp_verified: bool,
}

#[derive(Debug, Serialize)]
pub struct AdminUserResponse {
    pub user_id: Uuid,
    pub login: String,
    pub created_at: String,
    pub disabled: bool,
    pub is_admin: bool,
}

pub async fn users(
    State(state): State<AppState>,
    CurrentUser(caller): CurrentUser,
) -> Result<impl IntoResponse, ApiError> {
    if !auth::is_active_administrator(state.database(), caller.user_public_id).await? {
        return Err(ApiError::Forbidden(
            "Administrator access required".to_owned(),
        ));
    }
    let users = db::admin_users(state.database().pool())
        .await
        .map_err(application::auth::AuthError::from)?;
    Ok(Json(
        users
            .into_iter()
            .map(|user| AdminUserResponse {
                user_id: user.user_id.get(),
                login: user.login,
                created_at: user.created_at.to_rfc3339(),
                disabled: user.disabled,
                is_admin: user.is_admin,
            })
            .collect::<Vec<_>>(),
    ))
}

/// Returns the caller's administrator membership. This endpoint never
/// accepts a user id from the client and reveals no admin records to a
/// non-admin caller.
pub async fn me(
    State(state): State<AppState>,
    CurrentUser(caller): CurrentUser,
) -> Result<impl IntoResponse, ApiError> {
    let is_admin = auth::is_active_administrator(state.database(), caller.user_public_id).await?;
    let totp_verified = auth::session_totp_verified(state.database(), caller).await?;
    if !is_admin {
        return Err(ApiError::Forbidden(
            "Administrator access required".to_owned(),
        ));
    }
    Ok(Json(AdminMeResponse {
        user_id: caller.user_public_id.get(),
        is_admin,
        totp_verified,
    }))
}

#[derive(Debug, Serialize)]
pub struct DashboardResponse {
    pub users: i64,
    pub active_sessions: i64,
    pub contracts: i64,
    pub inventory_items: i64,
    pub market_purchases: i64,
    pub ledger_transactions: i64,
}

pub async fn dashboard(
    State(state): State<AppState>,
    CurrentUser(caller): CurrentUser,
) -> Result<impl IntoResponse, ApiError> {
    if !auth::is_active_administrator(state.database(), caller.user_public_id).await? {
        return Err(ApiError::Forbidden(
            "Administrator access required".to_owned(),
        ));
    }
    let stats = db::admin_dashboard_stats(state.database().pool())
        .await
        .map_err(application::auth::AuthError::from)?;
    Ok(Json(DashboardResponse {
        users: stats.users,
        active_sessions: stats.active_sessions,
        contracts: stats.contracts,
        inventory_items: stats.inventory_items,
        market_purchases: stats.market_purchases,
        ledger_transactions: stats.ledger_transactions,
    }))
}
