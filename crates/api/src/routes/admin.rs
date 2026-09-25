use application::auth;
use axum::{Json, extract::State, response::IntoResponse};
use serde::Serialize;
use uuid::Uuid;

use crate::{error::ApiError, extract::CurrentUser, state::AppState};

#[derive(Debug, Serialize)]
pub struct AdminMeResponse {
    pub user_id: Uuid,
    pub is_admin: bool,
}

/// Returns the caller's administrator membership. This endpoint never
/// accepts a user id from the client and reveals no admin records to a
/// non-admin caller.
pub async fn me(
    State(state): State<AppState>,
    CurrentUser(caller): CurrentUser,
) -> Result<impl IntoResponse, ApiError> {
    let is_admin = auth::is_active_administrator(state.database(), caller.user_public_id).await?;
    if !is_admin {
        return Err(ApiError::Forbidden(
            "Administrator access required".to_owned(),
        ));
    }
    Ok(Json(AdminMeResponse {
        user_id: caller.user_public_id.get(),
        is_admin,
    }))
}
