use std::{fmt, str::FromStr, time::Duration};

use sqlx::postgres::PgConnectOptions;
use thiserror::Error;

#[derive(Clone, PartialEq, Eq)]
pub struct DatabaseConfig {
    url: String,
    min_connections: u32,
    max_connections: u32,
    acquire_timeout: Duration,
}

impl DatabaseConfig {
    pub fn new(url: impl Into<String>) -> Result<Self, DatabaseConfigError> {
        let url = url.into();
        PgConnectOptions::from_str(&url)
            .map_err(|_| DatabaseConfigError::InvalidUrl("invalid PostgreSQL URL"))?;

        Ok(Self {
            url,
            min_connections: 0,
            max_connections: 10,
            acquire_timeout: Duration::from_secs(10),
        })
    }

    pub fn with_pool_limits(
        mut self,
        min_connections: u32,
        max_connections: u32,
    ) -> Result<Self, DatabaseConfigError> {
        if max_connections == 0 || min_connections > max_connections {
            return Err(DatabaseConfigError::InvalidPoolLimits);
        }
        self.min_connections = min_connections;
        self.max_connections = max_connections;
        Ok(self)
    }

    pub fn with_acquire_timeout(mut self, timeout: Duration) -> Self {
        self.acquire_timeout = timeout;
        self
    }

    pub(crate) fn connect_options(&self) -> PgConnectOptions {
        PgConnectOptions::from_str(&self.url).expect("URL was validated by DatabaseConfig::new")
    }

    pub(crate) const fn min_connections(&self) -> u32 {
        self.min_connections
    }

    pub(crate) const fn max_connections(&self) -> u32 {
        self.max_connections
    }

    pub(crate) const fn acquire_timeout(&self) -> Duration {
        self.acquire_timeout
    }
}

impl fmt::Debug for DatabaseConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DatabaseConfig")
            .field("url", &"[REDACTED]")
            .field("min_connections", &self.min_connections)
            .field("max_connections", &self.max_connections)
            .field("acquire_timeout", &self.acquire_timeout)
            .finish()
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum DatabaseConfigError {
    #[error("{0}")]
    InvalidUrl(&'static str),
    #[error("pool limits require max_connections > 0 and min_connections <= max_connections")]
    InvalidPoolLimits,
}
