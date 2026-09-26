use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use application::auth::AuthConfig;
use application::quote_signing::QuoteSigner;
use application::seed_protection::SeedProtector;
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
    seed_protector: Option<Arc<dyn SeedProtector>>,
    quote_signer: Option<Arc<dyn QuoteSigner>>,
    auth_rate_limiter: RateLimiter,
    draining: Arc<AtomicBool>,
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
            seed_protector: None,
            quote_signer: None,
            auth_rate_limiter: RateLimiter::new(RateLimitConfig::default()),
            draining: Arc::new(AtomicBool::new(false)),
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

    /// A handle that makes this process report itself unready.
    ///
    /// Shared with every clone of this state, so the shutdown path can set
    /// it once and every router sees it.
    #[must_use]
    pub fn draining_handle(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.draining)
    }

    /// Whether this process has begun shutting down.
    ///
    /// `Acquire`/`Release` rather than `Relaxed`: the flag is a signal
    /// between the shutdown task and every request handler, and a handler
    /// that saw a stale `false` would keep the load balancer sending
    /// traffic for exactly as long as the staleness lasted.
    pub fn is_draining(&self) -> bool {
        self.draining.load(Ordering::Acquire)
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

    #[must_use]
    pub fn with_quote_signer(mut self, signer: Arc<dyn QuoteSigner>) -> Self {
        self.quote_signer = Some(signer);
        self
    }

    pub fn quote_signer(&self) -> Option<&Arc<dyn QuoteSigner>> {
        self.quote_signer.as_ref()
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

#[cfg(test)]
mod tests {
    use super::AppState;
    use application::{
        auth::AuthConfig,
        quote_signing::{EnvironmentQuoteSigner, QuoteSigner},
    };
    use db::{Database, DatabaseConfig};
    use std::{sync::Arc, time::Duration};

    #[tokio::test]
    async fn quote_signer_is_available_only_after_explicit_secure_wiring() {
        let database = Database::connect_lazy(
            &DatabaseConfig::new("postgres://user:pass@127.0.0.1:1/unreachable")
                .unwrap()
                .with_acquire_timeout(Duration::from_millis(1)),
        );
        let state = AppState::new(database, AuthConfig::default());
        assert!(state.quote_signer().is_none());

        let signer =
            EnvironmentQuoteSigner::from_base64url("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA")
                .unwrap();
        let expected_key = signer.public_key();
        let state = state.with_quote_signer(Arc::new(signer));
        assert_eq!(state.quote_signer().unwrap().public_key(), expected_key);
    }
}
