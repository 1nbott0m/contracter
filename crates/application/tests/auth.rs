//! Real-PostgreSQL tests for the auth use-cases. Unlike the `db` crate's
//! suite these commit (the use-cases take `&Database` and run against the
//! pool, exactly as the HTTP layer will), so every fixture uses a unique
//! login and a fresh invitation; the rows left behind are harmless debris
//! in a disposable test database.

use std::time::Duration;

use application::auth::{self, AuthConfig, AuthError, SecretToken};
use db::{Database, DatabaseConfig};
use uuid::Uuid;

const PASSWORD: &str = "a-sufficiently-long-password";

async fn test_database() -> Database {
    let url = std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL must be set");
    let database = Database::connect(&DatabaseConfig::new(url).expect("valid database URL"))
        .await
        .expect("connect to isolated PostgreSQL");
    database.migrate().await.expect("apply migrations");
    database
}

/// Issues a real invitation and returns the raw token, mirroring what an
/// admin-side invitation flow will eventually hand to a new user.
async fn issue_invitation(database: &Database) -> SecretToken {
    let token = SecretToken::generate();
    sqlx::query(
        "INSERT INTO invitations (token_hash, expires_at) \
         VALUES ($1, clock_timestamp() + interval '1 hour')",
    )
    .bind(token.hash().as_slice())
    .execute(database.pool())
    .await
    .expect("issue an invitation");
    token
}

fn unique_login(prefix: &str) -> String {
    format!("{prefix}_{}", Uuid::new_v4().simple())
}

