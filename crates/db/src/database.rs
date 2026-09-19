use std::borrow::Cow;

use sqlx::{
    PgPool, Postgres, Transaction,
    migrate::{MigrateError, Migrator},
    postgres::PgPoolOptions,
};
use thiserror::Error;

use crate::DatabaseConfig;

pub static MIGRATOR: Migrator = sqlx::migrate!("../../db/migrations");

#[derive(Clone, Debug)]
pub struct Database {
    pool: PgPool,
}

impl Database {
    pub async fn connect(config: &DatabaseConfig) -> Result<Self, DatabaseError> {
        let pool = PgPoolOptions::new()
            .min_connections(config.min_connections())
            .max_connections(config.max_connections())
            .acquire_timeout(config.acquire_timeout())
            .connect_with(config.connect_options())
            .await?;
        Ok(Self { pool })
    }

    /// Builds a pool without validating connectivity -- unlike `connect`,
    /// this never touches the network and cannot fail. The first real
    /// query still goes through `acquire_timeout`/normal error handling
    /// exactly as it would for a pool built with `connect`; only the
    /// eager startup probe is skipped. Intended for callers that need to
    /// construct a `Database` representing "not yet known to be
    /// reachable" deterministically (for example, proving a health-check
    /// consumer correctly reports a database outage without depending on
    /// how fast a real unreachable host happens to fail in a given
    /// environment).
    pub fn connect_lazy(config: &DatabaseConfig) -> Self {
        let pool = PgPoolOptions::new()
            .min_connections(config.min_connections())
            .max_connections(config.max_connections())
            .acquire_timeout(config.acquire_timeout())
            .connect_lazy_with(config.connect_options());
        Self { pool }
    }

    pub const fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub async fn migrate(&self) -> Result<(), DatabaseError> {
        MIGRATOR.run(&self.pool).await?;
        Ok(())
    }

    pub async fn health_check(&self) -> Result<(), DatabaseError> {
        sqlx::query("SELECT 1").execute(&self.pool).await?;
        Ok(())
    }

    pub async fn begin(&self) -> Result<Transaction<'_, Postgres>, DatabaseError> {
        Ok(self.pool.begin().await?)
    }
}

#[derive(Debug, Error)]
pub enum DatabaseError {
    #[error("PostgreSQL operation failed: {0}")]
    Sqlx(#[from] sqlx::Error),
    #[error("PostgreSQL migration failed: {0}")]
    Migration(#[from] MigrateError),
}

impl DatabaseError {
    pub fn database_code(&self) -> Option<Cow<'_, str>> {
        match self {
            Self::Sqlx(sqlx::Error::Database(error)) => error.code(),
            Self::Sqlx(_) | Self::Migration(_) => None,
        }
    }

    /// The violated constraint's name, when PostgreSQL reported one.
    ///
    /// A SQLSTATE alone is too coarse to act on: one function can raise
    /// 23505 from several different unique constraints, and treating them
    /// alike turns one failure into another's error message. Callers that
    /// map an error to a user-visible outcome should match on this, not
    /// only on the code.
    pub fn constraint(&self) -> Option<&str> {
        match self {
            Self::Sqlx(sqlx::Error::Database(error)) => error.constraint(),
            Self::Sqlx(_) | Self::Migration(_) => None,
        }
    }
}
