use axum::{Json, http::StatusCode, response::IntoResponse};
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
    TooManyRequests(String),
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
            Self::TooManyRequests(message) => (
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
        (
            status,
            Json(ErrorEnvelope {
                error: ErrorBody {
                    code,
                    message,
                    request_id: None,
                },
            }),
        )
            .into_response()
    }
}
