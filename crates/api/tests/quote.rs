use std::time::Duration;

use api::{AppState, RouterConfig, SECURE_SESSION_COOKIE, build_router};
use application::auth::{AuthConfig, SecretToken};
use application::seed_protection::EnvironmentSeedProtector;
use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
};
use db::{Database, DatabaseConfig, UserId};
use serde_json::Value;
use tower::ServiceExt;
use uuid::Uuid;

const TEST_SEED_KEY: &str = "AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE";

async fn test_state() -> AppState {
    let url = std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL must be set");
    let database = Database::connect(&DatabaseConfig::new(url).expect("valid database URL"))
        .await
        .expect("connect to isolated PostgreSQL");
    database.migrate().await.expect("apply migrations");
    AppState::new(database, AuthConfig::default()).with_seed_protector(std::sync::Arc::new(
        EnvironmentSeedProtector::from_base64url(TEST_SEED_KEY).expect("valid test seed key"),
    ))
}

fn router(state: &AppState) -> axum::Router {
    build_router(state.clone(), &RouterConfig::default())
}

async fn body_json(response: axum::response::Response) -> Value {
    let bytes = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("response body reads cleanly");
    serde_json::from_slice(&bytes).expect("response body is valid JSON")
}

async fn user_session(state: &AppState) -> (UserId, String) {
    let suffix = Uuid::new_v4().simple().to_string();
    let user_id: UserId = sqlx::query_scalar(
        "INSERT INTO users (login, password_hash) VALUES ($1, 'quote-http-test') RETURNING id",
    )
    .bind(format!("quote_http_{suffix}"))
    .fetch_one(state.database().pool())
    .await
    .expect("insert quote API user");
    let token = SecretToken::generate();
    db::create_user_session(
        state.database().pool(),
        user_id,
        &token.hash(),
        Duration::from_secs(3600),
    )
    .await
    .expect("create quote API session");
    (
        user_id,
        format!("{SECURE_SESSION_COOKIE}={}", token.reveal()),
    )
}

struct QuoteFixture {
    quote_public_id: Uuid,
    input_public_id: Uuid,
    outcome_public_id: Uuid,
}

