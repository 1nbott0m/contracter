use db::Database;

/// Shared, cheaply-cloneable application state. `Database` wraps an
/// `sqlx::PgPool`, which is itself `Arc`-backed, so cloning `AppState`
/// never opens a new pool or duplicates connections.
#[derive(Clone)]
pub struct AppState {
    database: Database,
}

impl AppState {
    pub const fn new(database: Database) -> Self {
        Self { database }
    }

    pub const fn database(&self) -> &Database {
        &self.database
    }
}
