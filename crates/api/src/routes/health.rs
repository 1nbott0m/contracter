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
/// right now. Reflects real connectivity via `application::check_readiness`,
/// never a raw SQLx error or connection string.
pub async fn ready(State(state): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    application::check_readiness(state.database())
        .await
        .map_err(|_| ApiError::service_unavailable("Service temporarily unavailable"))?;
    Ok(Json(StatusBody { status: "ready" }))
}
