use application::auth;
use application::auth::AuthenticatedUser;
use axum::{Json, extract::State, response::IntoResponse};
use db;
use serde::Serialize;
use serde_json::json;
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

async fn require_verified_admin(
    state: &AppState,
    caller: AuthenticatedUser,
) -> Result<(), ApiError> {
    if !auth::is_active_administrator(state.database(), caller.user_public_id).await? {
        return Err(ApiError::Forbidden(
            "Administrator access required".to_owned(),
        ));
    }
    if !auth::session_totp_verified(state.database(), caller).await? {
        return Err(ApiError::Forbidden(
            "Administrator TOTP verification required".to_owned(),
        ));
    }
    Ok(())
}

pub async fn users(
    State(state): State<AppState>,
    CurrentUser(caller): CurrentUser,
) -> Result<impl IntoResponse, ApiError> {
    require_verified_admin(&state, caller).await?;
    db::record_admin_audit(
        state.database().pool(),
        caller.user_public_id,
        "admin.users.read",
        None,
        json!({}),
    )
    .await
    .map_err(application::auth::AuthError::from)?;
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

#[derive(Debug, Serialize)]
pub struct AdminAuditResponse {
    pub public_id: Uuid,
    pub administrator_public_id: Uuid,
    pub action_code: String,
    pub target_public_id: Option<Uuid>,
    pub metadata: serde_json::Value,
    pub created_at: String,
}
pub async fn audit(
    State(state): State<AppState>,
    CurrentUser(caller): CurrentUser,
) -> Result<impl IntoResponse, ApiError> {
    require_verified_admin(&state, caller).await?;
    let events = db::list_admin_audit(state.database().pool())
        .await
        .map_err(application::auth::AuthError::from)?;
    Ok(Json(
        events
            .into_iter()
            .map(|event| AdminAuditResponse {
                public_id: event.public_id.get(),
                administrator_public_id: event.administrator_public_id.get(),
                action_code: event.action_code,
                target_public_id: event.target_public_id.map(|id| id.get()),
                metadata: event.metadata,
                created_at: event.created_at.to_rfc3339(),
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
    require_verified_admin(&state, caller).await?;
    let stats = db::admin_dashboard_stats(state.database().pool())
        .await
        .map_err(application::auth::AuthError::from)?;
    db::record_admin_audit(
        state.database().pool(),
        caller.user_public_id,
        "admin.dashboard.read",
        None,
        json!({}),
    )
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
