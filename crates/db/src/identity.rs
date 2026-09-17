use sqlx::{
    Executor, Postgres,
    types::chrono::{DateTime, Utc},
};

use crate::{DatabaseError, PublicId, UserId};

/// One login's stored credential, as returned by the narrow
/// `find_user_credential_by_login` function. `password_hash` is a PHC
/// string produced and verified outside this crate (Argon2id, in the
/// application layer) -- `db` never interprets it, and no caller should
/// log or return it.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct UserCredential {
    pub user_id: UserId,
    pub user_public_id: PublicId,
    pub password_hash: String,
    pub disabled_at: Option<DateTime<Utc>>,
}

/// A session token that resolved to a live, non-revoked session owned by
/// an enabled user.
#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::FromRow)]
pub struct ActiveSession {
    pub session_public_id: PublicId,
    pub user_id: UserId,
    pub user_public_id: PublicId,
    pub expires_at: DateTime<Utc>,
}

/// The account projection behind `/me`: public identity only.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct Account {
    pub user_public_id: PublicId,
    pub login: String,
    pub created_at: DateTime<Utc>,
}

/// Looks up one login's credential for the login flow. Case-insensitive,
/// matching the `users_login_key` unique index on `lower(login)`. Returns
/// `None` for an unknown login -- callers must still run a password
/// verification against a dummy hash in that case, so a missing user and
/// a wrong password cost the same observable time.
pub async fn find_user_credential_by_login<'e, E>(
    executor: E,
    login: &str,
) -> Result<Option<UserCredential>, DatabaseError>
where
    E: Executor<'e, Database = Postgres>,
{
    Ok(sqlx::query_as(
        "SELECT user_id, user_public_id, password_hash, disabled_at \
         FROM find_user_credential_by_login($1)",
    )
    .bind(login)
    .fetch_optional(executor)
    .await?)
}

/// Redeems an invitation and creates its user in one atomic statement,
/// returning the new user's public id. The caller supplies an
/// already-derived password hash; no plaintext ever reaches the database.
pub async fn register_invited_user<'e, E>(
    executor: E,
    invitation_token_hash: &[u8],
    login: &str,
    password_hash: &str,
) -> Result<PublicId, DatabaseError>
where
    E: Executor<'e, Database = Postgres>,
{
    Ok(
        sqlx::query_scalar("SELECT register_invited_user($1, $2, $3)")
            .bind(invitation_token_hash)
            .bind(login)
            .bind(password_hash)
            .fetch_one(executor)
            .await?,
    )
}

/// Creates a session for an enabled user, returning the session's public
/// id. Only the token's hash is stored; the raw token stays with the
/// caller.
pub async fn create_user_session<'e, E>(
    executor: E,
    user_id: UserId,
    session_token_hash: &[u8],
    ttl: std::time::Duration,
) -> Result<PublicId, DatabaseError>
where
    E: Executor<'e, Database = Postgres>,
{
    Ok(
        sqlx::query_scalar("SELECT create_user_session($1, $2, make_interval(secs => $3))")
            .bind(user_id)
            .bind(session_token_hash)
            .bind(ttl.as_secs_f64())
            .fetch_one(executor)
            .await?,
    )
}

/// Resolves a presented session token hash to its owner, or `None` if the
/// token is unknown, expired, revoked, or its user has been disabled --
/// deliberately indistinguishable cases.
pub async fn find_active_user_session<'e, E>(
    executor: E,
    session_token_hash: &[u8],
) -> Result<Option<ActiveSession>, DatabaseError>
where
    E: Executor<'e, Database = Postgres>,
{
    Ok(sqlx::query_as(
        "SELECT session_public_id, user_id, user_public_id, expires_at \
         FROM find_active_user_session($1)",
    )
    .bind(session_token_hash)
    .fetch_optional(executor)
    .await?)
}

/// Revokes one session. Idempotent: `false` means there was no live
/// session for that token, which is not an error.
pub async fn revoke_user_session<'e, E>(
    executor: E,
    session_token_hash: &[u8],
) -> Result<bool, DatabaseError>
where
    E: Executor<'e, Database = Postgres>,
{
    Ok(sqlx::query_scalar("SELECT revoke_user_session($1)")
        .bind(session_token_hash)
        .fetch_one(executor)
        .await?)
}

/// Revokes every live session for one user, returning how many were
/// revoked.
pub async fn revoke_all_user_sessions<'e, E>(
    executor: E,
    user_id: UserId,
) -> Result<i32, DatabaseError>
where
    E: Executor<'e, Database = Postgres>,
{
    Ok(sqlx::query_scalar("SELECT revoke_all_user_sessions($1)")
        .bind(user_id)
        .fetch_one(executor)
        .await?)
}

/// The `/me` account projection, addressed by public id only.
pub async fn find_account_by_public_id<'e, E>(
    executor: E,
    user_public_id: PublicId,
) -> Result<Option<Account>, DatabaseError>
where
    E: Executor<'e, Database = Postgres>,
{
    Ok(sqlx::query_as(
        "SELECT user_public_id, login, created_at FROM find_account_by_public_id($1)",
    )
    .bind(user_public_id)
    .fetch_optional(executor)
    .await?)
}

/// The caller's own credit balance in integer microcredits, derived from
/// the ledger. Returns `None` for an unknown or disabled user; a user
/// with no ledger account yet reads as `0`.
pub async fn find_user_credit_balance<'e, E>(
    executor: E,
    user_public_id: PublicId,
) -> Result<Option<i64>, DatabaseError>
where
    E: Executor<'e, Database = Postgres>,
{
    Ok(
        sqlx::query_scalar("SELECT balance_microcredits FROM find_user_credit_balance($1)")
            .bind(user_public_id)
            .fetch_optional(executor)
            .await?,
    )
}
