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
}
