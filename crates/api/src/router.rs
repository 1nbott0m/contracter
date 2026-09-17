use std::time::Duration;

use axum::{BoxError, Router, error_handling::HandleErrorLayer, routing::get};
use tower::ServiceBuilder;
use tower_http::{
    catch_panic::CatchPanicLayer, cors::CorsLayer, limit::RequestBodyLimitLayer, trace::TraceLayer,
};

use crate::{error::ApiError, request_id::request_id_middleware, routes::health, state::AppState};

/// Router construction knobs that depend on runtime configuration
/// (`crates/server`), never hardcoded production values.
#[derive(Debug, Clone)]
pub struct RouterConfig {
    pub request_timeout: Duration,
    pub max_body_bytes: usize,
    pub cors_allowed_origins: Vec<String>,
}

impl Default for RouterConfig {
    fn default() -> Self {
        Self {
            request_timeout: Duration::from_secs(10),
            max_body_bytes: 256 * 1024,
            cors_allowed_origins: Vec::new(),
        }
    }
}

/// Builds the full application `Router`, independent of any TCP listener
/// so it can be exercised in tests via `tower::ServiceExt::oneshot`.
/// `/health/*` is intentionally outside `/api/v1`, which is reserved for
/// future business endpoints (BACKEND-02+).
pub fn build_router(state: AppState, config: &RouterConfig) -> Router {
    let health_routes = Router::new()
        .route("/health/live", get(health::live))
        .route("/health/ready", get(health::ready));

    let mut router = Router::new()
        .merge(health_routes)
        .fallback(fallback_404)
        .with_state(state)
        .layer(CatchPanicLayer::custom(handle_panic))
        .layer(RequestBodyLimitLayer::new(config.max_body_bytes))
        .layer(
            ServiceBuilder::new()
                .layer(HandleErrorLayer::new(handle_timeout_error))
                .timeout(config.request_timeout),
        );

    // CORS wraps outside timeout/body-limit/panic handling so error
    // responses (503 on timeout, 500 on panic, etc.) still carry the
    // headers a browser needs to read them via fetch, not just 2xx ones.
    if let Some(cors) = build_cors_layer(&config.cors_allowed_origins) {
        router = router.layer(cors);
    }

    router
        .layer(TraceLayer::new_for_http())
        .layer(axum::middleware::from_fn(request_id_middleware))
}

async fn fallback_404() -> ApiError {
    ApiError::not_found("The requested resource does not exist")
}

async fn handle_timeout_error(error: BoxError) -> ApiError {
    tracing::warn!(error = %error, "request exceeded the configured timeout");
    ApiError::service_unavailable("Service temporarily unavailable")
}

fn handle_panic(payload: Box<dyn std::any::Any + Send + 'static>) -> axum::response::Response {
    let message = payload
        .downcast_ref::<&str>()
        .map(|value| (*value).to_owned())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unknown panic payload".to_owned());
    tracing::error!(panic = %message, "request handler panicked");
    axum::response::IntoResponse::into_response(ApiError::Internal)
}

fn build_cors_layer(allowed_origins: &[String]) -> Option<CorsLayer> {
    if allowed_origins.is_empty() {
        return None;
    }
    let origins = allowed_origins
        .iter()
        .filter_map(|origin| origin.parse().ok())
        .collect::<Vec<_>>();
    if origins.is_empty() {
        return None;
    }
    Some(
        CorsLayer::new()
            .allow_origin(origins)
            .allow_methods([axum::http::Method::GET])
            .allow_headers([axum::http::header::CONTENT_TYPE]),
    )
}

#[cfg(test)]
mod tests {
    use std::convert::Infallible;

    use axum::{body::Body, http::Request, http::StatusCode, routing::get};
    use tower::ServiceExt;

    use super::{CatchPanicLayer, RequestBodyLimitLayer, ServiceBuilder, handle_panic};

    /// Exercises the exact `handle_panic` callback wired into the real
    /// router, not a stand-in -- a panicking handler must become a
    /// sanitized 500 in the standard envelope, never crash the worker or
    /// leak the panic payload.
    #[tokio::test]
    async fn catch_panic_layer_converts_a_handler_panic_into_a_safe_internal_error() {
        async fn panics() -> axum::response::Response {
            panic!("sensitive panic detail that must never reach a client");
        }

        let router = axum::Router::new()
            .route("/panics", get(panics))
            .layer(CatchPanicLayer::custom(handle_panic));
        let request = Request::builder()
            .uri("/panics")
            .body(Body::empty())
            .unwrap();

        let response = router.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["error"]["code"], "INTERNAL_SERVER_ERROR");
        let raw = String::from_utf8_lossy(&bytes);
        assert!(!raw.contains("sensitive panic detail"));
    }

    /// `RequestBodyLimit` (what `RequestBodyLimitLayer` applies) rejects a
    /// request whose declared `Content-Length` exceeds the configured
    /// limit immediately, before any inner service or route runs --
    /// proven here directly against the layer, matching the guarantee
    /// `build_router` relies on: this fires even though BACKEND-01 has no
    /// body-consuming route to observe it through end-to-end.
    #[tokio::test]
    async fn request_body_limit_rejects_an_oversized_declared_content_length() {
        async fn unreachable_inner(
            _request: Request<tower_http::body::Limited<Body>>,
        ) -> Result<axum::response::Response, Infallible> {
            panic!("the body limit must reject this request before the inner service runs")
        }

        let service = ServiceBuilder::new()
            .layer(RequestBodyLimitLayer::new(8))
            .service(tower::service_fn(unreachable_inner));

        let request = Request::builder()
            .uri("/")
            .header(axum::http::header::CONTENT_LENGTH, "1024")
            .body(Body::empty())
            .unwrap();

        let response = service.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    }
}
