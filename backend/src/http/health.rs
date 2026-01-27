use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use serde::Serialize;
use std::sync::Arc;

use crate::service::{MetricsService, ServiceContainer};

/// Basic health check response
#[derive(Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub version: String,
}

/// Detailed readiness check response
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadinessResponse {
    pub status: String,
    pub database: DatabaseHealth,
    pub uptime_seconds: u64,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Database health information
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseHealth {
    pub status: String,
    pub pool_size: u32,
    pub available_connections: u32,
}

/// Liveness probe response for Kubernetes
#[derive(Serialize)]
pub struct LivenessResponse {
    pub status: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// GET /health/health - Basic health check
///
/// Returns a simple health status indicating the service is running.
/// This endpoint is suitable for basic load balancer health checks.
pub async fn health_check() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "healthy".to_string(),
        timestamp: chrono::Utc::now(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

/// GET /health/ready - Readiness probe
///
/// Returns detailed health status including database connectivity.
/// This endpoint is suitable for Kubernetes readiness probes.
pub async fn readiness_check(State(services): State<Arc<ServiceContainer>>) -> impl IntoResponse {
    // Check database connectivity
    let (db_status, pool_status) = match services.db_pool.get().await {
        Ok(_client) => {
            let status = services.db_pool.status();
            (
                "connected",
                DatabaseHealth {
                    status: "connected".to_string(),
                    pool_size: status.size as u32,
                    available_connections: status.available as u32,
                },
            )
        }
        Err(e) => {
            tracing::error!(error = %e, "Database health check failed");
            (
                "disconnected",
                DatabaseHealth {
                    status: "disconnected".to_string(),
                    pool_size: 0,
                    available_connections: 0,
                },
            )
        }
    };

    let is_ready = db_status == "connected";
    let response = ReadinessResponse {
        status: if is_ready { "ready" } else { "not ready" }.to_string(),
        database: pool_status,
        uptime_seconds: MetricsService::get_uptime(),
        timestamp: chrono::Utc::now(),
    };

    if is_ready {
        (StatusCode::OK, Json(response))
    } else {
        (StatusCode::SERVICE_UNAVAILABLE, Json(response))
    }
}

/// GET /health/live - Liveness probe
///
/// Returns a simple "alive" status. This endpoint should always return 200
/// as long as the process is running. Suitable for Kubernetes liveness probes.
pub async fn liveness_check() -> Json<LivenessResponse> {
    Json(LivenessResponse {
        status: "alive".to_string(),
        timestamp: chrono::Utc::now(),
    })
}
