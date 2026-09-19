use application::auth::{self, SecretToken};
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode, header::SET_COOKIE},
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    error::ApiError,
    extract::{CurrentUser, PeerAddress},
    rate_limit::RateLimitKey,
    state::AppState,
};

/// The bucket this request spends from.
///
/// `ConnectInfo` is the transport peer, never a client-supplied header.
/// `X-Forwarded-For` is deliberately not consulted: anyone may set it, so
/// honouring it would hand an attacker a fresh budget per request and make
/// the limiter worse than none at all. Behind a reverse proxy every caller
/// shares the proxy's bucket, which is why a proxied deployment needs a
/// limiter at the edge as well -- stated here rather than assumed away.
///
/// With no peer address -- which is how tests drive the router -- one
/// shared bucket still bounds total work; it simply cannot tell callers
/// apart.
fn peer_key(PeerAddress(address): PeerAddress) -> RateLimitKey {
    address.map_or(RateLimitKey::Anonymous, RateLimitKey::Peer)
}

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
    peer: PeerAddress,
    Json(request): Json<RegisterRequest>,
) -> Result<impl IntoResponse, ApiError> {
    // Before any work at all, including the invitation lookup.
    let limiter = state.auth_rate_limiter();
    if let Err(retry) = limiter.check(&peer_key(peer)) {
        return Err(ApiError::too_many_requests(retry.0));
    }

    let invitation = SecretToken::new(request.invitation_token);
    // And a budget for the invitation itself. A failed registration does
    // not consume the invitation, so without this a holder of one valid
    // invitation could probe logins indefinitely by rotating addresses.
    if let Err(retry) = limiter.check(&RateLimitKey::Invitation(invitation.hash())) {
        return Err(ApiError::too_many_requests(retry.0));
    }
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
    peer: PeerAddress,
    Json(request): Json<LoginRequest>,
) -> Result<impl IntoResponse, ApiError> {
    // Two budgets, because either alone leaves a hole: by address only,
    // one host spreads guesses across every account; by account only, a
    // botnet hammers one account freely. The address is charged first so a
    // flood is refused before it can touch any account's budget.
    let limiter = state.auth_rate_limiter();
    if let Err(retry) = limiter.check(&peer_key(peer)) {
        return Err(ApiError::too_many_requests(retry.0));
    }
    if let Err(retry) = limiter.check(&RateLimitKey::login(&request.login)) {
        return Err(ApiError::too_many_requests(retry.0));
    }

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