async fn seed_active_quote(state: &AppState, user_id: UserId) -> QuoteFixture {
    let pool = state.database().pool();
    sqlx::query(
        "INSERT INTO contract_statuses (code, is_terminal, description) \
         VALUES ('completed', true, 'test fixture') ON CONFLICT (code) DO NOTHING",
    )
    .execute(pool)
    .await
    .expect("insert contract status fixture");
    sqlx::query(
        "INSERT INTO quote_statuses (code, is_terminal, description) \
         VALUES ('accepted', true, 'test fixture') ON CONFLICT (code) DO NOTHING",
    )
    .execute(pool)
    .await
    .expect("insert quote status fixture");
    sqlx::query(
        "INSERT INTO inventory_event_kinds (code, description) VALUES \
            ('contract_input', 'test fixture'), ('contract_output', 'test fixture') \
         ON CONFLICT (code) DO NOTHING",
    )
    .execute(pool)
    .await
    .expect("insert inventory event kinds fixture");
    let suffix = Uuid::new_v4().simple().to_string();
    sqlx::query(
        "INSERT INTO rarities (code, rank, is_covert) VALUES ('quote_http', 9701, false) \
         ON CONFLICT DO NOTHING",
    )
    .execute(pool)
    .await
    .expect("seed quote API rarity");
    sqlx::query(
        "INSERT INTO wear_bands (code, lower_bound, upper_bound, includes_upper_bound) \
         VALUES ('quote_http', 0, 1, true) ON CONFLICT (code) DO NOTHING",
    )
    .execute(pool)
    .await
    .expect("seed quote API wear band");
    sqlx::query(
        "INSERT INTO quote_statuses (code, is_terminal, description) \
         VALUES ('active', false, 'Active') ON CONFLICT (code) DO NOTHING",
    )
    .execute(pool)
    .await
    .expect("seed quote API status");
    sqlx::query(
        "INSERT INTO price_sources (code, display_name, enabled) \
         VALUES ('quote_http', 'Quote HTTP', true) ON CONFLICT (code) DO NOTHING",
    )
    .execute(pool)
    .await
    .expect("seed quote API price source");

    let collection_id: i64 = sqlx::query_scalar(
        "INSERT INTO collections (slug, display_name) VALUES ($1, 'Quote HTTP') RETURNING id",
    )
    .bind(format!("quote-http-{suffix}"))
    .fetch_one(pool)
    .await
    .expect("insert quote API collection");
    let catalog_item_id: i64 = sqlx::query_scalar(
        "INSERT INTO catalog_items (collection_id, rarity_code, stable_name, min_float, max_float) \
         VALUES ($1, 'quote_http', $2, 0, 1) RETURNING id",
    )
    .bind(collection_id)
    .bind(format!("quote-http-item-{suffix}"))
    .fetch_one(pool)
    .await
    .expect("insert quote API catalog item");
    let sku_id: i64 = sqlx::query_scalar(
        "INSERT INTO skus (catalog_item_id, wear_band_id) \
         SELECT $1, id FROM wear_bands WHERE code = 'quote_http' RETURNING id",
    )
    .bind(catalog_item_id)
    .fetch_one(pool)
    .await
    .expect("insert quote API SKU");

    let sequence = i64::from(Uuid::new_v4().as_fields().0) + 20_000_000_000;
    let commitment_id: i64 = sqlx::query_scalar(
        "INSERT INTO seed_commitments (sequence_number, commitment_hash, encoding_version) \
         VALUES ($1, digest($2, 'sha256'), 'quote-http-v1') RETURNING id",
    )
    .bind(sequence)
    .bind(&suffix)
    .fetch_one(pool)
    .await
    .expect("insert quote API commitment");
    let allocation_id: i64 = sqlx::query_scalar(
        "WITH c AS (SELECT clock_timestamp() AS now) \
         INSERT INTO seed_allocations (commitment_id, user_id, allocated_at, expires_at) \
         SELECT $1, $2, c.now, c.now + interval '15 seconds' FROM c RETURNING id",
    )
    .bind(commitment_id)
    .bind(user_id)
    .fetch_one(pool)
    .await
    .expect("insert quote API allocation");
    let snapshot_id: i64 = sqlx::query_scalar(
        "INSERT INTO valuation_snapshots (formula_version, snapshot_at) \
         VALUES ('quote-http-v1', clock_timestamp()) RETURNING id",
    )
    .fetch_one(pool)
    .await
    .expect("insert quote API snapshot");
    let snapshot_item_id: i64 = sqlx::query_scalar(
        "INSERT INTO valuation_snapshot_items ( \
            snapshot_id, sku_id, verified_price_microcredits, source_code, window_days, \
            valid_sale_count, evidence_cutoff_at, evidence_digest \
         ) VALUES ($1, $2, 20000000, 'quote_http', 7, 20, clock_timestamp(), \
                   decode(repeat('88', 32), 'hex')) RETURNING id",
    )
    .bind(snapshot_id)
    .bind(sku_id)
    .fetch_one(pool)
    .await
    .expect("insert quote API snapshot item");
    sqlx::query("UPDATE valuation_snapshots SET published_at = clock_timestamp() WHERE id = $1")
        .bind(snapshot_id)
        .execute(pool)
        .await
        .expect("publish quote API snapshot");
    sqlx::query("UPDATE risk_state SET valuation_snapshot_id = $1 WHERE singleton")
        .bind(snapshot_id)
        .execute(pool)
        .await
        .expect("activate quote API risk snapshot");
    sqlx::query(
        "UPDATE risk_state SET liquid_reserve_microcredits = 1000000000000, \
                outstanding_quote_exposure_microcredits = 8500000, \
                stressed_liability_microcredits = 0 \
         WHERE singleton",
    )
    .execute(pool)
    .await
    .expect("seed quote API risk exposure");

    let version = i32::try_from(Uuid::new_v4().as_fields().0 % 500_000_000 + 1_500_000_000)
        .expect("policy version fits i32");
    let stock_policy_id: i64 = sqlx::query_scalar(
        "INSERT INTO stock_policy_versions (version, activated_at) VALUES ($1, NULL) RETURNING id",
    )
    .bind(version)
    .fetch_one(pool)
    .await
    .expect("insert quote API stock policy");
    let risk_policy_id: i64 = sqlx::query_scalar(
        "INSERT INTO risk_policy_versions ( \
            version, minimum_coverage_ratio, maximum_quote_reserve_ratio, \
            maximum_item_liability_ratio, maximum_collection_liability_ratio, \
            minimum_notional_microcredits, maximum_dispersion_ratio, activated_at \
         ) VALUES ($1, 1, 0.1, 0.1, 0.2, 0, 1, NULL) RETURNING id",
    )
    .bind(version)
    .fetch_one(pool)
    .await
    .expect("insert quote API risk policy");
    let signing_key_id: i64 = sqlx::query_scalar(
        "INSERT INTO quote_signing_keys (public_key, activated_at) \
         VALUES (digest($1, 'sha256'), clock_timestamp()) RETURNING id",
    )
    .bind(&suffix)
    .fetch_one(pool)
    .await
    .expect("insert quote API signing key");

    let quote_public_id = Uuid::new_v4();
    let quote_id: i64 = sqlx::query_scalar(
        "WITH c AS (SELECT clock_timestamp() AS now) \
         INSERT INTO tradeup_quotes ( \
            public_id, user_id, allocation_id, commitment_id, valuation_snapshot_id, \
            stock_policy_version_id, risk_policy_version_id, signing_key_id, status_code, \
            formula_version, client_seed, nonce, verified_input_value_microcredits, \
            expected_buyback_microcredits, quote_total_microcredits, adjustment_microcredits, \
            maximum_exposure_microcredits, ordered_outcome_digest, signature, selected_outcome_position, created_at, expires_at \
         ) SELECT $1, $2, $3, $4, $5, $6, $7, $8, 'active', 'quote-http-v1', \
                  decode(repeat('99', 32), 'hex'), 7, 20000000, 8500000, 20000000, 0, \
                  8500000, decode(repeat('aa', 32), 'hex'), decode(repeat('bb', 64), 'hex'), 1, \
                  c.now, c.now + interval '15 seconds' FROM c RETURNING id",
    )
    .bind(quote_public_id)
    .bind(user_id)
    .bind(allocation_id)
    .bind(commitment_id)
    .bind(snapshot_id)
    .bind(stock_policy_id)
    .bind(risk_policy_id)
    .bind(signing_key_id)
    .fetch_one(pool)
    .await
    .expect("insert active quote API fixture");

    let input_public_id = Uuid::new_v4();
    let input_id: i64 = sqlx::query_scalar(
        "INSERT INTO inventory_items (public_id, sku_id, canonical_float) \
         VALUES ($1, $2, 0.10000000) RETURNING id",
    )
    .bind(input_public_id)
    .bind(sku_id)
    .fetch_one(pool)
    .await
    .expect("insert quoted input item");
    sqlx::query(
        "INSERT INTO inventory_positions (inventory_item_id, owner_user_id) VALUES ($1, $2)",
    )
    .bind(input_id)
    .bind(user_id)
    .execute(pool)
    .await
    .expect("position quoted input item");
    sqlx::query(
        "INSERT INTO inventory_items (public_id, sku_id, canonical_float) \
         SELECT gen_random_uuid(), $1, 0.10000000 FROM generate_series(1, 9)",
    )
    .bind(sku_id)
    .execute(pool)
    .await
    .expect("insert remaining quoted input items");
    sqlx::query(
        "INSERT INTO inventory_positions (inventory_item_id, owner_user_id) \
         SELECT id, $2 FROM ( \
             SELECT id FROM inventory_items WHERE sku_id = $1 ORDER BY id DESC LIMIT 10 \
         ) AS inputs \
         ON CONFLICT (inventory_item_id) DO NOTHING",
    )
    .bind(sku_id)
    .bind(user_id)
    .execute(pool)
    .await
    .expect("position remaining quoted input items");
    sqlx::query(
        "INSERT INTO quote_inputs (quote_id, position, inventory_item_id, \
                                   valuation_snapshot_item_id, locked_position_version) \
         SELECT $1, row_number() OVER (ORDER BY id)::int, id, $3, 1 \
           FROM (SELECT id FROM inventory_items WHERE sku_id = $2 ORDER BY id DESC LIMIT 10) AS inputs",
    )
    .bind(quote_id)
    .bind(sku_id)
    .bind(snapshot_item_id)
    .execute(pool)
    .await
    .expect("insert quote API inputs");

    let outcome_public_id = Uuid::new_v4();
    let outcome_id: i64 = sqlx::query_scalar(
        "INSERT INTO inventory_items (public_id, sku_id, canonical_float) \
         VALUES ($1, $2, 0.33333333) RETURNING id",
    )
    .bind(outcome_public_id)
    .bind(sku_id)
    .fetch_one(pool)
    .await
    .expect("insert quoted outcome item");
    sqlx::query(
        "INSERT INTO inventory_positions (inventory_item_id, in_warehouse) VALUES ($1, true)",
    )
    .bind(outcome_id)
    .execute(pool)
    .await
    .expect("position quoted outcome item");
    sqlx::query(
        "INSERT INTO quote_outcomes ( \
            quote_id, position, sku_id, candidate_inventory_item_id, valuation_snapshot_item_id, \
            probability_numerator, probability_denominator, output_float, buyback_microcredits, is_selected \
         ) VALUES ($1, 1, $2, $3, $4, 1, 2, 0.33333333, 850000, true)",
    )
    .bind(quote_id)
    .bind(sku_id)
    .bind(outcome_id)
    .bind(snapshot_item_id)
    .execute(pool)
    .await
    .expect("insert quote API outcome");
    sqlx::query(
        "INSERT INTO quote_risk_exposures \
            (quote_id, maximum_buyback_microcredits, maximum_rebate_microcredits, total_exposure_microcredits) \
         VALUES ($1, 8500000, 0, 8500000)",
    )
    .bind(quote_id)
    .execute(pool)
    .await
    .expect("insert quote API risk exposure");
    sqlx::query(
        "WITH allocation_clock AS (SELECT clock_timestamp() AS now) \
         UPDATE seed_allocations AS allocation \
            SET allocated_at = allocation_clock.now, \
                expires_at = allocation_clock.now + interval '15 seconds' \
           FROM allocation_clock \
          WHERE allocation.id = $1",
    )
    .bind(allocation_id)
    .execute(pool)
    .await
    .expect("refresh quote API allocation window");
    sqlx::query(
        "UPDATE tradeup_quotes SET created_at = clock_timestamp(), \
                expires_at = (SELECT expires_at FROM seed_allocations WHERE id = $2) \
         WHERE id = $1",
    )
    .bind(quote_id)
    .bind(allocation_id)
    .execute(pool)
    .await
    .expect("refresh quote API quote window");
    sqlx::query(
        "INSERT INTO inventory_item_locks (inventory_item_id, quote_id, expires_at) \
         SELECT inventory_item_id, quote_id, clock_timestamp() + interval '30 seconds' \
           FROM quote_inputs WHERE quote_id = $1",
    )
    .bind(quote_id)
    .execute(pool)
    .await
    .expect("lock quote API inputs");
    sqlx::query(
        "INSERT INTO quote_candidate_reservations \
            (quote_id, quote_outcome_id, inventory_item_id, sku_id, reserved_until) \
         VALUES ($1, (SELECT id FROM quote_outcomes WHERE quote_id = $1 AND position = 1), \
                 $2, $3, clock_timestamp() + interval '30 seconds')",
    )
    .bind(quote_id)
    .bind(outcome_id)
    .bind(sku_id)
    .execute(pool)
    .await
    .expect("reserve quote API outcome");
    sqlx::query(
        "INSERT INTO warehouse_stock (sku_id, available_units, reserved_units) \
         VALUES ($1, 1, 1) \
         ON CONFLICT (sku_id) DO UPDATE SET available_units = greatest(warehouse_stock.available_units, 1), \
             reserved_units = warehouse_stock.reserved_units + 1",
    )
    .bind(sku_id)
    .execute(pool)
    .await
    .expect("reserve quote API warehouse stock");

    QuoteFixture {
        quote_public_id,
        input_public_id,
        outcome_public_id,
    }
}

