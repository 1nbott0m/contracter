use application::auth::{self, SecretToken};
use axum::{
    Json,
    extract::Query,
    extract::State,
    http::{HeaderMap, StatusCode, header::SET_COOKIE},
    response::{IntoResponse, Redirect},
};
use db;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

use crate::{error::ApiError, extract::CurrentUser, state::AppState};

#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    /// Optional legacy invitation token. When absent, public signup is used.
    #[serde(default)]
    pub invitation_token: Option<String>,
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
    let user_id = match request.invitation_token {
        Some(token) if !token.trim().is_empty() => {
            let invitation = SecretToken::new(token);
            auth::register(
                state.database(),
                &invitation,
                &request.login,
                &request.password,
            )
            .await?
        }
        _ => auth::register_public(state.database(), &request.login, &request.password).await?,
    };

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

    let mut headers = HeaderMap::new();
    headers.insert(
        SET_COOKIE,
        state
            .session_cookie_policy()
            .set(session.token.reveal(), session.ttl),
    );

    Ok((
        headers,
        Json(LoginResponse {
            user_id: session.user_public_id.get(),
        }),
    ))
}

/// `POST /api/v1/auth/logout` -- end this session and clear the cookie.
/// Idempotent.
pub async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    if let Some(token) = state.session_cookie_policy().read_token(&headers) {
        auth::logout(state.database(), &SecretToken::new(token)).await?;
    }

    let mut response_headers = HeaderMap::new();
    response_headers.insert(SET_COOKIE, state.session_cookie_policy().clear());
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
    headers.insert(SET_COOKIE, state.session_cookie_policy().clear());
    Ok((headers, Json(LogoutAllResponse { revoked_sessions })))
}

pub async fn steam_start() -> Redirect {
    Redirect::temporary(
        "https://steamcommunity.com/openid/login?openid.ns=http%3A%2F%2Fspecs.openid.net%2Fauth%2F2.0&openid.mode=checkid_setup&openid.return_to=https%3A%2F%2Fcontracter.onrender.com%2Fapi%2Fv1%2Fauth%2Fsteam%2Fcallback&openid.realm=https%3A%2F%2Fcontracter.onrender.com&openid.identity=http%3A%2F%2Fspecs.openid.net%2Fauth%2F2.0%2Fidentifier_select&openid.claimed_id=http%3A%2F%2Fspecs.openid.net%2Fauth%2F2.0%2Fidentifier_select",
    )
}

pub async fn steam_callback(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<impl IntoResponse, ApiError> {
    let Some(claimed_id) = params.get("openid.claimed_id") else {
        return Err(ApiError::Unauthorized("Steam verification failed".into()));
    };
    let Some(steam_id) = claimed_id
        .rsplit('/')
        .next()
        .filter(|id| id.chars().all(|c| c.is_ascii_digit()) && id.len() >= 10)
    else {
        return Err(ApiError::Unauthorized("Steam verification failed".into()));
    };
    let mut form = params.clone();
    form.insert("openid.mode".into(), "check_authentication".into());
    let response = reqwest::Client::new()
        .post("https://steamcommunity.com/openid/login")
        .form(&form)
        .send()
        .await
        .map_err(|_| ApiError::Unauthorized("Steam verification failed".into()))?;
    let body = response
        .text()
        .await
        .map_err(|_| ApiError::Unauthorized("Steam verification failed".into()))?;
    if !body.lines().any(|line| line.trim() == "is_valid:true") {
        return Err(ApiError::Unauthorized("Steam verification failed".into()));
    }
    let user = db::find_user_by_steam_id(state.database().pool(), steam_id)
        .await
        .map_err(application::auth::AuthError::from)?;
    let (user_id, user_public_id) = match user {
        Some(pair) => pair,
        None => {
            let public_id = auth::register_steam(state.database(), steam_id).await?;
            let (user_id, _) = db::find_user_by_steam_id(state.database().pool(), steam_id)
                .await
                .map_err(application::auth::AuthError::from)?
                .ok_or_else(|| {
                    ApiError::Unauthorized("Steam account could not be created".into())
                })?;
            (user_id, public_id)
        }
    };
    let session = auth::issue_session_for_user(
        state.database(),
        state.auth_config(),
        user_id,
        user_public_id,
    )
    .await?;
    let mut headers = HeaderMap::new();
    headers.insert(
        SET_COOKIE,
        state
            .session_cookie_policy()
            .set(session.token.reveal(), session.ttl),
    );
    Ok((
        headers,
        Redirect::temporary("https://contracter-1t9.pages.dev/contracts"),
    ))
}
