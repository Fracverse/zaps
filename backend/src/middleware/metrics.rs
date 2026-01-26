use axum::{extract::Request, middleware::Next, response::Response};
use std::time::Instant;

use crate::service::MetricsService;

/// Middleware that tracks HTTP request metrics using the MetricsService.
/// 
/// This middleware:
/// - Records request count, duration, and error rates
/// - Tracks active connections
/// - Logs structured request details
pub async fn track_metrics(req: Request, next: Next) -> Response {
    let start = Instant::now();

    // Track connection opened
    MetricsService::connection_opened();

    let method = req.method().to_string();
    let path = req.uri().path().to_string();

    // Execute the request
    let res = next.run(req).await;

    let duration = start.elapsed().as_secs_f64();
    let status = res.status().as_u16();

    // Record metrics using the MetricsService
    MetricsService::record_request(&method, &path, status, duration);

    // Track connection closed
    MetricsService::connection_closed();

    // Structured logging with all relevant fields
    tracing::info!(
        http.method = %method,
        http.path = %path,
        http.status_code = %status,
        http.duration_ms = format!("{:.3}", duration * 1000.0),
        http.is_error = status >= 400,
        "Request completed"
    );

    res
}
