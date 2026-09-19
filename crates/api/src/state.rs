use application::auth::AuthConfig;
use db::Database;

use crate::{
    rate_limit::{RateLimitConfig, RateLimiter},
    session_cookie::SessionCookiePolicy,
};

/// Shared, cheaply-cloneable application state. `Database` wraps an
/// `sqlx::PgPool`, which is itself `Arc`-backed, so cloning `AppState`
/// never opens a new pool or duplicates connections.
#[derive(Clone)]
pub struct AppState {
    database: Database,
    auth_config: AuthConfig,
    secure_cookies: bool,
    auth_rate_limiter: RateLimiter,
}

impl AppState {
    /// Session cookies are marked `Secure` by default. `with_insecure_cookies`
    /// is the only way to turn that off, so plain-HTTP local development
    /// has to be an explicit choice rather than a silent default.
    pub fn new(database: Database, auth_config: AuthConfig) -> Self {
        Self {
            database,
            auth_config,
            secure_cookies: true,
            auth_rate_limiter: RateLimiter::new(RateLimitConfig::default()),
        }
    }

    /// Replaces the limiter guarding `/auth/*`.
    ///
    /// Exists so a test can make the limit reachable in a handful of
    /// requests instead of the production budget, and so a deployment can
    /// tighten it. The limiter is shared by every clone of this state, so
    /// one process has one budget however many routers are built from it.
    #[must_use]
    pub fn with_auth_rate_limit(mut self, config: RateLimitConfig) -> Self {
        self.auth_rate_limiter = RateLimiter::new(config);
        self
    }

    pub(crate) const fn auth_rate_limiter(&self) -> &RateLimiter {
        &self.auth_rate_limiter
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

    /// The single source of truth for which cookie name this deployment
    /// reads and writes. Derived from the security mode rather than stored
    /// separately, so reading and writing can never drift apart.
    pub const fn session_cookie_policy(&self) -> SessionCookiePolicy {
        SessionCookiePolicy::new(self.secure_cookies)
    }
}
