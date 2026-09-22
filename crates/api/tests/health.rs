use std::time::Duration;

use api::{AppState, RouterConfig, build_router};
use application::auth::AuthConfig;
use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use db::{Database, DatabaseConfig};
use serde_json::Value;
use tower::ServiceExt;

/// A `Database` pointed at an address nothing listens on, built lazily so
/// constructing it never touches the network (`Database::connect` does,
/// and how long an unreachable host takes to fail is not something a test
/// should depend on). The first real query -- exactly what
/// `health_check`/readiness does -- fails, bounded by `acquire_timeout`.
/// This lets every test below except the "readiness succeeds" one run
/// without a real PostgreSQL instance, matching this repo's rule that
/// PostgreSQL integration is only claimed verified when `TEST_DATABASE_URL`
/// is set and actually used.
fn unreachable_db_state() -> AppState {
    let config = DatabaseConfig::new("postgres://user:pass@127.0.0.1:1/nonexistent")
        .expect("well-formed but unreachable URL")
        .with_acquire_timeout(Duration::from_millis(200));
    AppState::new(Database::connect_lazy(&config), AuthConfig::default())
}

async fn real_db_state() -> Option<AppState> {
    let url = std::env::var("TEST_DATABASE_URL").ok()?;
    let config = DatabaseConfig::new(url).expect("valid TEST_DATABASE_URL");
    let database = Database::connect(&config)
        .await
        .expect("connect to isolated PostgreSQL");
    Some(AppState::new(database, AuthConfig::default()))
}

fn router_with(state: AppState) -> axum::Router {
    build_router(state, &RouterConfig::default())
}

async fn body_json(response: axum::response::Response) -> Value {
    let bytes = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("response body reads cleanly");
    serde_json::from_slice(&bytes).expect("response body is valid JSON")
}

#[tokio::test]
async fn liveness_returns_200_with_stable_json_and_no_db_dependency() {
    let router = router_with(unreachable_db_state());
    let request = Request::builder()
        .uri("/health/live")
        .body(Body::empty())
        .unwrap();

    let response = router.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    assert_eq!(body["status"], "live");
}

#[tokio::test]
async fn readiness_reports_503_and_a_sanitized_envelope_when_the_database_is_unreachable() {
    let router = router_with(unreachable_db_state());
    let request = Request::builder()
        .uri("/health/ready")
        .body(Body::empty())
        .unwrap();

    let response = router.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let request_id_header = response
        .headers()
        .get(api::REQUEST_ID_HEADER)
        .expect("x-request-id header is present")
        .to_str()
        .unwrap()
        .to_owned();

    let body = body_json(response).await;
    assert_eq!(body["error"]["code"], "SERVICE_UNAVAILABLE");
    assert_eq!(body["error"]["request_id"], request_id_header);

    let raw = body.to_string();
    assert!(
        !raw.contains("127.0.0.1")
            && !raw.contains("user:pass")
            && !raw.to_lowercase().contains("sqlx")
            && !raw.to_lowercase().contains("connection refused"),
        "readiness failure must never leak connection details: {raw}"
    );
}

