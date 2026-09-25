use std::time::Duration;

use axum::{
    BoxError, Router,
    error_handling::HandleErrorLayer,
    extract::Request,
    http::{
        HeaderValue,
        header::{CACHE_CONTROL, VARY},
    },
    middleware::Next,
    response::Response,
    routing::{get, post},
};
use tower::ServiceBuilder;
use tower_governor::{GovernorLayer, governor::GovernorConfigBuilder};
use tower_http::{
    catch_panic::CatchPanicLayer, cors::CorsLayer, limit::RequestBodyLimitLayer, trace::TraceLayer,
};

use crate::{
    client_ip::ClientIpKeyExtractor,
    error::ApiError,
    request_id::request_id_middleware,
    routes::{
        account, admin, admin_totp, auth, catalog, health, history, inventory, market, quote,
    },
    state::AppState,
};

/// Router construction knobs that depend on runtime configuration
/// (`crates/server`), never hardcoded production values.
#[derive(Debug, Clone)]
pub struct RouterConfig {
    pub request_timeout: Duration,
    pub max_body_bytes: usize,
    pub cors_allowed_origins: Vec<String>,
    /// Requests per second limiter burst. `None` is used by in-process tests;
    /// production wiring must set a finite value.
    pub rate_limit_burst: Option<u32>,
    pub trusted_proxies: crate::TrustedProxyConfig,
}

