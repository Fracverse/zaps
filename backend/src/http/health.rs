use axum::{extract::State, Json};
use serde::Serialize;
use std::sync::Arc;

use crate::service::ServiceContainer;

#[derive(Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Serialize)]
pub struct ReadinessResponse {
    pub status: String,
    pub database: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

pub async fn health_check() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "healthy".to_string(),
        timestamp: chrono::Utc::now(),
    })
}

pub async fn readiness_check(
    State(services): State<Arc<ServiceContainer>>,
) -> Json<ReadinessResponse> {
    // Check database connectivity
    let db_status = match services.db_pool.get().await {
        Ok(_) => "connected",
        Err(_) => "disconnected",
    };

    Json(ReadinessResponse {
        status: if db_status == "connected" {
            "ready"
        } else {
            "not ready"
        }
        .to_string(),
        database: db_status.to_string(),
        timestamp: chrono::Utc::now(),
    })
}