async fn register_and_login(database: &Database) -> (String, auth::IssuedSession) {
    let invitation = issue_invitation(database).await;
    let login = unique_login("app");
    auth::register(database, &invitation, &login, PASSWORD)
        .await
        .expect("registration succeeds");
    let session = auth::login(database, AuthConfig::default(), &login, PASSWORD)
        .await
        .expect("login succeeds");
    (login, session)
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn register_then_login_then_authenticate_round_trips() {
    let database = test_database().await;
    let invitation = issue_invitation(&database).await;
    let login = unique_login("roundtrip");

    let user_public_id = auth::register(&database, &invitation, &login, PASSWORD)
        .await
        .expect("registration succeeds");

    let session = auth::login(&database, AuthConfig::default(), &login, PASSWORD)
        .await
        .expect("login succeeds");

    let caller = auth::authenticate(&database, &session.token)
        .await
        .expect("the issued session authenticates");
    assert_eq!(caller.user_public_id, user_public_id);
    assert_eq!(caller.session_public_id, session.session_public_id);

    let account = auth::account(&database, caller.user_public_id)
        .await
        .expect("the account resolves");
    assert_eq!(account.login, login);
    assert_eq!(account.user_public_id, user_public_id);
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn a_wrong_password_and_an_unknown_login_are_reported_identically() {
    let database = test_database().await;
    let (login, _) = register_and_login(&database).await;

    let wrong_password = auth::login(
        &database,
        AuthConfig::default(),
        &login,
        "an-entirely-different-password",
    )
    .await
    .expect_err("a wrong password must fail");
    let unknown_login = auth::login(
        &database,
        AuthConfig::default(),
        &unique_login("nobody"),
        PASSWORD,
    )
    .await
    .expect_err("an unknown login must fail");

    assert!(matches!(wrong_password, AuthError::InvalidCredentials));
    assert!(matches!(unknown_login, AuthError::InvalidCredentials));
    assert_eq!(
        wrong_password.to_string(),
        unknown_login.to_string(),
        "the two cases must be indistinguishable to the caller"
    );
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn a_disabled_account_cannot_log_in_and_its_sessions_stop_authenticating() {
    let database = test_database().await;
    let (login, session) = register_and_login(&database).await;

    auth::authenticate(&database, &session.token)
        .await
        .expect("the session works before the account is disabled");

    sqlx::query("UPDATE users SET disabled_at = clock_timestamp() WHERE lower(login) = lower($1)")
        .bind(&login)
        .execute(database.pool())
        .await
        .expect("disable the account");

    let authenticate_error = auth::authenticate(&database, &session.token)
        .await
        .expect_err("a disabled account's live session must stop working");
    assert!(matches!(authenticate_error, AuthError::SessionInvalid));

    let login_error = auth::login(&database, AuthConfig::default(), &login, PASSWORD)
        .await
        .expect_err("a disabled account must not be able to log in");
    assert!(matches!(login_error, AuthError::InvalidCredentials));
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn logout_invalidates_only_the_presented_session() {
    let database = test_database().await;
    let (login, first_session) = register_and_login(&database).await;
    let second_session = auth::login(&database, AuthConfig::default(), &login, PASSWORD)
        .await
        .expect("a second concurrent session");

    auth::logout(&database, &first_session.token)
        .await
        .expect("logout succeeds");
    // Logging out twice is not an error.
    auth::logout(&database, &first_session.token)
        .await
        .expect("logout is idempotent");

    assert!(matches!(
        auth::authenticate(&database, &first_session.token)
            .await
            .expect_err("the logged-out session is dead"),
        AuthError::SessionInvalid
    ));
    auth::authenticate(&database, &second_session.token)
        .await
        .expect("the other session survives");
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn logout_all_invalidates_every_session_of_that_user_only() {
    let database = test_database().await;
    let (login, first_session) = register_and_login(&database).await;
    let second_session = auth::login(&database, AuthConfig::default(), &login, PASSWORD)
        .await
        .expect("a second session");
    let (_, bystander_session) = register_and_login(&database).await;

    let caller = auth::authenticate(&database, &first_session.token)
        .await
        .expect("resolve the caller");
    let revoked = auth::logout_all(&database, caller.user_id)
        .await
        .expect("logout everywhere");
    assert_eq!(revoked, 2);

    for token in [&first_session.token, &second_session.token] {
        assert!(matches!(
            auth::authenticate(&database, token)
                .await
                .expect_err("every session of that user is dead"),
            AuthError::SessionInvalid
        ));
    }
    auth::authenticate(&database, &bystander_session.token)
        .await
        .expect("another user's session is untouched");
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn an_unknown_or_tampered_session_token_never_authenticates() {
    let database = test_database().await;
    let (_, session) = register_and_login(&database).await;

    for candidate in [
        SecretToken::generate(),
        SecretToken::new(""),
        SecretToken::new(format!("{}x", session.token.reveal())),
        SecretToken::new(session.token.reveal().to_uppercase()),
    ] {
        assert!(
            matches!(
                auth::authenticate(&database, &candidate).await,
                Err(AuthError::SessionInvalid)
            ),
            "a token that is not exactly the issued one must never authenticate"
        );
    }

    auth::authenticate(&database, &session.token)
        .await
        .expect("the genuine token still works");
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn an_invitation_is_single_use_and_its_state_is_not_disclosed() {
    let database = test_database().await;
    let invitation = issue_invitation(&database).await;
    auth::register(&database, &invitation, &unique_login("first"), PASSWORD)
        .await
        .expect("the first registration succeeds");

    let reuse = auth::register(&database, &invitation, &unique_login("second"), PASSWORD)
        .await
        .expect_err("a spent invitation must not work again");
    let unknown = auth::register(
        &database,
        &SecretToken::generate(),
        &unique_login("third"),
        PASSWORD,
    )
    .await
    .expect_err("an invitation that never existed must not work");

    assert!(matches!(reuse, AuthError::InvitationUnusable));
    assert!(matches!(unknown, AuthError::InvitationUnusable));
    assert_eq!(
        reuse.to_string(),
        unknown.to_string(),
        "a spent invitation and a nonexistent one must look identical"
    );
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn a_taken_login_is_rejected_without_consuming_the_invitation() {
    let database = test_database().await;
    let first_invitation = issue_invitation(&database).await;
    let second_invitation = issue_invitation(&database).await;
    let login = unique_login("taken");
    auth::register(&database, &first_invitation, &login, PASSWORD)
        .await
        .expect("the first registration succeeds");

    let error = auth::register(
        &database,
        &second_invitation,
        &login.to_uppercase(),
        PASSWORD,
    )
    .await
    .expect_err("a duplicate login must be rejected");
    assert!(matches!(error, AuthError::LoginTaken));

    // The second invitation must still be usable: a failed registration
    // cannot silently burn it.
    auth::register(
        &database,
        &second_invitation,
        &unique_login("recovered"),
        PASSWORD,
    )
    .await
    .expect("the untouched invitation still works");
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn invalid_inputs_are_rejected_before_any_invitation_is_consumed() {
    let database = test_database().await;
    let invitation = issue_invitation(&database).await;

    assert!(matches!(
        auth::register(&database, &invitation, "ab", PASSWORD)
            .await
            .expect_err("a too-short login is rejected"),
        AuthError::InvalidLogin
    ));
    assert!(matches!(
        auth::register(&database, &invitation, " padded ", PASSWORD)
            .await
            .expect_err("a padded login is rejected"),
        AuthError::InvalidLogin
    ));
    assert!(matches!(
        auth::register(&database, &invitation, &unique_login("weak"), "short")
            .await
            .expect_err("a short password is rejected"),
        AuthError::WeakPassword
    ));

    auth::register(&database, &invitation, &unique_login("valid"), PASSWORD)
        .await
        .expect("the invitation was never consumed by the rejected attempts");
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn an_expired_session_stops_authenticating() {
    let database = test_database().await;
    let invitation = issue_invitation(&database).await;
    let login = unique_login("expiry");
    auth::register(&database, &invitation, &login, PASSWORD)
        .await
        .expect("register");
    let session = auth::login(
        &database,
        AuthConfig {
            session_ttl: Duration::from_secs(3600),
        },
        &login,
        PASSWORD,
    )
    .await
    .expect("login");

    // Both timestamps move together: the table's
    // CHECK (expires_at > created_at) refuses an inconsistent row.
    sqlx::query(
        "UPDATE user_sessions \
            SET created_at = clock_timestamp() - interval '2 hours', \
                expires_at = clock_timestamp() - interval '1 hour' \
          WHERE public_id = $1",
    )
    .bind(session.session_public_id)
    .execute(database.pool())
    .await
    .expect("force expiry");

    assert!(matches!(
        auth::authenticate(&database, &session.token)
            .await
            .expect_err("an expired session must not authenticate"),
        AuthError::SessionInvalid
    ));
}
