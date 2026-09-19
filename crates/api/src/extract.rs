use application::auth::{self, AuthError, SecretToken};
use axum::{extract::FromRequestParts, http::request::Parts};

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
