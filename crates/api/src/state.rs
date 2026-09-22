use application::auth::AuthConfig;
use application::seed_protection::SeedProtector;
use db::Database;
use std::sync::Arc;

use crate::session_cookie::SessionCookiePolicy;

/// Shared, cheaply-cloneable application state. `Database` wraps an
/// `sqlx::PgPool`, which is itself `Arc`-backed, so cloning `AppState`
/// never opens a new pool or duplicates connections.
#[derive(Clone)]
pub struct AppState {
    database: Database,
    auth_config: AuthConfig,
    secure_cookies: bool,
    seed_protector: Option<Arc<dyn SeedProtector>>,
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
            seed_protector: None,
        }
    }

    #[must_use]
    pub const fn with_insecure_cookies(mut self) -> Self {
        self.secure_cookies = false;
        self
    }

    #[must_use]
    pub fn with_seed_protector(mut self, protector: Arc<dyn SeedProtector>) -> Self {
        self.seed_protector = Some(protector);
        self
    }

    pub fn seed_protector(&self) -> Option<&Arc<dyn SeedProtector>> {
        self.seed_protector.as_ref()
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

    /// The single source of truth for which cookie name this deployment
    /// reads and writes. Derived from the security mode rather than stored
    /// separately, so reading and writing can never drift apart.
    pub const fn session_cookie_policy(&self) -> SessionCookiePolicy {
        SessionCookiePolicy::new(self.secure_cookies)
    }
}
