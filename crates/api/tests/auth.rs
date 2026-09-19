//! HTTP-level tests for the auth and account endpoints, driven through
//! the real router via `tower::ServiceExt::oneshot` -- no TCP listener and
//! no mocked middleware, so the cookie handling, extractor, and error
//! envelope under test are exactly the ones that ship.

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
    // `cargo test` runs many binaries in parallel, each with many tests,
    // so this suite asks for far more simultaneous Argon2id hashes than a
    // rate-limited service ever would. The production shed threshold is
    // correct and stays correct; widening it here keeps a real behaviour
    // from showing up as a flaky 503. The permit count -- the thing that
    // actually bounds memory -- is untouched.
    let _ = application::auth::set_hashing_queue_timeout(std::time::Duration::from_secs(120));

    let url = std::env::var("TEST_DATABASE_URL").ok()?;
    let database = Database::connect(&DatabaseConfig::new(url).expect("valid database URL"))
        .await
        .expect("connect to isolated PostgreSQL");
    database.migrate().await.expect("apply migrations");
    Some(AppState::new(database, AuthConfig::default()))
}

fn router(state: &AppState) -> Router {
    build_router(state.clone(), &RouterConfig::default())
}

async fn issue_invitation(state: &AppState) -> SecretToken {
    let token = SecretToken::generate();
    sqlx::query(
        "INSERT INTO invitations (token_hash, expires_at) \
         VALUES ($1, clock_timestamp() + interval '1 hour')",
    )
    .bind(token.hash().as_slice())
    .execute(state.database().pool())
    .await
    .expect("issue an invitation");
    token
}

fn unique_login(prefix: &str) -> String {
    format!("{prefix}_{}", Uuid::new_v4().simple())
}

fn post(uri: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

fn post_with_cookie(uri: &str, body: Value, cookie: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::COOKIE, cookie)
        .body(Body::from(body.to_string()))
        .unwrap()
}

fn get_with_cookie(uri: &str, cookie: &str) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .header(header::COOKIE, cookie)
        .body(Body::empty())
        .unwrap()
}

async fn body_json(response: axum::response::Response) -> Value {
    let bytes = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("response body reads cleanly");
    if bytes.is_empty() {
        return Value::Null;
    }
    serde_json::from_slice(&bytes).expect("response body is valid JSON")
}

/// Extracts the session cookie pair (`name=value`) from a login response.
fn session_cookie_pair(response: &axum::response::Response) -> String {
    let set_cookie = response
        .headers()
        .get(header::SET_COOKIE)
        .expect("login sets a session cookie")
        .to_str()
        .expect("the cookie is ASCII");
    set_cookie
        .split(';')
        .next()
        .expect("a cookie always has a name=value pair")
        .to_owned()
}

