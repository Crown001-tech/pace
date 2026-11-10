use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use pace_db::sqlx;
use serde::Serialize;

use crate::app::AppState;

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub version: &'static str,
}

#[derive(Debug, Serialize)]
pub struct ReadinessResponse {
    pub status: &'static str,
    pub database: &'static str,
}

/// GET /healthz
/// Liveness probe: returns 200 OK if the API server process is responding.
pub async fn healthz() -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(HealthResponse {
            status: "ok",
            version: env!("CARGO_PKG_VERSION"),
        }),
    )
}

/// GET /readyz
/// Readiness probe: returns 200 OK if the database connection pool is healthy.
/// Returns 503 Service Unavailable if the database is unreachable.
pub async fn readyz(State(state): State<AppState>) -> impl IntoResponse {
    match sqlx::query("SELECT 1").execute(&state.db).await {
        Ok(_) => (
            StatusCode::OK,
            Json(ReadinessResponse {
                status: "ok",
                database: "connected",
            }),
        ),
        Err(err) => {
            tracing::warn!(error = %err, "Database readiness probe failed");
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ReadinessResponse {
                    status: "degraded",
                    database: "unreachable",
                }),
            )
        }
    }
}
