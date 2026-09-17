//! HTTP-level tests for the catalog and inventory read endpoints, driven
//! through the real router via `tower::ServiceExt::oneshot`.

use api::{AppState, RouterConfig, SECURE_SESSION_COOKIE, build_router};
use application::auth::{AuthConfig, SecretToken};
use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
};
use db::{Database, DatabaseConfig};
use serde_json::{Value, json};
use tower::ServiceExt;
use uuid::Uuid;

const PASSWORD: &str = "a-sufficiently-long-password";

async fn test_state() -> Option<AppState> {
    let url = std::env::var("TEST_DATABASE_URL").ok()?;
    let database = Database::connect(&DatabaseConfig::new(url).expect("valid database URL"))
        .await
        .expect("connect to isolated PostgreSQL");
    database.migrate().await.expect("apply migrations");
    Some(AppState::new(database, AuthConfig::default()))
}

macro_rules! require_database {
    () => {
        match test_state().await {
            Some(state) => state,
            None => panic!("TEST_DATABASE_URL must be set to run this test"),
        }
    };
}

fn router(state: &AppState) -> Router {
    build_router(state.clone(), &RouterConfig::default())
}

fn get(uri: &str) -> Request<Body> {
    Request::builder().uri(uri).body(Body::empty()).unwrap()
}

fn get_with_cookie(uri: &str, cookie: &str) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .header(header::COOKIE, cookie)
        .body(Body::empty())
        .unwrap()
}

async fn body_json(response: axum::response::Response) -> Value {
    let bytes = to_bytes(response.into_body(), 4 * 1024 * 1024)
        .await
        .expect("response body reads cleanly");
    if bytes.is_empty() {
        return Value::Null;
    }
    serde_json::from_slice(&bytes).expect("response body is valid JSON")
}

/// Registers an account, logs it in, and returns `(user_id, cookie)`.
async fn registered_session(state: &AppState) -> (i64, String) {
    let invitation = SecretToken::generate();
    sqlx::query(
        "INSERT INTO invitations (token_hash, expires_at) \
         VALUES ($1, clock_timestamp() + interval '1 hour')",
    )
    .bind(invitation.hash().as_slice())
    .execute(state.database().pool())
    .await
    .expect("issue an invitation");

    let login = format!("cat_{}", Uuid::new_v4().simple());
    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/register")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "invitation_token": invitation.reveal(),
                        "login": login,
                        "password": PASSWORD,
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/login")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({ "login": login, "password": PASSWORD }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let cookie = response
        .headers()
        .get(header::SET_COOKIE)
        .expect("login sets a cookie")
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();

    let user_id: i64 = sqlx::query_scalar("SELECT id FROM users WHERE login = $1")
        .bind(&login)
        .fetch_one(state.database().pool())
        .await
        .expect("read the internal id for fixture setup only");

    (user_id, cookie)
}

