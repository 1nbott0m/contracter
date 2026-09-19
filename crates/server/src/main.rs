mod config;
mod shutdown;

use api::{AppState, build_router};
use application::auth::AuthConfig;
use config::ServerConfig;
use db::{Database, DatabaseConfig};

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

    let database_config = DatabaseConfig::new(config.database_url())?;
    let database = Database::connect(&database_config).await?;
    // Required startup check: prove the pool can actually reach
    // PostgreSQL before this process claims to be able to serve traffic,
    // rather than discovering that only when the first request arrives.
    database.health_check().await?;

    let mut state = AppState::new(database, AuthConfig::default());
    if config.insecure_cookies() {
        tracing::warn!(
            "INSECURE_COOKIES is set: session cookies omit the Secure attribute,              which is only safe for local HTTP development"
        );
        state = state.with_insecure_cookies();
    }
    let router = build_router(state, &config.router_config());

    let listener = tokio::net::TcpListener::bind(config.bind_addr()).await?;
    tracing::info!(addr = %config.bind_addr(), "contracter-server listening");

    axum::serve(
        listener,
        // Carries the transport peer address into handlers, which the
        // rate limiter keys on. Without this every caller shares one
        // bucket and the limiter cannot tell them apart.
        router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
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
