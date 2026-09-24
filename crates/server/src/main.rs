mod config;
mod shutdown;

use api::{AppState, RouterConfig, build_router};
use application::auth::AuthConfig;
use application::quote_signing::QuoteSigner;
use axum::{Router, routing::get};
use config::ServerConfig;
use db::{Database, DatabaseConfig};
use std::sync::Arc;

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("fatal startup error: {error}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), StartupError> {
    let config = ServerConfig::from_env()?;
    init_tracing(config.log_filter());
    tracing::info!(public_key = ?config.quote_signer().public_key(), "quote signer configured");

    let database_config = DatabaseConfig::new(config.database_url())?;
    let database = Database::connect(&database_config).await?;
    // Required startup check: prove the pool can actually reach
    // PostgreSQL before this process claims to be able to serve traffic,
    // rather than discovering that only when the first request arrives.
    database.health_check().await?;

    let mut state = AppState::new(database, AuthConfig::default())
        .with_seed_protector(Arc::new(config.seed_protector().clone()))
        .with_quote_signer(Arc::new(config.quote_signer().clone()));
    if config.insecure_cookies() {
        tracing::warn!(
            "INSECURE_COOKIES is set: session cookies omit the Secure attribute,              which is only safe for local HTTP development"
        );
        state = state.with_insecure_cookies();
    }
    let router = build_router(
        state,
        &RouterConfig {
            rate_limit_burst: Some(config.rate_limit_burst()),
            trusted_proxies: config.trusted_proxy_cidrs().clone(),
            ..RouterConfig::default()
        },
    );

    let listener = tokio::net::TcpListener::bind(config.bind_addr()).await?;
    let metrics_listener = tokio::net::TcpListener::bind(config.metrics_addr()).await?;
    tokio::spawn(async move {
        let metrics = Router::new().route("/metrics", get(|| async {
            "# HELP contracter_up Process liveness\n# TYPE contracter_up gauge\ncontracter_up 1\n"
        }));
        if let Err(error) = axum::serve(metrics_listener, metrics).await {
            tracing::error!(%error, "metrics listener stopped");
        }
    });
    tracing::info!(addr = %config.bind_addr(), "contracter-server listening");

    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown::shutdown_signal())
        .await?;

    Ok(())
}

fn init_tracing(filter: &str) {
    use tracing_subscriber::{EnvFilter, fmt, prelude::*};

    let env_filter = EnvFilter::try_new(filter).unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt::layer())
        .init();
}

#[derive(Debug, thiserror::Error)]
enum StartupError {
    #[error(transparent)]
    Config(#[from] config::ConfigError),
    #[error(transparent)]
    DatabaseConfig(#[from] db::DatabaseConfigError),
    #[error(transparent)]
    Database(#[from] db::DatabaseError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