fn unreachable_db_state() -> AppState {
    let config = DatabaseConfig::new("postgres://user:pass@127.0.0.1:1/nonexistent")
        .expect("well-formed but unreachable URL")
        .with_acquire_timeout(Duration::from_millis(200));
    AppState::new(Database::connect_lazy(&config), AuthConfig::default())
}

#[tokio::test]
async fn quote_read_requires_an_authenticated_session_before_touching_the_database() {
    let response = build_router(unreachable_db_state(), &RouterConfig::default())
        .oneshot(
            Request::builder()
                .uri("/api/v1/me/quote")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["error"]["code"], "UNAUTHORIZED");
}

#[tokio::test]
async fn quote_acceptance_requires_an_authenticated_session_before_touching_the_database() {
    let response = build_router(unreachable_db_state(), &RouterConfig::default())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/me/quote/{}/accept", Uuid::new_v4()))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"{\"idempotency_key\":\"11111111-1111-1111-1111-111111111111\"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["error"]["code"], "UNAUTHORIZED");
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn quote_acceptance_is_owner_bound_and_idempotent_over_http() {
    let state = test_state().await;
    let (owner_id, owner_cookie) = user_session(&state).await;
    let (_, stranger_cookie) = user_session(&state).await;
    let fixture = seed_active_quote(&state, owner_id).await;
    let key = Uuid::new_v4();
    sqlx::query(
        "WITH c AS (SELECT clock_timestamp() AS now) UPDATE seed_allocations SET allocated_at = c.now, expires_at = c.now + interval '15 seconds' FROM c \
         WHERE user_id = $1 AND released_at IS NULL",
    )
    .bind(owner_id)
    .execute(state.database().pool())
    .await
    .expect("refresh acceptance allocation window");

    let request = |cookie: &str, key: Uuid| {
        Request::builder()
            .method("POST")
            .uri(format!(
                "/api/v1/me/quote/{}/accept",
                fixture.quote_public_id
            ))
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::COOKIE, cookie)
            .body(Body::from(
                serde_json::json!({"idempotency_key": key}).to_string(),
            ))
            .unwrap()
    };

    let stranger = router(&state)
        .oneshot(request(&stranger_cookie, key))
        .await
        .unwrap();
    assert_eq!(stranger.status(), StatusCode::NOT_FOUND);

    sqlx::query(
        "WITH c AS (SELECT clock_timestamp() AS now) UPDATE seed_allocations SET allocated_at = c.now, expires_at = c.now + interval '15 seconds' FROM c \
         WHERE user_id = $1 AND released_at IS NULL",
    )
    .bind(owner_id)
    .execute(state.database().pool())
    .await
    .expect("refresh owner allocation before acceptance");
    sqlx::query(
        "WITH c AS (SELECT clock_timestamp() AS now) UPDATE tradeup_quotes SET status_code = 'active', created_at = c.now, expires_at = c.now + interval '15 seconds' FROM c WHERE public_id = $1",
    )
    .bind(fixture.quote_public_id)
    .execute(state.database().pool())
    .await
    .expect("refresh owner quote before acceptance");
    sqlx::query(
        "UPDATE inventory_item_locks SET expires_at = clock_timestamp() + interval '30 seconds' WHERE quote_id = (SELECT id FROM tradeup_quotes WHERE public_id = $1)",
    )
    .bind(fixture.quote_public_id)
    .execute(state.database().pool())
    .await
    .expect("refresh owner input locks");
    sqlx::query(
        "UPDATE quote_candidate_reservations SET reserved_until = clock_timestamp() + interval '30 seconds' WHERE quote_id = (SELECT id FROM tradeup_quotes WHERE public_id = $1)",
    )
    .bind(fixture.quote_public_id)
    .execute(state.database().pool())
    .await
    .expect("refresh owner outcome reservation");
    sqlx::query(
        "UPDATE risk_state SET valuation_snapshot_id = (SELECT valuation_snapshot_id FROM tradeup_quotes WHERE public_id = $1), liquid_reserve_microcredits = 1000000000000 WHERE singleton",
    )
    .bind(fixture.quote_public_id)
    .execute(state.database().pool())
    .await
    .expect("refresh risk snapshot immediately before acceptance");
    sqlx::query(
        "UPDATE risk_state SET valuation_snapshot_id = (SELECT valuation_snapshot_id FROM tradeup_quotes WHERE public_id = $1), liquid_reserve_microcredits = 1000000000000 WHERE singleton",
    )
    .bind(fixture.quote_public_id)
    .execute(state.database().pool())
    .await
    .expect("refresh risk snapshot before acceptance");
    sqlx::query(
        "UPDATE tradeup_quotes SET valuation_snapshot_id = (SELECT valuation_snapshot_id FROM risk_state WHERE singleton) WHERE public_id = $1",
    )
    .bind(fixture.quote_public_id)
    .execute(state.database().pool())
    .await
    .expect("align quote risk snapshot");

    let first = router(&state)
        .oneshot(request(&owner_cookie, key))
        .await
        .unwrap();
    let first_body = body_json(first).await;
    assert_eq!(
        first_body["error"]["code"],
        Value::Null,
        "first acceptance failed: {first_body:?}"
    );
    let contract_id = first_body["contract_id"].as_str().unwrap().to_owned();

    let retry = router(&state)
        .oneshot(request(&owner_cookie, key))
        .await
        .unwrap();
    assert_eq!(retry.status(), StatusCode::OK);
    assert_eq!(body_json(retry).await["contract_id"], contract_id);

    let conflicting = router(&state)
        .oneshot(request(&owner_cookie, Uuid::new_v4()))
        .await
        .unwrap();
    assert_eq!(conflicting.status(), StatusCode::CONFLICT);
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn owner_allocates_an_encrypted_seed_without_secret_material_in_the_response() {
    let state = test_state().await;
    let (owner_id, owner_cookie) = user_session(&state).await;

    let first_response = router(&state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/me/quote-allocations")
                .header(header::COOKIE, owner_cookie.clone())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(first_response.status(), StatusCode::OK);
    let first_body = body_json(first_response).await;
    let first_id = Uuid::parse_str(first_body["allocation_id"].as_str().unwrap()).unwrap();
    assert_eq!(first_body["commitment"].as_array().unwrap().len(), 32);
    assert!(first_body.get("server_seed").is_none());
    assert!(first_body.get("nonce").is_none());
    assert!(first_body.get("ciphertext").is_none());

    let stored: (i32, i32, bool) = sqlx::query_as(
        "SELECT octet_length(envelope.nonce), octet_length(envelope.ciphertext), \
                allocation.released_at IS NULL \
         FROM seed_allocations allocation \
         JOIN seed_secret_envelopes envelope ON envelope.commitment_id = allocation.commitment_id \
         WHERE allocation.public_id = $1 AND allocation.user_id = $2",
    )
    .bind(first_id)
    .bind(owner_id)
    .fetch_one(state.database().pool())
    .await
    .expect("encrypted seed envelope is stored");
    assert_eq!(stored, (24, 48, true));

    let second_response = router(&state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/me/quote-allocations")
                .header(header::COOKIE, owner_cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(second_response.status(), StatusCode::CONFLICT);
    let second_body = body_json(second_response).await;
    assert_eq!(second_body["error"]["code"], "CONFLICT");
    let serialized_error = serde_json::to_string(&second_body).unwrap();
    for forbidden in ["sql", "constraint", "seed", "nonce", "ciphertext"] {
        assert!(
            !serialized_error.to_lowercase().contains(forbidden),
            "conflict response leaked {forbidden}"
        );
    }

    let first_is_active: bool =
        sqlx::query_scalar("SELECT released_at IS NULL FROM seed_allocations WHERE public_id = $1")
            .bind(first_id)
            .fetch_one(state.database().pool())
            .await
            .expect("first allocation remains the active concurrency gate");
    assert!(first_is_active);
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn owner_reads_exact_active_quote_without_internal_or_secret_fields() {
    let state = test_state().await;
    let (owner_id, owner_cookie) = user_session(&state).await;
    let fixture = seed_active_quote(&state, owner_id).await;

    let response = router(&state)
        .oneshot(
            Request::builder()
                .uri("/api/v1/me/quote")
                .header(header::COOKIE, owner_cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    assert_eq!(response.headers()[header::VARY], "Cookie");
    let body = body_json(response).await;

    assert_eq!(body["quote_id"], fixture.quote_public_id.to_string());
    assert_eq!(body["formula_version"], "quote-http-v1");
    assert_eq!(body["currency_code"], "CC");
    assert_eq!(body["input_value_microcredits"], 20_000_000);
    assert_eq!(body["expected_buyback_microcredits"], 8_500_000);
    assert_eq!(body["total_microcredits"], 20_000_000);
    assert_eq!(body["inputs"][0]["position"], 1);
    assert_eq!(
        body["inputs"][0]["item_id"],
        fixture.input_public_id.to_string()
    );
    assert_eq!(body["inputs"][0]["canonical_float"], "0.10000000");
    assert_eq!(body["outcomes"][0]["position"], 1);
    assert_eq!(body["outcomes"][0]["currency_code"], "CC");
    assert_eq!(
        body["outcomes"][0]["item_id"],
        fixture.outcome_public_id.to_string()
    );
    assert_eq!(body["outcomes"][0]["output_float"], "0.33333333");
    assert_eq!(body["outcomes"][0]["probability_numerator"], 1);
    assert_eq!(body["outcomes"][0]["probability_denominator"], 2);
    assert_eq!(body["outcomes"][0]["buyback_microcredits"], 850_000);

    let encoded = serde_json::to_string(&body).unwrap();
    for forbidden in [
        "client_seed",
        "signature",
        "allocation_id",
        "commitment_id",
        "snapshot_id",
        "signing_key_id",
        "user_id",
        "internal_id",
    ] {
        assert!(!encoded.contains(forbidden), "response leaked {forbidden}");
    }

    let (_, stranger_cookie) = user_session(&state).await;
    let response = router(&state)
        .oneshot(
            Request::builder()
                .uri("/api/v1/me/quote")
                .header(header::COOKIE, stranger_cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(body_json(response).await["error"]["code"], "NOT_FOUND");
}
