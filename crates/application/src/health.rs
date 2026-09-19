use db::Database;
use thiserror::Error;

/// The one readiness use-case BACKEND-01 needs: is the database reachable
/// right now. Thin on purpose -- it exists so the API layer orchestrates
/// through this layer rather than calling `db` directly for anything
/// beyond holding the shared connection pool in application state.
pub async fn check_readiness(database: &Database) -> Result<(), ReadinessError> {
    database.health_check().await.map_err(ReadinessError::from)
}

/// The application layer's own error type for this use-case -- callers in
/// `api` see this, never `db::DatabaseError` directly, so `db`'s error
/// variants (and any future change to them) stay an internal detail of
/// this crate rather than leaking through the orchestration boundary.
/// `Display` stays safe to log (matches `db::DatabaseError`'s own
/// sanitized messages -- never a connection string or credential).
#[derive(Debug, Error)]
#[error("database is not reachable: {0}")]
pub struct ReadinessError(#[from] db::DatabaseError);
