use axum::{extract::Request, middleware::Next, response::Response};
use lazy_static::lazy_static;
use prometheus::{register_counter, register_histogram, Counter, Histogram};
use std::time::Instant;

lazy_static! {
    static ref HTTP_REQUESTS_TOTAL: Counter =
        register_counter!("http_requests_total", "Total number of HTTP requests")
            .expect("Can't create metric");
    static ref HTTP_REQUEST_DURATION: Histogram = register_histogram!(
        "http_request_duration_seconds",
        "HTTP request duration in seconds"
    )
    .expect("Can't create metric");
    static ref HTTP_REQUESTS_BY_STATUS: Counter = register_counter!(
        "http_requests_by_status_total",
        "Total number of HTTP requests by status code"
    )
    .expect("Can't create metric");
}

pub async fn track_metrics(req: Request, next: Next) -> Response {
    let start = Instant::now();

    HTTP_REQUESTS_TOTAL.inc();

    let method = req.method().clone();
    let uri = req.uri().clone();

    let res = next.run(req).await;

    let duration = start.elapsed().as_secs_f64();
    HTTP_REQUEST_DURATION.observe(duration);

    let status = res.status();
    if status.is_success() {
        HTTP_REQUESTS_BY_STATUS.inc();
    }

    // Log request details
    tracing::info!(
        method = %method,
        uri = %uri,
        status = %status,
        duration_ms = duration * 1000.0,
        "Request completed"
    );

    res
}
