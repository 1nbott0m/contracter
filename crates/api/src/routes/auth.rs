use application::auth::{self, SecretToken};
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode, header::SET_COOKIE},
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{error::ApiError, extract::CurrentUser, session_cookie, state::AppState};

#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    /// The raw invitation token. Registration is invitation-only by
    /// design; there is no open signup.
    pub invitation_token: String,
    pub login: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct RegisterResponse {
    /// The account's public UUID. Internal sequential ids never leave the
    /// database.
    pub user_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub login: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub user_id: Uuid,
}

#[derive(Debug, Serialize)]
pub struct LogoutAllResponse {
    pub revoked_sessions: i32,
}

/// `POST /api/v1/auth/register` -- redeem an invitation and create an
/// account. Does not log the new user in: they authenticate through the
/// normal login path, so a registration response never carries a session.
pub async fn register(
    State(state): State<AppState>,
    Json(request): Json<RegisterRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let invitation = SecretToken::new(request.invitation_token);
    let user_id = auth::register(
        state.database(),
        &invitation,
        &request.login,
        &request.password,
    )
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(RegisterResponse {
            user_id: user_id.get(),
        }),
    ))
}

/// `POST /api/v1/auth/login` -- exchange credentials for a session
/// cookie. The raw token is transmitted exactly once, in a `Set-Cookie`
/// the client cannot read from JavaScript, and never appears in the body.
pub async fn login(
    State(state): State<AppState>,
    Json(request): Json<LoginRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let session = auth::login(
        state.database(),
        state.auth_config(),
        &request.login,
        &request.password,
    )
    .await?;
    let caller = auth::authenticate(state.database(), &session.token).await?;

    let mut headers = HeaderMap::new();
    headers.insert(
        SET_COOKIE,
        session_cookie::session_cookie(session.token.reveal(), session.ttl, state.secure_cookies()),
    );

    Ok((
        headers,
        Json(LoginResponse {
            user_id: caller.user_public_id.get(),
        }),
    ))
}

/// `POST /api/v1/auth/logout` -- end this session and clear the cookie.
/// Idempotent.
pub async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    if let Some(token) = session_cookie::read_session_token(&headers) {
        auth::logout(state.database(), &SecretToken::new(token)).await?;
    }

    let mut response_headers = HeaderMap::new();
    response_headers.insert(
        SET_COOKIE,
        session_cookie::cleared_session_cookie(state.secure_cookies()),
    );
    Ok((StatusCode::NO_CONTENT, response_headers))
}

/// `POST /api/v1/auth/logout-all` -- end every session this account has,
/// including the current one, and clear this client's cookie. Requires a
/// live session: the account is identified by that session, never by the
/// request body.
pub async fn logout_all(
    State(state): State<AppState>,
    CurrentUser(caller): CurrentUser,
) -> Result<impl IntoResponse, ApiError> {
    let revoked_sessions = auth::logout_all(state.database(), caller.user_id).await?;

    let mut headers = HeaderMap::new();
    headers.insert(
        SET_COOKIE,
        session_cookie::cleared_session_cookie(state.secure_cookies()),
    );
    Ok((headers, Json(LogoutAllResponse { revoked_sessions })))
}
