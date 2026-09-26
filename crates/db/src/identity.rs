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

/// Creates a user without an invitation. Registration is intentionally
/// separate from invitation redemption so the legacy, single-use invitation
/// path remains available for deployments that need it.
pub async fn register_public_user<'e, E>(
    executor: E,
    login: &str,
    password_hash: &str,
) -> Result<PublicId, DatabaseError>
where
    E: Executor<'e, Database = Postgres>,
{
    Ok(sqlx::query_scalar("SELECT register_public_user($1, $2)")
        .bind(login)
        .bind(password_hash)
        .fetch_one(executor)
        .await?)
}

/// Returns whether the account has an active administrator membership.
pub async fn is_active_administrator<'e, E>(
    executor: E,
    user_public_id: PublicId,
) -> Result<bool, DatabaseError>
where
    E: Executor<'e, Database = Postgres>,
{
    Ok(sqlx::query_scalar("SELECT is_active_administrator($1)")
        .bind(user_public_id)
        .fetch_one(executor)
        .await?)
}

pub async fn find_user_by_steam_id<'e, E>(
    executor: E,
    steam_id: &str,
) -> Result<Option<(UserId, PublicId)>, DatabaseError>
where
    E: Executor<'e, Database = Postgres>,
{
    Ok(
        sqlx::query_as("SELECT user_id, user_public_id FROM find_user_by_steam_id($1)")
            .bind(steam_id)
            .fetch_optional(executor)
            .await?,
    )
}

pub async fn register_steam_user<'e, E>(
    executor: E,
    login: &str,
    password_hash: &str,
    steam_id: &str,
) -> Result<PublicId, DatabaseError>
where
    E: Executor<'e, Database = Postgres>,
{
    Ok(sqlx::query_scalar("SELECT register_steam_user($1,$2,$3)")
        .bind(login)
        .bind(password_hash)
        .bind(steam_id)
        .fetch_one(executor)
        .await?)
}

pub async fn administrator_totp_secret<'e, E>(
    executor: E,
    user_public_id: PublicId,
) -> Result<Option<Vec<u8>>, DatabaseError>
where
    E: Executor<'e, Database = Postgres>,
{
    Ok(sqlx::query_scalar("SELECT administrator_totp_secret($1)")
        .bind(user_public_id)
        .fetch_one(executor)
        .await?)
}

pub async fn set_administrator_totp_secret<'e, E>(
    executor: E,
    user_public_id: PublicId,
    blob: &[u8],
) -> Result<bool, DatabaseError>
where
    E: Executor<'e, Database = Postgres>,
{
    Ok(
        sqlx::query_scalar("SELECT set_administrator_totp_secret($1, $2)")
            .bind(user_public_id)
            .bind(blob)
            .fetch_one(executor)
            .await?,
    )
}

pub async fn mark_session_totp_verified<'e, E>(
    executor: E,
    session_public_id: PublicId,
) -> Result<bool, DatabaseError>
where
    E: Executor<'e, Database = Postgres>,
{
    Ok(sqlx::query_scalar("SELECT mark_session_totp_verified($1)")
        .bind(session_public_id)
        .fetch_one(executor)
        .await?)
}

