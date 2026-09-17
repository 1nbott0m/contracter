use application::auth::AuthConfig;
use db::Database;

/// Shared, cheaply-cloneable application state. `Database` wraps an
/// `sqlx::PgPool`, which is itself `Arc`-backed, so cloning `AppState`
/// never opens a new pool or duplicates connections.
#[derive(Clone)]
pub struct AppState {
    database: Database,
    auth_config: AuthConfig,
    secure_cookies: bool,
}

impl AppState {
    /// Session cookies are marked `Secure` by default. `with_insecure_cookies`
    /// is the only way to turn that off, so plain-HTTP local development
    /// has to be an explicit choice rather than a silent default.
    pub const fn new(database: Database, auth_config: AuthConfig) -> Self {
        Self {
            database,
            auth_config,
            secure_cookies: true,
        }
    }

    #[must_use]
    pub const fn with_insecure_cookies(mut self) -> Self {
        self.secure_cookies = false;
        self
    }

    pub const fn database(&self) -> &Database {
        &self.database
    }

    pub const fn auth_config(&self) -> AuthConfig {
        self.auth_config
    }

    pub const fn secure_cookies(&self) -> bool {
        self.secure_cookies
    }
}