/// A minimal active quote, which exists only so an item lock has
/// something to point at.
///
/// `tradeup_quotes` has seven non-null foreign keys and a signature, all
/// of which a real quote earns in BACKEND-06. None of that is what this
/// test is about -- it needs one row that satisfies the constraints so
/// `inventory_item_locks` can reference it.
async fn seed_quote(state: &AppState, user_id: i64) -> i64 {
    let pool = state.database().pool();
    sqlx::query(
        "INSERT INTO quote_statuses (code, is_terminal, description) \
         VALUES ('active', false, 'test active') ON CONFLICT (code) DO NOTHING",
    )
    .execute(pool)
    .await
    .expect("seed quote status");

    // `version` columns are `integer`, so the fixture stays well inside
    // i32 while still being unique enough not to collide across runs.
    let version = 900_000 + i32::try_from(rand_suffix() % 1_000_000).expect("in range");
    let sequence = i64::from(version);
    let commitment_id: i64 = sqlx::query_scalar(
        "INSERT INTO seed_commitments (sequence_number, commitment_hash, encoding_version) \
         VALUES ($1, $2, 'test-v1') RETURNING id",
    )
    .bind(sequence)
    .bind(unique_bytes())
    .fetch_one(pool)
    .await
    .expect("insert seed commitment");
    // `clock_timestamp()` is volatile: two readings in one statement can
    // land microseconds apart and violate the zero-margin
    // "expires_at <= allocated_at + 15s" CHECK. One reading feeds both.
    let allocation_id: i64 = sqlx::query_scalar(
        "WITH allocation_clock AS (SELECT clock_timestamp() AS now) \
         INSERT INTO seed_allocations (commitment_id, user_id, allocated_at, expires_at) \
         SELECT $1, $2, allocation_clock.now, allocation_clock.now + interval '15 seconds' \
         FROM allocation_clock RETURNING id",
    )
    .bind(commitment_id)
    .bind(user_id)
    .fetch_one(pool)
    .await
    .expect("insert seed allocation");
    let snapshot_id: i64 = sqlx::query_scalar(
        "INSERT INTO valuation_snapshots (formula_version, snapshot_at) \
         VALUES ('http-test-v1', clock_timestamp()) RETURNING id",
    )
    .fetch_one(pool)
    .await
    .expect("insert valuation snapshot");
    // Deliberately left un-activated (`activated_at IS NULL`), like the
    // risk policy below.
    //
    // This fixture commits, unlike the transaction-scoped ones in the `db`
    // suite, so anything it activates is activated for every other test
    // sharing this database. An active stock policy version with no bands
    // leaves every collection uncovered, which silently breaks scarcity
    // publishing in a test that has nothing to do with this one. The quote
    // needs the foreign key to resolve, not the policy to be in force.
    let stock_policy_id: i64 = sqlx::query_scalar(
        "INSERT INTO stock_policy_versions (version, activated_at) \
         VALUES ($1, NULL) RETURNING id",
    )
    .bind(version)
    .fetch_one(pool)
    .await
    .expect("insert stock policy");
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
    .expect("insert risk policy");
    let signing_key_id: i64 = sqlx::query_scalar(
        "INSERT INTO quote_signing_keys (public_key, activated_at) \
         VALUES ($1, clock_timestamp()) RETURNING id",
    )
    .bind(unique_bytes())
    .fetch_one(pool)
    .await
    .expect("insert signing key");

    sqlx::query_scalar(
        "INSERT INTO tradeup_quotes ( \
            user_id, allocation_id, commitment_id, valuation_snapshot_id, \
            stock_policy_version_id, risk_policy_version_id, signing_key_id, \
            status_code, formula_version, client_seed, nonce, \
            verified_input_value_microcredits, expected_buyback_microcredits, \
            quote_total_microcredits, adjustment_microcredits, \
            maximum_exposure_microcredits, ordered_outcome_digest, signature, \
            created_at, expires_at \
         ) \
         SELECT $1, $2, $3, $4, $5, $6, $7, 'active', 'http-test-v1', \
                decode(repeat('33', 32), 'hex'), 0, 0, 0, 0, 0, 0, \
                decode(repeat('44', 32), 'hex'), decode(repeat('55', 64), 'hex'), \
                quote_clock.now, quote_clock.now + interval '30 seconds' \
         FROM (SELECT clock_timestamp() AS now) AS quote_clock \
         RETURNING id",
    )
    .bind(user_id)
    .bind(allocation_id)
    .bind(commitment_id)
    .bind(snapshot_id)
    .bind(stock_policy_id)
    .bind(risk_policy_id)
    .bind(signing_key_id)
    .fetch_one(pool)
    .await
    .expect("insert active quote")
}

/// Keeps the several `UNIQUE` version columns in the quote fixture from
/// colliding across runs against a shared database.
fn rand_suffix() -> u32 {
    Uuid::new_v4().as_fields().0
}

/// 32 distinct bytes for the fixture's `UNIQUE` hash and key columns.
/// These stand in for real cryptographic material, which BACKEND-06
/// produces; all this test needs is that two runs never collide.
fn unique_bytes() -> Vec<u8> {
    let mut bytes = Uuid::new_v4().as_bytes().to_vec();
    bytes.extend_from_slice(Uuid::new_v4().as_bytes());
    bytes
}