/// Registers an account and logs it in, returning its cookie pair.
async fn registered_session(state: &AppState) -> (String, String) {
    let invitation = issue_invitation(state).await;
    let login = unique_login("http");

    let response = router(state)
        .oneshot(post(
            "/api/v1/auth/register",
            json!({
                "invitation_token": invitation.reveal(),
                "login": login,
                "password": PASSWORD,
            }),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let response = router(state)
        .oneshot(post(
            "/api/v1/auth/login",
            json!({ "login": login, "password": PASSWORD }),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let cookie = session_cookie_pair(&response);
    (login, cookie)
}

macro_rules! require_database {
    () => {
        match test_state().await {
            Some(state) => state,
            None => panic!("TEST_DATABASE_URL must be set to run this test"),
        }
    };
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn register_login_me_and_balance_work_end_to_end() {
    let state = require_database!();
    let (login, cookie) = registered_session(&state).await;

    let response = router(&state)
        .oneshot(get_with_cookie("/api/v1/me", &cookie))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    assert_eq!(body["login"], login);
    assert!(
        Uuid::parse_str(body["user_id"].as_str().expect("user_id is a string")).is_ok(),
        "the account is addressed by public UUID, never an internal id"
    );

    let response = router(&state)
        .oneshot(get_with_cookie("/api/v1/me/balance", &cookie))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    assert_eq!(
        body["balance_microcredits"], 0,
        "a fresh account has no ledger account yet and reads as zero, not an error"
    );
    assert!(
        body["balance_microcredits"].is_i64(),
        "money is an integer count of microcredits, never a float"
    );
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn the_session_cookie_is_httponly_samesite_lax_and_never_echoes_the_token_in_the_body() {
    let state = require_database!();
    let invitation = issue_invitation(&state).await;
    let login = unique_login("cookie");

    router(&state)
        .oneshot(post(
            "/api/v1/auth/register",
            json!({
                "invitation_token": invitation.reveal(),
                "login": login,
                "password": PASSWORD,
            }),
        ))
        .await
        .unwrap();

    let response = router(&state)
        .oneshot(post(
            "/api/v1/auth/login",
            json!({ "login": login, "password": PASSWORD }),
        ))
        .await
        .unwrap();

    let set_cookie = response
        .headers()
        .get(header::SET_COOKIE)
        .expect("login sets a cookie")
        .to_str()
        .unwrap()
        .to_owned();
    assert!(set_cookie.contains("HttpOnly"), "{set_cookie}");
    assert!(set_cookie.contains("SameSite=Lax"), "{set_cookie}");
    assert!(set_cookie.contains("Secure"), "{set_cookie}");
    assert!(set_cookie.contains("Path=/"), "{set_cookie}");

    let token = set_cookie
        .split(';')
        .next()
        .unwrap()
        .split_once('=')
        .unwrap()
        .1
        .to_owned();
    let body = body_json(response).await;
    let rendered = body.to_string();
    assert!(
        !rendered.contains(&token),
        "the session token must travel only in the cookie, never in the body: {rendered}"
    );
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn protected_endpoints_reject_missing_tampered_and_foreign_cookies() {
    let state = require_database!();
    let (_, cookie) = registered_session(&state).await;
    let token = cookie.split_once('=').unwrap().1.to_owned();

    // No cookie at all.
    let response = router(&state)
        .oneshot(
            Request::builder()
                .uri("/api/v1/me")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let unauthenticated_body = body_json(response).await;
    assert_eq!(unauthenticated_body["error"]["code"], "UNAUTHORIZED");

    for bad_cookie in [
        format!("{SECURE_SESSION_COOKIE}={token}x"),
        format!(
            "{SECURE_SESSION_COOKIE}={}",
            SecretToken::generate().reveal()
        ),
        format!("{SECURE_SESSION_COOKIE}="),
        format!("other_cookie={token}"),
        // Cookie shadowing: the unprefixed name is the one a sibling
        // subdomain could write, and a `Secure` deployment must not read
        // it at all -- even carrying a genuinely valid token.
        format!("contracter_session={token}"),
        // A duplicated name is what an in-progress shadowing attack looks
        // like; it is refused outright rather than resolved by position.
        format!("{SECURE_SESSION_COOKIE}=attacker; {SECURE_SESSION_COOKIE}={token}"),
        format!("{SECURE_SESSION_COOKIE}={token}; {SECURE_SESSION_COOKIE}=attacker"),
    ] {
        let response = router(&state)
            .oneshot(get_with_cookie("/api/v1/me", &bad_cookie))
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "rejected for {bad_cookie}"
        );
        let body = body_json(response).await;
        assert_eq!(
            body["error"]["message"], unauthenticated_body["error"]["message"],
            "every rejection reads the same, so nothing reveals why"
        );
    }
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn one_account_can_never_read_another_accounts_data() {
    let state = require_database!();
    let (first_login, first_cookie) = registered_session(&state).await;
    let (second_login, second_cookie) = registered_session(&state).await;

    let first = body_json(
        router(&state)
            .oneshot(get_with_cookie("/api/v1/me", &first_cookie))
            .await
            .unwrap(),
    )
    .await;
    let second = body_json(
        router(&state)
            .oneshot(get_with_cookie("/api/v1/me", &second_cookie))
            .await
            .unwrap(),
    )
    .await;

    assert_eq!(first["login"], first_login);
    assert_eq!(second["login"], second_login);
    assert_ne!(
        first["user_id"], second["user_id"],
        "each session resolves to its own account"
    );
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn logout_clears_the_cookie_and_kills_only_that_session() {
    let state = require_database!();
    let (login, first_cookie) = registered_session(&state).await;

    let response = router(&state)
        .oneshot(post(
            "/api/v1/auth/login",
            json!({ "login": login, "password": PASSWORD }),
        ))
        .await
        .unwrap();
    let second_cookie = session_cookie_pair(&response);

    let response = router(&state)
        .oneshot(post_with_cookie(
            "/api/v1/auth/logout",
            json!({}),
            &first_cookie,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let cleared = response
        .headers()
        .get(header::SET_COOKIE)
        .expect("logout clears the cookie")
        .to_str()
        .unwrap();
    assert!(cleared.contains("Max-Age=0"), "{cleared}");

    let response = router(&state)
        .oneshot(get_with_cookie("/api/v1/me", &first_cookie))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let response = router(&state)
        .oneshot(get_with_cookie("/api/v1/me", &second_cookie))
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "logging out one session must not disturb another"
    );
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn logout_all_kills_every_session_of_the_caller_and_requires_authentication() {
    let state = require_database!();
    let (login, first_cookie) = registered_session(&state).await;
    let response = router(&state)
        .oneshot(post(
            "/api/v1/auth/login",
            json!({ "login": login, "password": PASSWORD }),
        ))
        .await
        .unwrap();
    let second_cookie = session_cookie_pair(&response);
    let (_, bystander_cookie) = registered_session(&state).await;

    // Unauthenticated callers cannot revoke anyone's sessions.
    let response = router(&state)
        .oneshot(post("/api/v1/auth/logout-all", json!({})))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let response = router(&state)
        .oneshot(post_with_cookie(
            "/api/v1/auth/logout-all",
            json!({}),
            &first_cookie,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    assert_eq!(body["revoked_sessions"], 2);

    for cookie in [&first_cookie, &second_cookie] {
        let response = router(&state)
            .oneshot(get_with_cookie("/api/v1/me", cookie))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
    let response = router(&state)
        .oneshot(get_with_cookie("/api/v1/me", &bystander_cookie))
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "logout-all is scoped to the caller's own account"
    );
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn a_wrong_password_and_an_unknown_login_return_the_same_401() {
    let state = require_database!();
    let (login, _) = registered_session(&state).await;

    let wrong = router(&state)
        .oneshot(post(
            "/api/v1/auth/login",
            json!({ "login": login, "password": "an-entirely-different-password" }),
        ))
        .await
        .unwrap();
    assert_eq!(wrong.status(), StatusCode::UNAUTHORIZED);
    assert!(
        wrong.headers().get(header::SET_COOKIE).is_none(),
        "a failed login must not set any cookie"
    );
    let wrong_body = body_json(wrong).await;

    let unknown = router(&state)
        .oneshot(post(
            "/api/v1/auth/login",
            json!({ "login": unique_login("ghost"), "password": PASSWORD }),
        ))
        .await
        .unwrap();
    assert_eq!(unknown.status(), StatusCode::UNAUTHORIZED);
    let unknown_body = body_json(unknown).await;

    assert_eq!(
        wrong_body["error"]["message"], unknown_body["error"]["message"],
        "the two failures must be indistinguishable over HTTP"
    );
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn registration_rejects_bad_input_and_spent_invitations_with_distinct_codes() {
    let state = require_database!();
    let invitation = issue_invitation(&state).await;

    let short_login = router(&state)
        .oneshot(post(
            "/api/v1/auth/register",
            json!({ "invitation_token": invitation.reveal(), "login": "ab", "password": PASSWORD }),
        ))
        .await
        .unwrap();
    assert_eq!(short_login.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let weak_password = router(&state)
        .oneshot(post(
            "/api/v1/auth/register",
            json!({
                "invitation_token": invitation.reveal(),
                "login": unique_login("weak"),
                "password": "short",
            }),
        ))
        .await
        .unwrap();
    assert_eq!(weak_password.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let login = unique_login("spend");
    let created = router(&state)
        .oneshot(post(
            "/api/v1/auth/register",
            json!({
                "invitation_token": invitation.reveal(),
                "login": login,
                "password": PASSWORD,
            }),
        ))
        .await
        .unwrap();
    assert_eq!(
        created.status(),
        StatusCode::CREATED,
        "the rejected attempts must not have consumed the invitation"
    );

    let reused = router(&state)
        .oneshot(post(
            "/api/v1/auth/register",
            json!({
                "invitation_token": invitation.reveal(),
                "login": unique_login("late"),
                "password": PASSWORD,
            }),
        ))
        .await
        .unwrap();
    assert_eq!(reused.status(), StatusCode::FORBIDDEN);
    let reused_body = body_json(reused).await;

    let never_existed = router(&state)
        .oneshot(post(
            "/api/v1/auth/register",
            json!({
                "invitation_token": SecretToken::generate().reveal(),
                "login": unique_login("nope"),
                "password": PASSWORD,
            }),
        ))
        .await
        .unwrap();
    assert_eq!(never_existed.status(), StatusCode::FORBIDDEN);
    let never_existed_body = body_json(never_existed).await;
    assert_eq!(
        reused_body["error"]["message"], never_existed_body["error"]["message"],
        "a spent invitation and one that never existed must look identical"
    );

    let taken = router(&state)
        .oneshot(post(
            "/api/v1/auth/register",
            json!({
                "invitation_token": issue_invitation(&state).await.reveal(),
                "login": login.to_uppercase(),
                "password": PASSWORD,
            }),
        ))
        .await
        .unwrap();
    assert_eq!(taken.status(), StatusCode::CONFLICT);
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn auth_failures_never_leak_sql_or_stored_hashes() {
    let state = require_database!();
    let (login, cookie) = registered_session(&state).await;

    let responses = [
        router(&state)
            .oneshot(post(
                "/api/v1/auth/login",
                json!({ "login": login, "password": "wrong-but-long-enough" }),
            ))
            .await
            .unwrap(),
        router(&state)
            .oneshot(get_with_cookie(
                "/api/v1/me",
                &format!("{SECURE_SESSION_COOKIE}=bogus"),
            ))
            .await
            .unwrap(),
        router(&state)
            .oneshot(post(
                "/api/v1/auth/register",
                json!({
                    "invitation_token": SecretToken::generate().reveal(),
                    "login": unique_login("leak"),
                    "password": PASSWORD,
                }),
            ))
            .await
            .unwrap(),
    ];

    for response in responses {
        let rendered = body_json(response).await.to_string().to_lowercase();
        for forbidden in [
            "argon2",
            "$argon",
            "select ",
            "insert ",
            "sqlx",
            "postgres",
            "pg_",
            "password_hash",
            "user_sessions",
            "invitation_redemptions",
            "constraint",
        ] {
            assert!(
                !rendered.contains(forbidden),
                "an auth error must not disclose {forbidden}: {rendered}"
            );
        }
    }

    // The genuine session still works after all that.
    let response = router(&state)
        .oneshot(get_with_cookie("/api/v1/me", &cookie))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn a_disabled_account_loses_access_immediately() {
    let state = require_database!();
    let (login, cookie) = registered_session(&state).await;

    sqlx::query("UPDATE users SET disabled_at = clock_timestamp() WHERE lower(login) = lower($1)")
        .bind(&login)
        .execute(state.database().pool())
        .await
        .expect("disable the account");

    let response = router(&state)
        .oneshot(get_with_cookie("/api/v1/me", &cookie))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let response = router(&state)
        .oneshot(post(
            "/api/v1/auth/login",
            json!({ "login": login, "password": PASSWORD }),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

/// The leak sweep above only ever sees 4xx bodies, which are hand-written
/// strings -- it would pass even if the 500 path echoed the SQLx error
/// verbatim. This provokes a genuine internal error by pointing the router
/// at a database that is not there, and checks the one response an
/// attacker would most like to read.
#[tokio::test]
async fn an_internal_database_failure_returns_a_sanitized_500() {
    // Port 1 on the loopback: nothing listens there, and `connect_lazy`
    // means the failure happens at query time, inside the handler, exactly
    // where a production outage would put it.
    let config =
        DatabaseConfig::new("postgresql://contracter:contracter@127.0.0.1:1/contracter".to_owned())
            .expect("valid database URL");
    let state = AppState::new(Database::connect_lazy(&config), AuthConfig::default());

    let response = router(&state)
        .oneshot(get_with_cookie(
            "/api/v1/me",
            &format!("{SECURE_SESSION_COOKIE}=any-token-at-all"),
        ))
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::INTERNAL_SERVER_ERROR,
        "an unreachable database must surface as a 500, not as a 401"
    );
    let rendered = body_json(response).await.to_string().to_lowercase();
    for forbidden in [
        "sqlx",
        "postgres",
        "connection",
        "refused",
        "127.0.0.1",
        "contracter:",
        "pool",
        "timed out",
    ] {
        assert!(
            !rendered.contains(forbidden),
            "a 500 body must not contain {forbidden:?}: {rendered}"
        );
    }
}

/// `/api/v1` responses are per-account and cookie-dependent, so a shared
/// cache must never be free to reuse one caller's for another's.
#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn authenticated_responses_are_not_cacheable_and_vary_on_the_cookie() {
    let state = require_database!();
    let (_, cookie) = registered_session(&state).await;

    for uri in ["/api/v1/me", "/api/v1/me/balance"] {
        let response = router(&state)
            .oneshot(get_with_cookie(uri, &cookie))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK, "{uri}");
        assert_eq!(
            response
                .headers()
                .get(header::CACHE_CONTROL)
                .and_then(|value| value.to_str().ok()),
            Some("no-store"),
            "{uri} must not be written down by any cache"
        );
        assert_eq!(
            response
                .headers()
                .get(header::VARY)
                .and_then(|value| value.to_str().ok()),
            Some("Cookie"),
            "{uri} must tell caches the cookie selects the response"
        );
    }

    // The unauthenticated rejection carries them too: a cached 401 served
    // to an authenticated caller is its own kind of wrong.
    let response = router(&state)
        .oneshot(get_with_cookie("/api/v1/me", "other=1"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        response
            .headers()
            .get(header::CACHE_CONTROL)
            .and_then(|value| value.to_str().ok()),
        Some("no-store")
    );
}