pub async fn session_totp_verified<'e, E>(
    executor: E,
    session_public_id: PublicId,
) -> Result<bool, DatabaseError>
where
    E: Executor<'e, Database = Postgres>,
{
    Ok(sqlx::query_scalar("SELECT session_totp_verified($1)")
        .bind(session_public_id)
        .fetch_one(executor)
        .await?)
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AdminDashboardStats {
    pub users: i64,
    pub active_sessions: i64,
    pub contracts: i64,
    pub inventory_items: i64,
    pub market_purchases: i64,
    pub ledger_transactions: i64,
}

pub async fn admin_dashboard_stats<'e, E>(executor: E) -> Result<AdminDashboardStats, DatabaseError>
where
    E: Executor<'e, Database = Postgres>,
{
    Ok(sqlx::query_as("SELECT (SELECT count(*) FROM users) AS users, (SELECT count(*) FROM user_sessions WHERE revoked_at IS NULL AND expires_at > clock_timestamp()) AS active_sessions, (SELECT count(*) FROM contracts) AS contracts, (SELECT count(*) FROM inventory_items) AS inventory_items, (SELECT count(*) FROM market_purchase_events) AS market_purchases, (SELECT count(*) FROM ledger_transactions) AS ledger_transactions").fetch_one(executor).await?)
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AdminUserRow {
    pub user_id: PublicId,
    pub login: String,
    pub created_at: DateTime<Utc>,
    pub disabled: bool,
    pub is_admin: bool,
}

pub async fn admin_users<'e, E>(executor: E) -> Result<Vec<AdminUserRow>, DatabaseError>
where
    E: Executor<'e, Database = Postgres>,
{
    Ok(sqlx::query_as("SELECT u.public_id AS user_id, u.login, u.created_at, (u.disabled_at IS NOT NULL) AS disabled, COALESCE(a.is_active AND a.deactivated_at IS NULL, false) AS is_admin FROM users u LEFT JOIN administrators a ON a.user_id=u.id ORDER BY u.created_at DESC LIMIT 500").fetch_all(executor).await?)
}

pub async fn admin_disable_user<'e, E>(
    executor: E,
    admin_public_id: PublicId,
    target_public_id: PublicId,
) -> Result<bool, DatabaseError>
where
    E: Executor<'e, Database = Postgres>,
{
    Ok(sqlx::query_scalar("SELECT admin_disable_user($1, $2)")
        .bind(admin_public_id)
        .bind(target_public_id)
        .fetch_one(executor)
        .await?)
}

pub async fn admin_totp_attempt_allowed<'e, E>(
    executor: E,
    session_public_id: PublicId,
) -> Result<bool, DatabaseError>
where
    E: Executor<'e, Database = Postgres>,
{
    Ok(sqlx::query_scalar("SELECT admin_totp_attempt_allowed($1)")
        .bind(session_public_id)
        .fetch_one(executor)
        .await?)
}

pub async fn record_admin_totp_failure<'e, E>(
    executor: E,
    session_public_id: PublicId,
) -> Result<bool, DatabaseError>
where
    E: Executor<'e, Database = Postgres>,
{
    Ok(sqlx::query_scalar("SELECT record_admin_totp_failure($1)")
        .bind(session_public_id)
        .fetch_one(executor)
        .await?)
}

pub async fn clear_admin_totp_failures<'e, E>(
    executor: E,
    session_public_id: PublicId,
) -> Result<(), DatabaseError>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query("SELECT clear_admin_totp_failures($1)")
        .bind(session_public_id)
        .execute(executor)
        .await?;
    Ok(())
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AdminAuditRow {
    pub public_id: PublicId,
    pub administrator_public_id: PublicId,
    pub action_code: String,
    pub target_public_id: Option<PublicId>,
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

pub async fn record_admin_audit<'e, E>(
    executor: E,
    admin_public_id: PublicId,
    action: &str,
    target: Option<PublicId>,
    metadata: serde_json::Value,
) -> Result<PublicId, DatabaseError>
where
    E: Executor<'e, Database = Postgres>,
{
    Ok(sqlx::query_scalar("SELECT record_admin_audit($1,$2,$3,$4)")
        .bind(admin_public_id)
        .bind(action)
        .bind(target)
        .bind(metadata)
        .fetch_one(executor)
        .await?)
}

pub async fn list_admin_audit<'e, E>(executor: E) -> Result<Vec<AdminAuditRow>, DatabaseError>
where
    E: Executor<'e, Database = Postgres>,
{
    Ok(sqlx::query_as("SELECT public_id, administrator_public_id, action_code, target_public_id, metadata, created_at FROM list_admin_audit_events()").fetch_all(executor).await?)
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