impl Default for RouterConfig {
    fn default() -> Self {
        Self {
            request_timeout: Duration::from_secs(10),
            max_body_bytes: 256 * 1024,
            cors_allowed_origins: Vec::new(),
            rate_limit_burst: None,
            trusted_proxies: crate::TrustedProxyConfig::default(),
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

    let market_routes = Router::new()
        .route("/market/valuations", get(market::valuations))
        .route("/market/price-halts", get(market::price_halts))
        // Market data can change immediately when a price halt begins.
        // Do not let a browser or intermediary keep serving an older price.
        .layer(axum::middleware::from_fn(market_response_headers));

    // Everything a client calls lives under /api/v1; /health stays
    // outside it so orchestrators never depend on an API version.
    let api_v1 = Router::new()
        .route("/auth/register", post(auth::register))
        .route("/auth/login", post(auth::login))
        .route("/auth/steam/start", get(auth::steam_start))
        .route("/auth/steam/callback", get(auth::steam_callback))
        .route("/auth/logout", post(auth::logout))
        .route("/auth/logout-all", post(auth::logout_all))
        .route("/auth/totp/verify", post(admin_totp::verify))
        .route("/me", get(account::me))
        .route("/admin/me", get(admin::me))
        .route("/admin/dashboard", get(admin::dashboard))
        .route("/admin/users", get(admin::users))
        .route("/admin/audit", get(admin::audit))
        .route("/admin/totp/provision", post(admin_totp::provision))
        .route("/me/balance", get(account::balance))
        .route("/me/inventory", get(inventory::list))
        .route("/me/inventory/{item_id}", get(inventory::detail))
        .route("/me/quote-allocations", post(quote::allocate))
        .route("/me/quotes", post(quote::create))
        .route("/me/market/purchases/{sku_id}", post(market::purchase))
        .route("/me/market/buybacks/{item_id}", post(market::buyback))
        .route("/me/history/contracts", get(history::contracts))
        .route("/me/history/ledger", get(history::ledger))
        .route(
            "/me/history/inventory-events",
            get(history::inventory_events),
        )
        .route("/me/quote", get(quote::active))
        .route("/me/quote/{quote_id}/accept", post(quote::accept))
        .layer(axum::middleware::from_fn(private_response_headers))
        // The catalog is the same for everyone and carries no session, so
        // it is deliberately outside that layer: marking it `no-store`
        // would forbid every CDN and proxy from caching the one part of
        // this API that is safe to cache.
        .route("/catalog/collections", get(catalog::collections))
        .route("/catalog/skus", get(catalog::skus))
        .merge(market_routes);

    let mut router = Router::new()
        .merge(health_routes)
        .nest("/api/v1", api_v1)
        .fallback(fallback_404)
        .with_state(state)
        .layer(CatchPanicLayer::custom(handle_panic))
        .layer(RequestBodyLimitLayer::new(config.max_body_bytes))
        .layer(
            ServiceBuilder::new()
                .layer(HandleErrorLayer::new(handle_timeout_error))
                .timeout(config.request_timeout),
        );

    if let Some(burst) = config.rate_limit_burst {
        let mut governor_builder = GovernorConfigBuilder::default()
            .key_extractor(ClientIpKeyExtractor::new(config.trusted_proxies.clone()));
        let governor = governor_builder
            .per_second(1)
            .burst_size(burst.max(1))
            .finish()
            .expect("rate limiter configuration must be valid");
        router = router.layer(GovernorLayer::new(governor));
    }

    // CORS wraps outside timeout/body-limit/panic handling so error
    // responses (503 on timeout, 500 on panic, etc.) still carry the
    // headers a browser needs to read them via fetch, not just 2xx ones.
    if let Some(cors) = build_cors_layer(&config.cors_allowed_origins) {
        router = router.layer(cors);
    }

    router
        .layer(TraceLayer::new_for_http())
        .layer(axum::middleware::from_fn(security_response_headers))
        .layer(axum::middleware::from_fn(request_id_middleware))
}

/// Deployment terminates TLS at the trusted edge; every response advertises
/// the HTTPS-only browser policy so public, private, health, and error routes
/// cannot drift apart.
async fn security_response_headers(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    response.headers_mut().insert(
        axum::http::header::STRICT_TRANSPORT_SECURITY,
        HeaderValue::from_static("max-age=31536000; includeSubDomains"),
    );
    response
}

/// Marks every `/api/v1` response as private and cookie-dependent.
///
/// Without `Vary: Cookie`, any shared cache in front of this service --
/// a CDN, a reverse proxy, a corporate middlebox -- is entitled to serve
/// one account's `/me` to the next caller, because the requests differ
/// only in a header it was never told mattered. `no-store` is the
/// stronger half: these responses should not be written down at all.
/// Applied as a layer rather than per handler so a route added later
/// cannot forget it.
async fn private_response_headers(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(VARY, HeaderValue::from_static("Cookie"));
    response
}

/// Market values are public but volatile: a fresh price halt must take effect
/// for every client immediately, so these responses are never cacheable.
async fn market_response_headers(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
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
        .filter_map(|origin| match origin.parse() {
            Ok(parsed) => Some(parsed),
            Err(_) => {
                tracing::warn!(origin = %origin, "ignoring malformed CORS_ALLOWED_ORIGINS entry");
                None
            }
        })
        .collect::<Vec<_>>();
    if origins.is_empty() {
        return None;
    }
    Some(
        CorsLayer::new()
            .allow_origin(origins)
            .allow_methods([axum::http::Method::GET, axum::http::Method::POST])
            .allow_headers([axum::http::header::CONTENT_TYPE])
            // The session lives in a cookie, so a browser frontend on a
            // different origin cannot call this API at all without it.
            // Safe only because the origin list is explicit: the CORS spec
            // forbids pairing credentials with a wildcard, and
            // `build_cors_layer` returns `None` rather than a wildcard
            // when nothing is configured.
            .allow_credentials(true),
    )
}

#[cfg(test)]
mod tests {
    use std::{convert::Infallible, net::SocketAddr, time::Duration};

    use axum::{body::Body, extract::ConnectInfo, http::Request, http::StatusCode, routing::get};
    use tower::ServiceExt;

    use super::{
        CatchPanicLayer, RequestBodyLimitLayer, RouterConfig, ServiceBuilder, build_router,
        handle_panic,
    };
    use crate::request_id::{REQUEST_ID_HEADER, request_id_middleware};
    use crate::{AppState, TrustedProxyConfig};
    use application::auth::AuthConfig;
    use db::{Database, DatabaseConfig};

    /// Exercises the exact `handle_panic` callback wired into the real
    /// router, not a stand-in -- a panicking handler must become a
    /// sanitized 500 in the standard envelope, never crash the worker or
    /// leak the panic payload. Also wraps the request-id middleware around
    /// it (not just `CatchPanicLayer` alone) to prove `x-request-id`
    /// survives the panic path end to end, the same as any other response.
    #[tokio::test]
    async fn catch_panic_layer_converts_a_handler_panic_into_a_safe_internal_error() {
        async fn panics() -> axum::response::Response {
            panic!("sensitive panic detail that must never reach a client");
        }

        let router = axum::Router::new()
            .route("/panics", get(panics))
            .layer(CatchPanicLayer::custom(handle_panic))
            .layer(axum::middleware::from_fn(request_id_middleware));
        let request = Request::builder()
            .uri("/panics")
            .body(Body::empty())
            .unwrap();

        let response = router.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let request_id_header = response
            .headers()
            .get(REQUEST_ID_HEADER)
            .expect("x-request-id header is present even on a panic response")
            .to_str()
            .unwrap()
            .to_owned();
        let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["error"]["request_id"], request_id_header);
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

    /// Removing the production governor layer, changing its key extractor, or
    /// accidentally leaving production wiring unlimited must make this fail.
    #[tokio::test]
    async fn configured_rate_limit_returns_429_for_the_same_peer() {
        let database = Database::connect_lazy(
            &DatabaseConfig::new("postgres://user:pass@127.0.0.1:1/unreachable")
                .unwrap()
                .with_acquire_timeout(Duration::from_millis(1)),
        );
        let router = build_router(
            AppState::new(database, AuthConfig::default()),
            &RouterConfig {
                rate_limit_burst: Some(1),
                ..RouterConfig::default()
            },
        );
        let peer: SocketAddr = "127.0.0.1:41000".parse().unwrap();
        let request = || {
            let mut request = Request::builder()
                .uri("/health/live")
                .body(Body::empty())
                .unwrap();
            request.extensions_mut().insert(ConnectInfo(peer));
            request
        };

        assert_eq!(
            router.clone().oneshot(request()).await.unwrap().status(),
            StatusCode::OK
        );
        assert_eq!(
            router.oneshot(request()).await.unwrap().status(),
            StatusCode::TOO_MANY_REQUESTS
        );
    }

    #[tokio::test]
    async fn configured_rate_limit_separates_clients_behind_a_trusted_proxy() {
        let database = Database::connect_lazy(
            &DatabaseConfig::new("postgres://user:pass@127.0.0.1:1/unreachable")
                .unwrap()
                .with_acquire_timeout(Duration::from_millis(1)),
        );
        let router = build_router(
            AppState::new(database, AuthConfig::default()),
            &RouterConfig {
                rate_limit_burst: Some(1),
                trusted_proxies: TrustedProxyConfig::parse("172.30.0.0/24").unwrap(),
                ..RouterConfig::default()
            },
        );
        let peer: SocketAddr = "172.30.0.2:41000".parse().unwrap();
        let request = |client: &str| {
            let mut request = Request::builder()
                .uri("/health/live")
                .header("x-forwarded-for", client)
                .body(Body::empty())
                .unwrap();
            request.extensions_mut().insert(ConnectInfo(peer));
            request
        };

        assert_eq!(
            router
                .clone()
                .oneshot(request("203.0.113.7"))
                .await
                .unwrap()
                .status(),
            StatusCode::OK
        );
        assert_eq!(
            router
                .clone()
                .oneshot(request("203.0.113.8"))
                .await
                .unwrap()
                .status(),
            StatusCode::OK
        );
        assert_eq!(
            router
                .oneshot(request("203.0.113.7"))
                .await
                .unwrap()
                .status(),
            StatusCode::TOO_MANY_REQUESTS
        );
    }

    #[tokio::test]
    async fn configured_rate_limit_ignores_forwarding_headers_from_untrusted_peers() {
        let database = Database::connect_lazy(
            &DatabaseConfig::new("postgres://user:pass@127.0.0.1:1/unreachable")
                .unwrap()
                .with_acquire_timeout(Duration::from_millis(1)),
        );
        let router = build_router(
            AppState::new(database, AuthConfig::default()),
            &RouterConfig {
                rate_limit_burst: Some(1),
                trusted_proxies: TrustedProxyConfig::parse("172.30.0.0/24").unwrap(),
                ..RouterConfig::default()
            },
        );
        let peer: SocketAddr = "198.51.100.4:41000".parse().unwrap();
        let request = |forged: &str| {
            let mut request = Request::builder()
                .uri("/health/live")
                .header("x-forwarded-for", forged)
                .body(Body::empty())
                .unwrap();
            request.extensions_mut().insert(ConnectInfo(peer));
            request
        };

        assert_eq!(
            router
                .clone()
                .oneshot(request("203.0.113.7"))
                .await
                .unwrap()
                .status(),
            StatusCode::OK
        );
        assert_eq!(
            router
                .oneshot(request("203.0.113.8"))
                .await
                .unwrap()
                .status(),
            StatusCode::TOO_MANY_REQUESTS
        );
    }
}
