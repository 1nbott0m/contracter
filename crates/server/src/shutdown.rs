/// Resolves on Ctrl+C or (on Unix) SIGTERM, whichever comes first, so
/// `axum::serve(...).with_graceful_shutdown(...)` stops accepting new
/// connections and lets in-flight requests finish before the process
/// exits.
pub async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install the Ctrl+C signal handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install the SIGTERM signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {}
        () = terminate => {}
    }
    tracing::info!("shutdown signal received, draining in-flight requests");
}

/// Reports this process unready, waits, and only then lets the server stop
/// accepting connections.
///
/// A load balancer takes an instance out of rotation when its readiness
/// probe fails, and it only learns that on the next probe. Closing the
/// listening socket the instant the signal arrives means every request
/// routed in the meantime is refused at the transport level, which the
/// balancer reports as an error rather than as a healthy instance going
/// away. The wait is what turns a rolling deploy from "a burst of
/// connection-refused" into "nothing happened".
///
/// `grace` should exceed the balancer's readiness interval. Too short and
/// the wait buys nothing; too long and every deploy is slower by that much,
/// so it is configuration rather than a constant.
pub async fn drain_then_shutdown(
    draining: std::sync::Arc<std::sync::atomic::AtomicBool>,
    grace: std::time::Duration,
) {
    shutdown_signal().await;

    tracing::info!(
        grace_seconds = grace.as_secs(),
        "shutdown signal received; reporting unready before closing the listener"
    );
    draining.store(true, std::sync::atomic::Ordering::Release);

    tokio::time::sleep(grace).await;
    tracing::info!("drain period elapsed; closing the listener");
}