#[tokio::test]
async fn a_database_outage_does_not_affect_liveness() {
    let router = router_with(unreachable_db_state());
    let request = Request::builder()
        .uri("/health/live")
        .body(Body::empty())
        .unwrap();

    let response = router.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn unknown_route_returns_404_in_the_standard_error_envelope() {
    let router = router_with(unreachable_db_state());
    let request = Request::builder()
        .uri("/this/route/does/not/exist")
        .body(Body::empty())
        .unwrap();

    let response = router.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body = body_json(response).await;
    assert_eq!(body["error"]["code"], "NOT_FOUND");
    assert!(body["error"]["request_id"].is_string());
}

#[tokio::test]
async fn wrong_method_on_a_valid_route_still_gets_the_standard_envelope() {
    let router = router_with(unreachable_db_state());
    let request = Request::builder()
        .method("POST")
        .uri("/health/live")
        .body(Body::empty())
        .unwrap();

    let response = router.oneshot(request).await.unwrap();
    // 405 keeps its meaning rather than collapsing to 400. It used to
    // collapse, which produced a self-contradictory response -- a 400
    // carrying axum's `Allow` header -- and made "wrong method"
    // indistinguishable from "malformed body".
    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
    assert!(
        response.headers().contains_key(axum::http::header::ALLOW),
        "a 405 must still say which methods would work"
    );
    let body = body_json(response).await;
    assert_eq!(body["error"]["code"], "METHOD_NOT_ALLOWED");
    assert!(body["error"]["request_id"].is_string());
}

#[tokio::test]
async fn every_response_carries_a_request_id_header() {
    let router = router_with(unreachable_db_state());
    let request = Request::builder()
        .uri("/health/live")
        .body(Body::empty())
        .unwrap();

    let response = router.oneshot(request).await.unwrap();
    let header = response
        .headers()
        .get(api::REQUEST_ID_HEADER)
        .expect("x-request-id header is present")
        .to_str()
        .unwrap();
    assert!(uuid::Uuid::parse_str(header).is_ok());
}

#[tokio::test]
async fn a_caller_supplied_request_id_is_honored_and_echoed_back() {
    let router = router_with(unreachable_db_state());
    let supplied = uuid::Uuid::new_v4();
    let request = Request::builder()
        .uri("/health/live")
        .header(api::REQUEST_ID_HEADER, supplied.to_string())
        .body(Body::empty())
        .unwrap();

    let response = router.oneshot(request).await.unwrap();
    let header = response
        .headers()
        .get(api::REQUEST_ID_HEADER)
        .unwrap()
        .to_str()
        .unwrap();
    assert_eq!(header, supplied.to_string());
}

#[tokio::test]
async fn health_endpoints_are_not_nested_under_api_v1() {
    let router = router_with(unreachable_db_state());
    let request = Request::builder()
        .uri("/api/v1/health/live")
        .body(Body::empty())
        .unwrap();

    let response = router.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn readiness_failure_never_leaks_the_configured_database_credential() {
    let config = DatabaseConfig::new("postgres://admin:super-secret@example/db")
        .expect("well-formed URL")
        .with_acquire_timeout(Duration::from_millis(200));
    let router = router_with(AppState::new(
        Database::connect_lazy(&config),
        AuthConfig::default(),
    ));
    let request = Request::builder()
        .uri("/health/ready")
        .body(Body::empty())
        .unwrap();

    let response = router.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let bytes = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("response body reads cleanly");
    let raw = String::from_utf8_lossy(&bytes);
    assert!(
        !raw.contains("super-secret") && !raw.contains("admin"),
        "the error response must never contain the database credential: {raw}"
    );
}

#[tokio::test]
async fn a_request_that_exceeds_the_configured_timeout_gets_a_deterministic_503() {
    let config = RouterConfig {
        request_timeout: Duration::from_millis(5),
        ..RouterConfig::default()
    };
    // acquire_timeout is intentionally far longer than request_timeout, so
    // the outer timeout layer -- not the DB call itself -- is what fires.
    let db_config = DatabaseConfig::new("postgres://user:pass@127.0.0.1:1/nonexistent")
        .expect("well-formed but unreachable URL")
        .with_acquire_timeout(Duration::from_secs(5));
    let router = build_router(
        AppState::new(Database::connect_lazy(&db_config), AuthConfig::default()),
        &config,
    );
    let request = Request::builder()
        .uri("/health/ready")
        .body(Body::empty())
        .unwrap();

    // Timed, because the status alone cannot tell the two causes apart:
    // the timeout layer firing and the database call failing both come
    // back as SERVICE_UNAVAILABLE. Removing the timeout layer left the
    // status assertion green and simply took five seconds instead of five
    // milliseconds -- so the only thing that distinguishes "the timeout
    // fired" from "the pool gave up" is how long it took.
    let started = std::time::Instant::now();
    let response = router.oneshot(request).await.unwrap();
    let elapsed = started.elapsed();

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert!(
        elapsed < Duration::from_secs(2),
        "the request timeout must fire long before the 5s pool acquire timeout, took {elapsed:?}"
    );
    let body = body_json(response).await;
    assert_eq!(body["error"]["code"], "SERVICE_UNAVAILABLE");
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn readiness_returns_200_against_a_real_postgresql_instance() {
    let Some(state) = real_db_state().await else {
        panic!("TEST_DATABASE_URL must be set to run this test");
    };
    let router = router_with(state);
    let request = Request::builder()
        .uri("/health/ready")
        .body(Body::empty())
        .unwrap();

    let response = router.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    assert_eq!(body["status"], "ready");
}

/// A draining process must report itself unready before its socket closes.
///
/// `with_graceful_shutdown` stops accepting the instant the signal
/// arrives. A load balancer only learns an instance is gone from its next
/// readiness probe, so without this every rolling deploy produced a burst
/// of connection-refused for callers routed in between.
///
/// Needs a reachable database, because the assertion that matters is the
/// transition. An earlier version of this test used an unreachable one,
/// where readiness was already 503 before draining began -- so deleting
/// the draining check from the handler left it green. 200 before and 503
/// after is the property; 503 before and 503 after proves nothing.
#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn a_draining_process_reports_itself_unready() {
    let state = real_db_state()
        .await
        .expect("TEST_DATABASE_URL must be set to run this test");
    let draining = state.draining_handle();
    let router = router_with(state);

    let ready = |router: axum::Router| async move {
        router
            .oneshot(
                Request::builder()
                    .uri("/health/ready")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap()
    };

    assert_eq!(
        ready(router.clone()).await.status(),
        StatusCode::OK,
        "a healthy process with a reachable database is ready before draining"
    );

    draining.store(true, std::sync::atomic::Ordering::Release);

    let response = ready(router.clone()).await;
    assert_eq!(
        response.status(),
        StatusCode::SERVICE_UNAVAILABLE,
        "and unready once draining, with the database still perfectly reachable"
    );
    let body = body_json(response).await;
    assert_eq!(body["error"]["code"], "SERVICE_UNAVAILABLE");

    // Liveness stays up while draining: the process is still serving
    // in-flight work, and an orchestrator that killed it now would cut
    // those requests off.
    let response = router
        .oneshot(
            Request::builder()
                .uri("/health/live")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "draining is not the same as dead"
    );
}
