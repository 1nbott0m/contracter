use std::{
    convert::Infallible,
    net::{IpAddr, SocketAddr},
};

use application::auth::{self, AuthError, SecretToken};
use axum::{
    extract::{ConnectInfo, FromRequestParts},
    http::request::Parts,
};

use crate::{error::ApiError, state::AppState};

/// The authenticated caller behind a request.
///
/// Identity comes from the session cookie and nothing else: there is no
/// constructor that accepts a client-supplied user id, so no handler can
/// accidentally trust one. Any handler that takes this extractor is
/// authenticated by construction; any handler that does not, is public.
#[derive(Debug, Clone, Copy)]
pub struct CurrentUser(pub auth::AuthenticatedUser);

impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let token = state
            .session_cookie_policy()
            .read_token(&parts.headers)
            .map(SecretToken::new)
            .ok_or_else(unauthorized)?;

        match auth::authenticate(state.database(), &token).await {
            Ok(user) => Ok(Self(user)),
            Err(AuthError::SessionInvalid) => Err(unauthorized()),
            Err(error) => Err(crate::error::internal(&error, "resolving a session failed")),
        }
    }
}

/// A missing cookie and a rejected one are reported identically: the
/// response never says whether a token existed, was expired, was revoked,
/// or belonged to a disabled account.
fn unauthorized() -> ApiError {
    ApiError::Unauthorized("Authentication required".to_owned())
}

/// The transport peer's address, when the server was configured to record
/// one.
///
/// Read from the connection rather than from a header. `X-Forwarded-For`
/// is deliberately ignored: anyone may set it, so honouring it would hand
/// a rate-limited caller a fresh budget per request and make the limiter
/// worse than none. Behind a reverse proxy every caller therefore shares
/// the proxy's address, which is why such a deployment needs its own
/// limiter at the edge.
///
/// Never rejects. A missing address -- which is how tests drive the router
/// directly, and how a misconfigured server would behave -- yields `None`,
/// and the caller decides what that means. Failing the request instead
/// would turn a configuration detail into an outage.
#[derive(Debug, Clone, Copy)]
pub struct PeerAddress(pub Option<IpAddr>);

impl<S> FromRequestParts<S> for PeerAddress
where
    S: Send + Sync,
{
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        Ok(Self(
            parts
                .extensions
                .get::<ConnectInfo<SocketAddr>>()
                .map(|ConnectInfo(address)| address.ip()),
        ))
    }
}
