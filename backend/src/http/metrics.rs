use axum::{
    body::Body,
    extract::State,
    http::{Request, Response, StatusCode},
    response::IntoResponse,
    Json,
};
use serde::Serialize;
use std::sync::Arc;

use crate::service::{MetricsService, ServiceContainer};

/// Response for the /metrics endpoint (JSON format)
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MetricsResponse {
    pub uptime: u64,
    pub request_count: u64,
    pub error_rate: f64,
    pub active_connections: f64,
    pub db_pool_connections: f64,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Response for Prometheus-compatible metrics
pub struct PrometheusMetrics(pub String);

impl IntoResponse for PrometheusMetrics {
    fn into_response(self) -> Response<Body> {
        Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "text/plain; version=0.0.4; charset=utf-8")
            .body(Body::from(self.0))
            .unwrap()
    }
}

/// GET /metrics - Prometheus-compatible metrics endpoint
/// 
/// Returns metrics in Prometheus text exposition format.
/// This endpoint can be scraped by Prometheus or other compatible monitoring systems.
pub async fn prometheus_metrics() -> impl IntoResponse {
    match MetricsService::export_prometheus() {
        Ok(metrics) => PrometheusMetrics(metrics).into_response(),
        Err(e) => {
            tracing::error!(error = %e, "Failed to export Prometheus metrics");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to export metrics",
            )
                .into_response()
        }
    }
}

/// GET /metrics/json - JSON metrics endpoint
/// 
/// Returns the metrics payload as specified in the issue:
/// - uptime: number (seconds)
/// - requestCount: number
/// - errorRate: number (percentage)
pub async fn json_metrics(
    State(services): State<Arc<ServiceContainer>>,
) -> Json<MetricsResponse> {
    // Update database pool metrics
    let db_pool_size = services.db_pool.status().size as usize;
    MetricsService::update_db_pool_metrics(db_pool_size);

    let detailed = MetricsService::get_detailed_metrics();
    
    Json(MetricsResponse {
        uptime: detailed.basic.uptime,
        request_count: detailed.basic.request_count,
        error_rate: detailed.basic.error_rate,
        active_connections: detailed.active_connections,
        db_pool_connections: detailed.db_pool_connections,
        timestamp: detailed.timestamp,
    })
}

/// GET /metrics/alerts - Check current alert status
/// 
/// Returns any active alerts based on configured thresholds.
/// This is a placeholder for future alerting integration.
pub async fn check_alerts() -> Json<Vec<crate::service::AlertPayload>> {
    let metrics_service = MetricsService::new();
    let alerts = metrics_service.check_alerts();
    Json(alerts)
}
