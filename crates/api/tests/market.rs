//! HTTP contracts for public market reads. These tests use the real router
//! and PostgreSQL because the visibility rules are enforced by joins across
//! catalog, pricing, halt, and stock projections.

use std::time::Duration;

use api::{AppState, RouterConfig, build_router};
use application::auth::AuthConfig;
use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
};
use db::{Database, DatabaseConfig};
use serde_json::Value;
use tower::ServiceExt;
use uuid::Uuid;

async fn test_state() -> AppState {
    let url = std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL must be set");
    let database = Database::connect(&DatabaseConfig::new(url).expect("valid database URL"))
        .await
        .expect("connect to isolated PostgreSQL");
    database.migrate().await.expect("apply migrations");
    AppState::new(database, AuthConfig::default())
}

fn router(state: &AppState) -> axum::Router {
    build_router(state.clone(), &RouterConfig::default())
}

fn unreachable_db_state() -> AppState {
    let config = DatabaseConfig::new("postgres://user:pass@127.0.0.1:1/nonexistent")
        .expect("well-formed but unreachable URL")
        .with_acquire_timeout(Duration::from_millis(200));
    AppState::new(Database::connect_lazy(&config), AuthConfig::default())
}

async fn body_json(response: axum::response::Response) -> Value {
    let bytes = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("response body reads cleanly");
    serde_json::from_slice(&bytes).expect("response body is valid JSON")
}

async fn seed_market_sku(
    state: &AppState,
    source_code: &str,
    available_units: i32,
    reserved_units: i32,
) -> Uuid {
    let pool = state.database().pool();
    let suffix = Uuid::new_v4().simple().to_string();
    sqlx::query(
        "INSERT INTO rarities (code, rank, is_covert) \
         VALUES ('http_market', 9601, false) ON CONFLICT (code) DO NOTHING",
    )
    .execute(pool)
    .await
    .expect("seed market rarity");
    sqlx::query(
        "INSERT INTO wear_bands (code, lower_bound, upper_bound, includes_upper_bound) \
         VALUES ('http_market', 0, 1, true) ON CONFLICT (code) DO NOTHING",
    )
    .execute(pool)
    .await
    .expect("seed market wear band");
    sqlx::query(
        "INSERT INTO price_sources (code, display_name, enabled) \
         VALUES ($1, $1, $2) ON CONFLICT (code) DO UPDATE SET enabled = EXCLUDED.enabled",
    )
    .bind(source_code)
    .bind(source_code != "http_market_disabled")
    .execute(pool)
    .await
    .expect("seed market price source");
    sqlx::query(
        "INSERT INTO price_halt_reasons (code, description) \
         VALUES ('http_market', 'HTTP market test') ON CONFLICT (code) DO NOTHING",
    )
    .execute(pool)
    .await
    .expect("seed market halt reason");

    let collection_id: i64 = sqlx::query_scalar(
        "INSERT INTO collections (slug, display_name) VALUES ($1, 'HTTP Market') RETURNING id",
    )
    .bind(format!("http-market-{suffix}"))
    .fetch_one(pool)
    .await
    .expect("insert collection");
    let item_id: i64 = sqlx::query_scalar(
        "INSERT INTO catalog_items (collection_id, rarity_code, stable_name, min_float, max_float) 
         VALUES ($1, 'http_market', $2, 0, 1) RETURNING id",
    )
    .bind(collection_id)
    .bind(format!("http-market-item-{suffix}"))
    .fetch_one(pool)
    .await
    .expect("insert catalog item");
    let (sku_id, sku_public_id): (i64, Uuid) = sqlx::query_as(
        "INSERT INTO skus (catalog_item_id, wear_band_id) 
         SELECT $1, id FROM wear_bands WHERE code = 'http_market' 
         RETURNING id, public_id",
    )
    .bind(item_id)
    .fetch_one(pool)
    .await
    .expect("insert SKU");
    let snapshot_id: i64 = sqlx::query_scalar(
        "INSERT INTO valuation_snapshots (formula_version, snapshot_at) 
         VALUES ('http-market-v1', clock_timestamp()) RETURNING id",
    )
    .fetch_one(pool)
    .await
    .expect("insert valuation snapshot");
    let snapshot_item_id: i64 = sqlx::query_scalar(
        "INSERT INTO valuation_snapshot_items ( 
            snapshot_id, sku_id, verified_price_microcredits, source_code, 
            window_days, valid_sale_count, evidence_cutoff_at, evidence_digest 
         ) VALUES ( 
            $1, $2, 1_234_567, $3, 7, 20, clock_timestamp(), 
            decode(repeat('88', 32), 'hex') 
         ) RETURNING id",
    )
    .bind(snapshot_id)
    .bind(sku_id)
    .bind(source_code)
    .fetch_one(pool)
    .await
    .expect("insert valuation snapshot item");
    sqlx::query("UPDATE valuation_snapshots SET published_at = clock_timestamp() WHERE id = $1")
        .bind(snapshot_id)
        .execute(pool)
        .await
        .expect("publish valuation snapshot");
    sqlx::query(
        "INSERT INTO current_valuations 
            (sku_id, snapshot_id, snapshot_item_id, verified_price_microcredits) 
         VALUES ($1, $2, $3, 1_234_567)",
    )
    .bind(sku_id)
    .bind(snapshot_id)
    .bind(snapshot_item_id)
    .execute(pool)
    .await
    .expect("insert current valuation");
    sqlx::query(
        "INSERT INTO warehouse_stock (sku_id, available_units, reserved_units) 
         VALUES ($1, $2, $3)",
    )
    .bind(sku_id)
    .bind(available_units)
    .bind(reserved_units)
    .execute(pool)
    .await
    .expect("insert warehouse stock");

    sku_public_id
}

