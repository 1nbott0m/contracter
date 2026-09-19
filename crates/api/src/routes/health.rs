use axum::{Json, extract::State, response::IntoResponse};
use serde::Serialize;

use crate::{error::ApiError, state::AppState};

#[derive(Debug, Serialize)]
struct StatusBody {
    status: &'static str,
}

/// `GET /health/live` -- process is up and can accept traffic. Never
/// touches the database: a stalled or unreachable PostgreSQL must not
/// take liveness down with it (that would make an orchestrator kill and
/// restart a process that a DB outage alone should not affect).
pub async fn live() -> impl IntoResponse {
    Json(StatusBody { status: "live" })
}

/// `GET /health/ready` -- process can actually serve DB-backed requests
/// right now. Reflects real connectivity via `application::check_readiness`.
/// The real cause is logged via `tracing` (its `Display`, matching
/// `db::DatabaseError`'s own sanitized messages -- never the connection
/// string) so an outage is diagnosable from logs; the HTTP response itself
/// never carries a raw SQLx error or connection string.
pub async fn ready(State(state): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    // Draining is checked before connectivity, and is the whole point of
    // this branch.
    //
    // `with_graceful_shutdown` stops accepting connections the moment the
    // signal arrives. Without this, readiness kept answering 200 for as
    // long as PostgreSQL answered, so during every rolling deploy the load
    // balancer went on routing to a process that had stopped accepting --
    // and callers saw connection refused until the next readiness probe.
    // Reporting unready first gives the balancer time to take this
    // instance out before the socket closes.
    if state.is_draining() {
        return Err(ApiError::service_unavailable(
            "Service temporarily unavailable",
        ));
    }

    application::check_readiness(state.database())
        .await
        .map_err(|error| {
            tracing::warn!(error = %error, "readiness check failed");
            ApiError::service_unavailable("Service temporarily unavailable")
        })?;
    Ok(Json(StatusBody { status: "ready" }))
}
