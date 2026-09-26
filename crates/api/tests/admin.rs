//! HTTP boundary checks for administrator-only routes.

use std::time::Duration;

use api::{AppState, RouterConfig, build_router};
use application::auth::AuthConfig;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use db::{Database, DatabaseConfig};
use tower::ServiceExt;
use uuid::Uuid;

fn router() -> axum::Router {
    let config = DatabaseConfig::new("postgres://user:pass@127.0.0.1:1/unreachable")
        .expect("well-formed URL")
        .with_acquire_timeout(Duration::from_millis(100));
    let state = AppState::new(Database::connect_lazy(&config), AuthConfig::default());
    build_router(state, &RouterConfig::default())
}

#[tokio::test]
async fn admin_reads_and_mutations_require_a_session_before_database_access() {
    let uris = vec![
        "/api/v1/admin/me".to_owned(),
        "/api/v1/admin/dashboard".to_owned(),
        "/api/v1/admin/users".to_owned(),
        "/api/v1/admin/audit".to_owned(),
        "/api/v1/admin/ledger".to_owned(),
        "/api/v1/admin/market-purchases".to_owned(),
        format!("/api/v1/admin/users/{}/disable", Uuid::new_v4()),
    ];
    for uri in uris {
        let response = router()
            .oneshot(
                Request::builder()
                    .method(if uri.ends_with("/disable") {
                        "POST"
                    } else {
                        "GET"
                    })
                    .uri(&uri)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{uri}");
    }
}
