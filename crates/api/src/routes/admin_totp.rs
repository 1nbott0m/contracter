use crate::{error::ApiError, extract::CurrentUser, state::AppState};
use application::auth;
use axum::{Json, extract::State};
use serde::{Deserialize, Serialize};
#[derive(Debug, Serialize)]
pub struct ProvisionResponse {
    pub secret: String,
    pub otpauth_uri: String,
}
#[derive(Debug, Deserialize)]
pub struct VerifyRequest {
    pub code: String,
}
pub async fn provision(
    State(state): State<AppState>,
    CurrentUser(caller): CurrentUser,
) -> Result<Json<ProvisionResponse>, ApiError> {
    if !auth::is_active_administrator(state.database(), caller.user_public_id).await? {
        return Err(ApiError::Forbidden("Administrator access required".into()));
    }
    let protector = state
        .seed_protector()
        .ok_or_else(|| ApiError::ServiceUnavailable("TOTP encryption is not configured".into()))?;
    let secret =
        auth::provision_totp(state.database(), protector.as_ref(), caller.user_public_id).await?;
    Ok(Json(ProvisionResponse {
        otpauth_uri: format!(
            "otpauth://totp/CONTRACTER:{}?secret={secret}&issuer=CONTRACTER",
            caller.user_public_id.get()
        ),
        secret,
    }))
}
pub async fn verify(
    State(state): State<AppState>,
    CurrentUser(caller): CurrentUser,
    Json(request): Json<VerifyRequest>,
) -> Result<axum::http::StatusCode, ApiError> {
    let protector = state
        .seed_protector()
        .ok_or_else(|| ApiError::ServiceUnavailable("TOTP encryption is not configured".into()))?;
    auth::verify_totp(state.database(), protector.as_ref(), caller, &request.code).await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}
