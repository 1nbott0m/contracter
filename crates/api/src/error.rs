use std::time::Duration;

use axum::{
    Json,
    http::{HeaderValue, StatusCode, header::RETRY_AFTER},
    response::IntoResponse,
};
use serde::Serialize;
use uuid::Uuid;

/// The stable API error envelope. `request_id` is `None` at construction
/// time -- the outermost request-id middleware (`crate::request_id`)
/// fills it in on the way out, so every error response carries the same
/// correlation id as its `x-request-id` response header regardless of
/// which layer produced the error.
#[derive(Debug, Serialize)]
pub struct ErrorEnvelope {
    pub error: ErrorBody,
}

#[derive(Debug, Serialize)]
pub struct ErrorBody {
    pub code: &'static str,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<Uuid>,
}

/// The full set of stable error codes BACKEND-01 supports. Deliberately
/// not one variant per business scenario -- callers pick the HTTP-semantic
/// bucket that fits, and a human-readable `message` carries the specific,
/// already-sanitized detail.
#[derive(Debug)]
pub enum ApiError {
    BadRequest(String),
    Unauthorized(String),
    Forbidden(String),
    NotFound(String),
    Conflict(String),
    UnprocessableEntity(String),
    TooManyRequests {
        message: String,
        retry_after: Duration,
    },
    Internal,
    ServiceUnavailable(String),
}

impl ApiError {
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::NotFound(message.into())
    }

    pub fn service_unavailable(message: impl Into<String>) -> Self {
        Self::ServiceUnavailable(message.into())
    }

    /// The caller is over budget.
    ///
    /// `retry_after` is carried in the variant rather than only formatted
    /// into the message, because it has to reach the response as a
    /// `Retry-After` header. A 429 whose delay is only readable as English
    /// prose is a 429 no client, proxy or SDK can honour, so it invites
    /// the immediate retry it was meant to prevent.
    pub fn too_many_requests(retry_after: Duration) -> Self {
        Self::TooManyRequests {
            message: format!(
                "Too many requests; retry in {} seconds",
                retry_after.as_secs().max(1)
            ),
            retry_after,
        }
    }

    /// `(status, stable code, safe message)`. `Internal`'s real cause is
    /// never in this tuple -- callers that have one must log it via
    /// `tracing` themselves before returning `ApiError::Internal`.
    fn parts(&self) -> (StatusCode, &'static str, String) {
        match self {
            Self::BadRequest(message) => (StatusCode::BAD_REQUEST, "BAD_REQUEST", message.clone()),
            Self::Unauthorized(message) => {
                (StatusCode::UNAUTHORIZED, "UNAUTHORIZED", message.clone())
            }
            Self::Forbidden(message) => (StatusCode::FORBIDDEN, "FORBIDDEN", message.clone()),
            Self::NotFound(message) => (StatusCode::NOT_FOUND, "NOT_FOUND", message.clone()),
            Self::Conflict(message) => (StatusCode::CONFLICT, "CONFLICT", message.clone()),
            Self::UnprocessableEntity(message) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "UNPROCESSABLE_ENTITY",
                message.clone(),
            ),
            Self::TooManyRequests { message, .. } => (
                StatusCode::TOO_MANY_REQUESTS,
                "TOO_MANY_REQUESTS",
                message.clone(),
            ),
            Self::Internal => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "INTERNAL_SERVER_ERROR",
                "Internal server error".to_owned(),
            ),
            Self::ServiceUnavailable(message) => (
                StatusCode::SERVICE_UNAVAILABLE,
                "SERVICE_UNAVAILABLE",
                message.clone(),
            ),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let (status, code, message) = self.parts();
        // Whole seconds, and never zero: `Retry-After` has no sub-second
        // form, and rounding down to zero would tell a client to retry
        // immediately, which is the behaviour being refused.
        let retry_after = match &self {
            Self::TooManyRequests { retry_after, .. } => {
                Some(retry_after.as_secs().max(1).to_string())
            }
            _ => None,
        };

        let mut response = (
            status,
            Json(ErrorEnvelope {
                error: ErrorBody {
                    code,
                    message,
                    request_id: None,
                },
            }),
        )
            .into_response();

        if let Some(seconds) = retry_after
            && let Ok(value) = HeaderValue::from_str(&seconds)
        {
            response.headers_mut().insert(RETRY_AFTER, value);
        }
        response
    }
}

/// Logs the real cause and returns the generic 500. The single place a
/// non-client error becomes an HTTP response, so the cause is always
/// recorded and never transmitted.
pub(crate) fn internal(error: &dyn std::fmt::Display, context: &'static str) -> ApiError {
    tracing::error!(error = %error, "{context}");
    ApiError::Internal
}

impl From<application::auth::AuthError> for ApiError {
    fn from(error: application::auth::AuthError) -> Self {
        use application::auth::AuthError;

        match error {
            // Input the caller can fix, described precisely: these carry
            // no information about other accounts or stored state.
            AuthError::InvalidLogin | AuthError::WeakPassword => {
                Self::UnprocessableEntity(error.to_string())
            }
            AuthError::LoginTaken => Self::Conflict(error.to_string()),
            // One outcome for every unusable invitation, so a response
            // never reveals whether an invitation exists or was spent.
            AuthError::InvitationUnusable => Self::Forbidden(error.to_string()),
            // One outcome for a wrong password, an unknown login, and a
            // dead session alike.
            AuthError::InvalidCredentials => Self::Unauthorized(error.to_string()),
            AuthError::SessionInvalid => Self::Unauthorized("Authentication required".to_owned()),
            // Not the caller's fault and not a bug: every password-hashing
            // permit is taken. 503 says "try again", which is true, where a
            // 500 would say "something is broken", which is not.
            AuthError::Overloaded => {
                tracing::warn!("password hashing is saturated; shedding a request");
                Self::ServiceUnavailable(error.to_string())
            }
            AuthError::PasswordHashing => internal(&error, "password hashing failed"),
            AuthError::Database(ref cause) => internal(cause, "an auth database call failed"),
        }
    }
}

impl From<application::catalog::CatalogError> for ApiError {
    fn from(error: application::catalog::CatalogError) -> Self {
        use application::catalog::CatalogError;

        match error {
            // A bad cursor or filter is the caller's to fix, and saying so
            // costs nothing: neither reveals anything about stored data.
            CatalogError::InvalidCursor | CatalogError::InvalidFilter => {
                Self::BadRequest(error.to_string())
            }
            CatalogError::Database(ref cause) => internal(cause, "a catalog query failed"),
        }
    }
}

impl From<application::inventory::InventoryError> for ApiError {
    fn from(error: application::inventory::InventoryError) -> Self {
        use application::inventory::InventoryError;

        match error {
            InventoryError::InvalidCursor | InventoryError::InvalidFilter => {
                Self::BadRequest(error.to_string())
            }
            // An item owned by someone else and one that never existed are
            // the same answer on purpose: distinguishing them would make
            // this endpoint an oracle for which UUIDs are real.
            InventoryError::NotFound => Self::NotFound("The item does not exist".to_owned()),
            InventoryError::Database(ref cause) => internal(cause, "an inventory query failed"),
        }
    }
}