async fn halt_price(state: &AppState, sku_public_id: Uuid) {
    sqlx::query(
        "INSERT INTO price_halts (sku_id, reason_code, observed_ratio) 
         SELECT id, 'http_market', 1.25 FROM skus WHERE public_id = $1",
    )
    .bind(sku_public_id)
    .execute(state.database().pool())
    .await
    .expect("halt price");
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn market_valuations_expose_only_boolean_availability_and_hide_halted_or_disabled_prices() {
    let state = test_state().await;
    let available_sku = seed_market_sku(&state, "http_market_enabled", 2, 1).await;
    let unavailable_sku = seed_market_sku(&state, "http_market_enabled", 1, 1).await;
    let halted_sku = seed_market_sku(&state, "http_market_enabled", 2, 0).await;
    halt_price(&state, halted_sku).await;
    let disabled_source_sku = seed_market_sku(&state, "http_market_disabled", 2, 0).await;

    let response = router(&state)
        .oneshot(
            Request::builder()
                .uri("/api/v1/market/valuations")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    let body = body_json(response).await;
    let items = body["items"].as_array().expect("market listing array");

    let available = items
        .iter()
        .find(|item| item["sku_id"] == available_sku.to_string())
        .expect("available SKU is listed");
    assert_eq!(available["price_microcredits"], 1_234_567);
    assert_eq!(available["available"], true);
    assert!(available.get("available_units").is_none());
    assert!(available.get("reserved_units").is_none());
    assert!(available.get("sku_internal_id").is_none());

    let unavailable = items
        .iter()
        .find(|item| item["sku_id"] == unavailable_sku.to_string())
        .expect("out-of-stock SKU remains visible with its boolean status");
    assert_eq!(unavailable["available"], false);
    assert!(
        items
            .iter()
            .all(|item| item["sku_id"] != halted_sku.to_string())
    );
    assert!(
        items
            .iter()
            .all(|item| item["sku_id"] != disabled_source_sku.to_string())
    );
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn active_price_halts_expose_public_skus_and_timestamps_without_risk_details() {
    let state = test_state().await;
    let halted_sku = seed_market_sku(&state, "http_market_enabled", 2, 0).await;
    halt_price(&state, halted_sku).await;

    let response = router(&state)
        .oneshot(
            Request::builder()
                .uri("/api/v1/market/price-halts")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    let body = body_json(response).await;
    let halted = body["items"]
        .as_array()
        .expect("price halt array")
        .iter()
        .find(|item| item["sku_id"] == halted_sku.to_string())
        .expect("active price halt is listed");
    assert!(halted["halted_at"].is_string());
    assert!(halted.get("reason_code").is_none());
    assert!(halted.get("observed_ratio").is_none());
    assert!(halted.get("halt_id").is_none());

    sqlx::query(
        "UPDATE catalog_items AS item SET enabled = false \
         FROM skus WHERE skus.catalog_item_id = item.id AND skus.public_id = $1",
    )
    .bind(halted_sku)
    .execute(state.database().pool())
    .await
    .expect("disable the halted SKU's catalog item");
    let response = router(&state)
        .oneshot(
            Request::builder()
                .uri("/api/v1/market/price-halts")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = body_json(response).await;
    assert!(
        body["items"]
            .as_array()
            .expect("price halt array")
            .iter()
            .any(|item| item["sku_id"] == halted_sku.to_string()),
        "an active halt must remain visible until it is lifted, even after a catalog disable"
    );
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn market_valuation_uses_the_published_snapshot_price_when_the_cache_drifts() {
    let state = test_state().await;
    let sku = seed_market_sku(&state, "http_market_enabled", 2, 0).await;
    sqlx::query(
        "UPDATE current_valuations AS valuation \
         SET verified_price_microcredits = 9_999_999 \
         FROM skus WHERE skus.id = valuation.sku_id AND skus.public_id = $1",
    )
    .bind(sku)
    .execute(state.database().pool())
    .await
    .expect("simulate an owner-level cached valuation drift");

    let response = router(&state)
        .oneshot(
            Request::builder()
                .uri("/api/v1/market/valuations")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    let listing = body["items"]
        .as_array()
        .expect("market listing array")
        .iter()
        .find(|item| item["sku_id"] == sku.to_string())
        .expect("drifted cached valuation remains backed by its published snapshot");
    assert_eq!(listing["price_microcredits"], 1_234_567);
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn market_valuation_pagination_uses_its_cursor_to_advance() {
    let state = test_state().await;
    seed_market_sku(&state, "http_market_enabled", 2, 0).await;
    seed_market_sku(&state, "http_market_enabled", 2, 0).await;

    let response = router(&state)
        .oneshot(
            Request::builder()
                .uri("/api/v1/market/valuations?limit=1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let first_page = body_json(response).await;
    let first_sku = first_page["items"][0]["sku_id"]
        .as_str()
        .expect("first page has one SKU")
        .to_owned();
    let cursor = first_page["next_cursor"]
        .as_str()
        .expect("a full page has a continuation cursor");

    let response = router(&state)
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/market/valuations?limit=1&cursor={cursor}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let second_page = body_json(response).await;
    let second_sku = second_page["items"][0]["sku_id"]
        .as_str()
        .expect("the continuation page has one SKU");
    assert_ne!(second_sku, first_sku, "a cursor must not repeat page one");
}

#[tokio::test]
async fn malformed_market_queries_are_rejected_before_a_database_query() {
    let state = unreachable_db_state();
    for uri in [
        "/api/v1/market/valuations?cursor=not-a-real-cursor",
        "/api/v1/market/price-halts?unexpected=true",
    ] {
        let response = router(&state)
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{uri}");
        assert_eq!(body_json(response).await["error"]["code"], "BAD_REQUEST");
    }
}
