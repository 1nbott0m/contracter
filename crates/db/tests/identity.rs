use std::time::Duration;

use db::{
    Database, DatabaseConfig, UserId, create_user_session, find_account_by_public_id,
    find_active_user_session, find_user_credential_by_login, register_invited_user,
    revoke_all_user_sessions, revoke_user_session,
};
use sqlx::{Executor, Postgres, Transaction};
use uuid::Uuid;

async fn test_database() -> Database {
    let url = std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL must be set");
    let database = Database::connect(&DatabaseConfig::new(url).expect("valid database URL"))
        .await
        .expect("connect to isolated PostgreSQL");
    database.migrate().await.expect("apply migrations");
    database
}

async fn isolated_transaction(database: &Database) -> Transaction<'_, Postgres> {
    let mut transaction = database.begin().await.expect("begin transaction");
    transaction
        .execute("SELECT pg_advisory_xact_lock(812_202_609_61)")
        .await
        .expect("serialize identity integration fixtures");
    transaction
}

/// A fresh 32-byte token hash. Tests never need the "raw token" side of
/// the pair -- hashing is the application layer's job; `db` only ever
/// sees hashes.
fn token_hash(seed: u8) -> Vec<u8> {
    let mut hash = Uuid::new_v4().as_bytes().to_vec();
    hash.extend_from_slice(Uuid::new_v4().as_bytes());
    hash[0] = seed;
    hash
}

async fn insert_invitation(transaction: &mut Transaction<'_, Postgres>, hash: &[u8], valid: bool) {
    if valid {
        sqlx::query(
            "INSERT INTO invitations (token_hash, expires_at) \
             VALUES ($1, clock_timestamp() + interval '1 hour')",
        )
        .bind(hash)
        .execute(transaction.as_mut())
        .await
        .expect("insert invitation");
    } else {
        sqlx::query(
            "INSERT INTO invitations (token_hash, created_at, expires_at) \
             VALUES ($1, clock_timestamp() - interval '2 hours', \
                     clock_timestamp() - interval '1 hour')",
        )
        .bind(hash)
        .execute(transaction.as_mut())
        .await
        .expect("insert expired invitation");
    }
}