/// Gives `owner` one item, returning its public UUID. Committed rather
/// than transactional, because the router uses its own pooled
/// connections and cannot see an uncommitted fixture.
async fn give_item(state: &AppState, owner: i64, retired: bool, locked: bool) -> Uuid {
    let suffix = Uuid::new_v4().simple().to_string();
    let pool = state.database().pool();

    // `rarities.rank` is globally UNIQUE, and this fixture commits rather
    // than rolling back, so a rank near the other suites' 90-95 block
    // would make an unrelated test fail on a duplicate key. Sitting far
    // outside every range in use keeps that from happening again.
    sqlx::query(
        "INSERT INTO rarities (code, rank, is_covert) VALUES ('http_catalog', 9501, false) \
         ON CONFLICT (code) DO NOTHING",
    )
    .execute(pool)
    .await
    .expect("seed rarity");
    sqlx::query(
        "INSERT INTO wear_bands (code, lower_bound, upper_bound, includes_upper_bound) \
         VALUES ('http_catalog', 0, 1, true) ON CONFLICT (code) DO NOTHING",
    )
    .execute(pool)
    .await
    .expect("seed wear band");

    let collection_id: i64 = sqlx::query_scalar(
        "INSERT INTO collections (slug, display_name) VALUES ($1, 'HTTP Catalog') RETURNING id",
    )
    .bind(format!("http-catalog-{suffix}"))
    .fetch_one(pool)
    .await
    .expect("insert collection");
    let catalog_item_id: i64 = sqlx::query_scalar(
        "INSERT INTO catalog_items (collection_id, rarity_code, stable_name, min_float, max_float) \
         VALUES ($1, 'http_catalog', $2, 0, 1) RETURNING id",
    )
    .bind(collection_id)
    .bind(format!("http-item-{suffix}"))
    .fetch_one(pool)
    .await
    .expect("insert catalog item");
    let sku_id: i64 = sqlx::query_scalar(
        "INSERT INTO skus (catalog_item_id, wear_band_id) \
         SELECT $1, id FROM wear_bands WHERE code = 'http_catalog' RETURNING id",
    )
    .bind(catalog_item_id)
    .fetch_one(pool)
    .await
    .expect("insert sku");

    let public_id = Uuid::new_v4();
    let item_id: i64 = sqlx::query_scalar(
        "INSERT INTO inventory_items (public_id, sku_id, canonical_float, retired_at) \
         VALUES ($1, $2, 0.12345678, CASE WHEN $3 THEN clock_timestamp() ELSE NULL END) \
         RETURNING id",
    )
    .bind(public_id)
    .bind(sku_id)
    .bind(retired)
    .fetch_one(pool)
    .await
    .expect("insert inventory item");
    sqlx::query(
        "INSERT INTO inventory_positions (inventory_item_id, owner_user_id) VALUES ($1, $2)",
    )
    .bind(item_id)
    .bind(owner)
    .execute(pool)
    .await
    .expect("insert position");

    if locked {
        let quote_id = seed_quote(state, owner).await;
        sqlx::query(
            "INSERT INTO inventory_item_locks (inventory_item_id, quote_id, expires_at) \
             VALUES ($1, $2, clock_timestamp() + interval '1 hour')",
        )
        .bind(item_id)
        .bind(quote_id)
        .execute(pool)
        .await
        .expect("lock item");
    }

    public_id
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn the_catalog_is_public_paginated_and_free_of_internal_ids() {
    let state = require_database!();

    let response = router(&state)
        .oneshot(get("/api/v1/catalog/collections?limit=2"))
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "the catalog needs no session: nothing in it is account-specific"
    );
    let body = body_json(response).await;
    let items = body["items"].as_array().expect("items is an array");
    assert!(items.len() <= 2, "the limit is honoured");

    for item in items {
        assert!(
            Uuid::parse_str(item["collection_id"].as_str().expect("a string")).is_ok(),
            "collections are addressed by public UUID"
        );
        assert!(
            item.get("id").is_none() && item.get("collection_internal_id").is_none(),
            "no internal sequential id appears in the response: {item}"
        );
    }

    // A limit beyond the ceiling is clamped rather than obeyed.
    let response = router(&state)
        .oneshot(get("/api/v1/catalog/skus?limit=65535"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    assert!(
        body["items"].as_array().expect("array").len() <= 200,
        "a client cannot ask the database for unbounded work"
    );
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn a_bad_cursor_or_unknown_filter_is_refused_rather_than_ignored() {
    let state = require_database!();

    for uri in [
        "/api/v1/catalog/collections?cursor=not-a-cursor",
        "/api/v1/catalog/skus?cursor=%21%21%21%21",
        "/api/v1/catalog/collections?cursor=",
    ] {
        let response = router(&state).oneshot(get(uri)).await.unwrap();
        assert_eq!(
            response.status(),
            StatusCode::BAD_REQUEST,
            "a corrupt cursor must not be answered with page one: {uri}"
        );
    }

    // A misspelled filter is refused, not silently dropped -- answering
    // an intended filter with the unfiltered list is the worst default
    // available.
    let response = router(&state)
        .oneshot(get("/api/v1/catalog/skus?colection=abc"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    // A filter that parses but matches nothing is an empty page, not an
    // error: "no results" is a legitimate answer.
    let response = router(&state)
        .oneshot(get(&format!(
            "/api/v1/catalog/skus?collection={}",
            Uuid::new_v4()
        )))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    assert!(body["items"].as_array().expect("array").is_empty());
    assert!(
        body.get("next_cursor").is_none(),
        "an exhausted walk carries no cursor"
    );
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn inventory_requires_a_session_and_shows_locked_but_not_retired_items() {
    let state = require_database!();
    let (owner, cookie) = registered_session(&state).await;

    let plain = give_item(&state, owner, false, false).await;
    let locked = give_item(&state, owner, false, true).await;
    let retired = give_item(&state, owner, true, false).await;

    // No cookie, no inventory.
    for uri in [
        "/api/v1/me/inventory".to_owned(),
        format!("/api/v1/me/inventory/{plain}"),
    ] {
        let response = router(&state).oneshot(get(&uri)).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{uri}");
    }

    let response = router(&state)
        .oneshot(get_with_cookie("/api/v1/me/inventory?limit=200", &cookie))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    let items = body["items"].as_array().expect("array");

    let find = |wanted: Uuid| {
        items
            .iter()
            .find(|item| item["item_id"].as_str() == Some(&wanted.to_string()))
    };

    let plain_item = find(plain).expect("the free item is listed");
    assert_eq!(plain_item["locked"], false);
    assert_eq!(
        plain_item["canonical_float"], "0.12345678",
        "float is an exact decimal string, never a JSON binary float"
    );
    assert!(plain_item["canonical_float"].is_string());

    assert_eq!(
        find(locked).expect("a reserved item is still owned")["locked"],
        true,
        "an item reserved in a quote is flagged, not hidden"
    );
    assert!(
        find(retired).is_none(),
        "a retired item is gone rather than reserved"
    );

    // Detail agrees with the listing.
    let response = router(&state)
        .oneshot(get_with_cookie(
            &format!("/api/v1/me/inventory/{plain}"),
            &cookie,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let detail = body_json(response).await;
    assert_eq!(detail["item_id"], plain.to_string());
    assert_eq!(detail["locked"], false);
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn one_account_can_never_read_another_accounts_inventory() {
    let state = require_database!();
    let (alice, alice_cookie) = registered_session(&state).await;
    let (_bob, bob_cookie) = registered_session(&state).await;
    let alices_item = give_item(&state, alice, false, false).await;

    // Bob's listing never contains Alice's item.
    let response = router(&state)
        .oneshot(get_with_cookie(
            "/api/v1/me/inventory?limit=200",
            &bob_cookie,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    assert!(
        !body["items"]
            .as_array()
            .expect("array")
            .iter()
            .any(|item| item["item_id"].as_str() == Some(&alices_item.to_string())),
        "another account's item must not appear in this account's inventory"
    );

    // Knowing the UUID is not authorization, and the refusal is
    // indistinguishable from one for an item that never existed.
    let foreign = router(&state)
        .oneshot(get_with_cookie(
            &format!("/api/v1/me/inventory/{alices_item}"),
            &bob_cookie,
        ))
        .await
        .unwrap();
    assert_eq!(foreign.status(), StatusCode::NOT_FOUND);
    let foreign_body = body_json(foreign).await;

    let nonexistent = router(&state)
        .oneshot(get_with_cookie(
            &format!("/api/v1/me/inventory/{}", Uuid::new_v4()),
            &bob_cookie,
        ))
        .await
        .unwrap();
    assert_eq!(nonexistent.status(), StatusCode::NOT_FOUND);
    let nonexistent_body = body_json(nonexistent).await;

    assert_eq!(
        foreign_body["error"]["message"], nonexistent_body["error"]["message"],
        "an item owned by someone else and one that never existed read identically"
    );

    // The real owner still reads it.
    let owned = router(&state)
        .oneshot(get_with_cookie(
            &format!("/api/v1/me/inventory/{alices_item}"),
            &alice_cookie,
        ))
        .await
        .unwrap();
    assert_eq!(owned.status(), StatusCode::OK);
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn paging_the_inventory_returns_every_item_exactly_once() {
    let state = require_database!();
    let (owner, cookie) = registered_session(&state).await;

    let mut expected = Vec::new();
    for _ in 0..5 {
        expected.push(give_item(&state, owner, false, false).await.to_string());
    }
    expected.sort();

    let mut seen = Vec::new();
    let mut uri = "/api/v1/me/inventory?limit=2".to_owned();
    loop {
        let response = router(&state)
            .oneshot(get_with_cookie(&uri, &cookie))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = body_json(response).await;
        let items = body["items"].as_array().expect("array").clone();
        assert!(items.len() <= 2);
        seen.extend(
            items
                .iter()
                .map(|item| item["item_id"].as_str().expect("a string").to_owned()),
        );

        let Some(cursor) = body["next_cursor"].as_str() else {
            break;
        };
        uri = format!("/api/v1/me/inventory?limit=2&cursor={cursor}");
    }

    let mut sorted = seen.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), seen.len(), "no item is returned twice");
    assert_eq!(sorted, expected, "and none is skipped");
}

/// Session-bearing responses must not be cacheable, and the inventory is
/// the most account-specific thing the API serves.
#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn inventory_responses_are_not_cacheable() {
    let state = require_database!();
    let (_, cookie) = registered_session(&state).await;

    let response = router(&state)
        .oneshot(get_with_cookie("/api/v1/me/inventory", &cookie))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(header::CACHE_CONTROL)
            .and_then(|value| value.to_str().ok()),
        Some("no-store")
    );
    assert_eq!(
        response
            .headers()
            .get(header::VARY)
            .and_then(|value| value.to_str().ok()),
        Some("Cookie")
    );
}

/// A malformed UUID in the path is a client error, and the response must
/// not echo back anything that reveals how the id is stored.
#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn a_malformed_item_id_is_rejected_without_leaking_internals() {
    let state = require_database!();
    let (_, cookie) = registered_session(&state).await;

    for bad in ["not-a-uuid", "1", "0"] {
        let response = router(&state)
            .oneshot(get_with_cookie(
                &format!("/api/v1/me/inventory/{bad}"),
                &cookie,
            ))
            .await
            .unwrap();
        assert!(
            response.status().is_client_error(),
            "{bad} must be a client error, not a 500"
        );
        let rendered = body_json(response).await.to_string().to_lowercase();
        for forbidden in ["sqlx", "postgres", "inventory_items", "select ", "bigint"] {
            assert!(
                !rendered.contains(forbidden),
                "{bad}: response must not contain {forbidden:?}: {rendered}"
            );
        }
    }

    // A sequential id is not an address here: even if one happens to
    // exist internally, it is not a UUID and so resolves to nothing.
    let response = router(&state)
        .oneshot(get_with_cookie(
            &format!("/api/v1/me/inventory/{SECURE_SESSION_COOKIE}"),
            &cookie,
        ))
        .await
        .unwrap();
    assert!(response.status().is_client_error());
}
