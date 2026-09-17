use db::{Database, DatabaseError};

/// The one readiness use-case BACKEND-01 needs: is the database reachable
/// right now. Thin on purpose -- it exists so the API layer orchestrates
/// through this layer rather than calling `db` directly for anything
/// beyond holding the shared connection pool in application state.
pub async fn check_readiness(database: &Database) -> Result<(), DatabaseError> {
    database.health_check().await
}