fn unique_login(prefix: &str) -> String {
    format!("{prefix}_{}", Uuid::new_v4().simple())
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn registration_redeems_the_invitation_and_creates_a_reachable_account() {
    let database = test_database().await;
    let mut transaction = isolated_transaction(&database).await;
    let invitation = token_hash(1);
    insert_invitation(&mut transaction, &invitation, true).await;
    let login = unique_login("identity");

    let user_public_id = register_invited_user(
        transaction.as_mut(),
        &invitation,
        &login,
        "argon2id$stored$hash",
    )
    .await
    .expect("register the invited user");

    let account = find_account_by_public_id(transaction.as_mut(), user_public_id)
        .await
        .expect("query the new account")
        .expect("the account exists");
    assert_eq!(account.login, login);
    assert_eq!(account.user_public_id, user_public_id);

    let redeemed: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM invitation_redemptions AS redemption \
           JOIN invitations AS invitation ON invitation.id = redemption.invitation_id \
          WHERE invitation.token_hash = $1)",
    )
    .bind(&invitation)
    .fetch_one(transaction.as_mut())
    .await
    .expect("check redemption");
    assert!(redeemed, "registering must consume the invitation");

    transaction.rollback().await.expect("rollback fixture");
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn an_invitation_cannot_be_redeemed_twice_or_after_expiry() {
    let database = test_database().await;
    let mut transaction = isolated_transaction(&database).await;
    let invitation = token_hash(2);
    let expired = token_hash(3);
    insert_invitation(&mut transaction, &invitation, true).await;
    insert_invitation(&mut transaction, &expired, false).await;

    register_invited_user(
        transaction.as_mut(),
        &invitation,
        &unique_login("first"),
        "hash",
    )
    .await
    .expect("first redemption succeeds");

    transaction
        .execute("SAVEPOINT reuse")
        .await
        .expect("savepoint");
    let reuse = register_invited_user(
        transaction.as_mut(),
        &invitation,
        &unique_login("second"),
        "hash",
    )
    .await
    .expect_err("a redeemed invitation must not be reusable");
    assert_eq!(reuse.database_code().as_deref(), Some("23514"));
    transaction
        .execute("ROLLBACK TO SAVEPOINT reuse")
        .await
        .expect("recover");

    let expired_error = register_invited_user(
        transaction.as_mut(),
        &expired,
        &unique_login("third"),
        "hash",
    )
    .await
    .expect_err("an expired invitation must be rejected");
    assert_eq!(expired_error.database_code().as_deref(), Some("23514"));

    transaction.rollback().await.expect("rollback fixture");
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn an_unknown_invitation_is_rejected_without_creating_a_user() {
    let database = test_database().await;
    let mut transaction = isolated_transaction(&database).await;
    let login = unique_login("ghost");

    transaction
        .execute("SAVEPOINT unknown_invitation")
        .await
        .expect("savepoint");
    let error = register_invited_user(transaction.as_mut(), &token_hash(4), &login, "hash")
        .await
        .expect_err("an unknown invitation must be rejected");
    assert_eq!(error.database_code().as_deref(), Some("23503"));
    transaction
        .execute("ROLLBACK TO SAVEPOINT unknown_invitation")
        .await
        .expect("recover after the expected rejection");

    assert!(
        find_user_credential_by_login(transaction.as_mut(), &login)
            .await
            .expect("credential lookup")
            .is_none(),
        "a rejected registration must leave no user behind"
    );

    transaction.rollback().await.expect("rollback fixture");
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn logins_are_unique_case_insensitively_and_credentials_resolve_either_way() {
    let database = test_database().await;
    let mut transaction = isolated_transaction(&database).await;
    let first_invitation = token_hash(5);
    let second_invitation = token_hash(6);
    insert_invitation(&mut transaction, &first_invitation, true).await;
    insert_invitation(&mut transaction, &second_invitation, true).await;
    let login = unique_login("CaseUser");

    register_invited_user(
        transaction.as_mut(),
        &first_invitation,
        &login,
        "argon2id$stored$hash",
    )
    .await
    .expect("register");

    transaction
        .execute("SAVEPOINT duplicate")
        .await
        .expect("savepoint");
    let duplicate = register_invited_user(
        transaction.as_mut(),
        &second_invitation,
        &login.to_uppercase(),
        "hash",
    )
    .await
    .expect_err("logins must be unique regardless of case");
    assert_eq!(duplicate.database_code().as_deref(), Some("23505"));
    transaction
        .execute("ROLLBACK TO SAVEPOINT duplicate")
        .await
        .expect("recover");

    let credential = find_user_credential_by_login(transaction.as_mut(), &login.to_uppercase())
        .await
        .expect("credential lookup")
        .expect("the credential resolves case-insensitively");
    assert_eq!(credential.password_hash, "argon2id$stored$hash");
    assert!(credential.disabled_at.is_none());

    assert!(
        find_user_credential_by_login(transaction.as_mut(), &unique_login("absent"))
            .await
            .expect("credential lookup for an unknown login")
            .is_none()
    );

    transaction.rollback().await.expect("rollback fixture");
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn a_session_resolves_until_it_is_revoked_expired_or_its_user_is_disabled() {
    let database = test_database().await;
    let mut transaction = isolated_transaction(&database).await;
    let invitation = token_hash(7);
    insert_invitation(&mut transaction, &invitation, true).await;
    let login = unique_login("session");
    let user_public_id = register_invited_user(transaction.as_mut(), &invitation, &login, "hash")
        .await
        .expect("register");
    let user_id: UserId = sqlx::query_scalar("SELECT id FROM users WHERE public_id = $1")
        .bind(user_public_id)
        .fetch_one(transaction.as_mut())
        .await
        .expect("resolve the internal id for the fixture");

    let live = token_hash(8);
    create_user_session(
        transaction.as_mut(),
        user_id,
        &live,
        Duration::from_secs(3600),
    )
    .await
    .expect("create a session");
    let resolved = find_active_user_session(transaction.as_mut(), &live)
        .await
        .expect("resolve the session")
        .expect("a live session resolves");
    assert_eq!(resolved.user_public_id, user_public_id);
    assert_eq!(resolved.user_id, user_id);

    assert!(
        find_active_user_session(transaction.as_mut(), &token_hash(9))
            .await
            .expect("resolve an unknown token")
            .is_none(),
        "an unknown token must not resolve"
    );

    assert!(
        revoke_user_session(transaction.as_mut(), &live)
            .await
            .expect("revoke"),
        "revoking a live session reports true"
    );
    assert!(
        !revoke_user_session(transaction.as_mut(), &live)
            .await
            .expect("revoke again"),
        "revoking twice is idempotent and reports false"
    );
    assert!(
        find_active_user_session(transaction.as_mut(), &live)
            .await
            .expect("resolve a revoked session")
            .is_none(),
        "a revoked session must not resolve"
    );

    // Expiry, forced deterministically rather than by sleeping.
    let stale = token_hash(10);
    create_user_session(
        transaction.as_mut(),
        user_id,
        &stale,
        Duration::from_secs(3600),
    )
    .await
    .expect("create a session to expire");
    // Both timestamps move back together: the table's
    // CHECK (expires_at > created_at) holds regardless of age, so an
    // expired session can only be fabricated as a consistent one.
    sqlx::query(
        "UPDATE user_sessions \
            SET created_at = clock_timestamp() - interval '2 hours', \
                expires_at = clock_timestamp() - interval '1 hour' \
          WHERE session_token_hash = $1",
    )
    .bind(&stale)
    .execute(transaction.as_mut())
    .await
    .expect("force expiry");
    assert!(
        find_active_user_session(transaction.as_mut(), &stale)
            .await
            .expect("resolve an expired session")
            .is_none(),
        "an expired session must not resolve"
    );

    // A disabled user's still-unexpired session stops resolving too.
    let disabled_session = token_hash(11);
    create_user_session(
        transaction.as_mut(),
        user_id,
        &disabled_session,
        Duration::from_secs(3600),
    )
    .await
    .expect("create a session before disabling");
    sqlx::query("UPDATE users SET disabled_at = clock_timestamp() WHERE id = $1")
        .bind(user_id)
        .execute(transaction.as_mut())
        .await
        .expect("disable the user");
    assert!(
        find_active_user_session(transaction.as_mut(), &disabled_session)
            .await
            .expect("resolve a disabled user's session")
            .is_none(),
        "disabling a user must invalidate their live sessions"
    );
    assert!(
        find_account_by_public_id(transaction.as_mut(), user_public_id)
            .await
            .expect("query a disabled account")
            .is_none(),
        "a disabled account is not readable through the account projection"
    );

    transaction.rollback().await.expect("rollback fixture");
}

#[tokio::test]
#[ignore = "requires an isolated PostgreSQL database in TEST_DATABASE_URL"]
async fn logout_all_revokes_every_live_session_of_one_user_only() {
    let database = test_database().await;
    let mut transaction = isolated_transaction(&database).await;
    let first_invitation = token_hash(12);
    let second_invitation = token_hash(13);
    insert_invitation(&mut transaction, &first_invitation, true).await;
    insert_invitation(&mut transaction, &second_invitation, true).await;

    let target_public_id = register_invited_user(
        transaction.as_mut(),
        &first_invitation,
        &unique_login("target"),
        "hash",
    )
    .await
    .expect("register the target user");
    let other_public_id = register_invited_user(
        transaction.as_mut(),
        &second_invitation,
        &unique_login("other"),
        "hash",
    )
    .await
    .expect("register the other user");
    let target_id: UserId = sqlx::query_scalar("SELECT id FROM users WHERE public_id = $1")
        .bind(target_public_id)
        .fetch_one(transaction.as_mut())
        .await
        .expect("resolve target id");
    let other_id: UserId = sqlx::query_scalar("SELECT id FROM users WHERE public_id = $1")
        .bind(other_public_id)
        .fetch_one(transaction.as_mut())
        .await
        .expect("resolve other id");

    let target_sessions = [token_hash(14), token_hash(15), token_hash(16)];
    for session in &target_sessions {
        create_user_session(
            transaction.as_mut(),
            target_id,
            session,
            Duration::from_secs(3600),
        )
        .await
        .expect("create a target session");
    }
    let other_session = token_hash(17);
    create_user_session(
        transaction.as_mut(),
        other_id,
        &other_session,
        Duration::from_secs(3600),
    )
    .await
    .expect("create the other user's session");

    let revoked = revoke_all_user_sessions(transaction.as_mut(), target_id)
        .await
        .expect("logout everywhere");
    assert_eq!(revoked, 3);
    assert_eq!(
        revoke_all_user_sessions(transaction.as_mut(), target_id)
            .await
            .expect("logout everywhere again"),
        0,
        "a second logout-all has nothing left to revoke"
    );

    for session in &target_sessions {
        assert!(
            find_active_user_session(transaction.as_mut(), session)
                .await
                .expect("resolve a revoked session")
                .is_none()
        );
    }
    assert!(
        find_active_user_session(transaction.as_mut(), &other_session)
            .await
            .expect("resolve the other user's session")
            .is_some(),
        "logout-all must not touch another user's sessions"
    );

    transaction.rollback().await.expect("rollback fixture");
}
