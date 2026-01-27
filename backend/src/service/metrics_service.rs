use lazy_static::lazy_static;
use prometheus::{
    core::Collector, register_counter_vec, register_gauge, register_histogram_vec, CounterVec,
    Encoder, Gauge, HistogramVec, TextEncoder,
};
use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

lazy_static! {
    /// Total HTTP requests counter with method, path, and status labels
    pub static ref HTTP_REQUESTS_TOTAL: CounterVec = register_counter_vec!(
        "http_requests_total",
        "Total number of HTTP requests",
        &["method", "path", "status"]
    )
    .expect("Can't create http_requests_total metric");

    /// HTTP request duration histogram with method and path labels
    pub static ref HTTP_REQUEST_DURATION_SECONDS: HistogramVec = register_histogram_vec!(
        "http_request_duration_seconds",
        "HTTP request duration in seconds",
        &["method", "path"],
        vec![0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0]
    )
    .expect("Can't create http_request_duration_seconds metric");

    /// HTTP errors counter with method, path, and error type labels
    pub static ref HTTP_ERRORS_TOTAL: CounterVec = register_counter_vec!(
        "http_errors_total",
        "Total number of HTTP errors",
        &["method", "path", "error_type"]
    )
    .expect("Can't create http_errors_total metric");

    /// Active connections gauge
    pub static ref ACTIVE_CONNECTIONS: Gauge = register_gauge!(
        "active_connections",
        "Number of currently active connections"
    )
    .expect("Can't create active_connections metric");

    /// Database pool connections gauge
    pub static ref DB_POOL_CONNECTIONS: Gauge = register_gauge!(
        "db_pool_connections",
        "Number of active database pool connections"
    )
    .expect("Can't create db_pool_connections metric");

    /// Application uptime gauge (set at startup)
    pub static ref APP_UPTIME_SECONDS: Gauge = register_gauge!(
        "app_uptime_seconds",
        "Application uptime in seconds"
    )
    .expect("Can't create app_uptime_seconds metric");

    /// Application start time (Unix timestamp)
    static ref APP_START_TIME: AtomicU64 = AtomicU64::new(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
    );
}

/// Metrics payload as specified in the issue
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MetricsPayload {
    /// Application uptime in seconds
    pub uptime: u64,
    /// Total number of requests since startup
    pub request_count: u64,
    /// Error rate as a percentage (0.0 - 100.0)
    pub error_rate: f64,
}

/// Detailed metrics response
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetailedMetrics {
    /// Basic metrics payload
    #[serde(flatten)]
    pub basic: MetricsPayload,
    /// Active connections
    pub active_connections: f64,
    /// Database pool connections
    pub db_pool_connections: f64,
    /// Timestamp of metrics collection
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Alert severity levels
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AlertSeverity {
    Info,
    Warning,
    Critical,
}

/// Alert hook payload (placeholder for future integration)
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AlertPayload {
    pub severity: AlertSeverity,
    pub title: String,
    pub message: String,
    pub metric_name: String,
    pub current_value: f64,
    pub threshold: f64,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Alert threshold configuration
#[derive(Debug, Clone)]
pub struct AlertThreshold {
    pub metric_name: String,
    pub warning_threshold: f64,
    pub critical_threshold: f64,
}

/// Metrics service for monitoring and alerting
#[derive(Clone)]
pub struct MetricsService {
    alert_thresholds: Vec<AlertThreshold>,
}

impl Default for MetricsService {
    fn default() -> Self {
        Self::new()
    }
}

impl MetricsService {
    pub fn new() -> Self {
        // Initialize default alert thresholds
        let alert_thresholds = vec![
            AlertThreshold {
                metric_name: "error_rate".to_string(),
                warning_threshold: 5.0,   // 5% error rate warning
                critical_threshold: 10.0, // 10% error rate critical
            },
            AlertThreshold {
                metric_name: "request_duration_p99".to_string(),
                warning_threshold: 1.0,  // 1 second warning
                critical_threshold: 5.0, // 5 seconds critical
            },
        ];

        Self { alert_thresholds }
    }

    /// Initialize metrics (call at application startup)
    pub fn init() {
        // Force lazy_static initialization
        let _ = &*HTTP_REQUESTS_TOTAL;
        let _ = &*HTTP_REQUEST_DURATION_SECONDS;
        let _ = &*HTTP_ERRORS_TOTAL;
        let _ = &*ACTIVE_CONNECTIONS;
        let _ = &*DB_POOL_CONNECTIONS;
        let _ = &*APP_UPTIME_SECONDS;

        tracing::info!("Metrics service initialized");
    }

    /// Get application uptime in seconds
    pub fn get_uptime() -> u64 {
        let start_time = APP_START_TIME.load(Ordering::Relaxed);
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        now.saturating_sub(start_time)
    }

    /// Get total request count from Prometheus metrics
    pub fn get_request_count() -> u64 {
        use prometheus::proto::MetricFamily;

        let metric_families: Vec<MetricFamily> = HTTP_REQUESTS_TOTAL.collect();
        metric_families
            .first()
            .map(|mf| {
                mf.get_metric()
                    .iter()
                    .map(|m| m.get_counter().get_value() as u64)
                    .sum()
            })
            .unwrap_or(0)
    }

    /// Get total error count from Prometheus metrics
    pub fn get_error_count() -> u64 {
        use prometheus::proto::MetricFamily;

        let metric_families: Vec<MetricFamily> = HTTP_ERRORS_TOTAL.collect();
        metric_families
            .first()
            .map(|mf| {
                mf.get_metric()
                    .iter()
                    .map(|m| m.get_counter().get_value() as u64)
                    .sum()
            })
            .unwrap_or(0)
    }

    /// Calculate error rate as a percentage
    pub fn get_error_rate() -> f64 {
        let total_requests = Self::get_request_count();
        let total_errors = Self::get_error_count();

        if total_requests == 0 {
            return 0.0;
        }

        (total_errors as f64 / total_requests as f64) * 100.0
    }

    /// Get basic metrics payload
    pub fn get_metrics_payload() -> MetricsPayload {
        // Update uptime gauge
        APP_UPTIME_SECONDS.set(Self::get_uptime() as f64);

        MetricsPayload {
            uptime: Self::get_uptime(),
            request_count: Self::get_request_count(),
            error_rate: Self::get_error_rate(),
        }
    }

    /// Get detailed metrics
    pub fn get_detailed_metrics() -> DetailedMetrics {
        DetailedMetrics {
            basic: Self::get_metrics_payload(),
            active_connections: ACTIVE_CONNECTIONS.get(),
            db_pool_connections: DB_POOL_CONNECTIONS.get(),
            timestamp: chrono::Utc::now(),
        }
    }

    /// Export metrics in Prometheus text format
    pub fn export_prometheus() -> Result<String, prometheus::Error> {
        // Update uptime before export
        APP_UPTIME_SECONDS.set(Self::get_uptime() as f64);

        let encoder = TextEncoder::new();
        let metric_families = prometheus::gather();
        let mut buffer = Vec::new();
        encoder.encode(&metric_families, &mut buffer)?;
        String::from_utf8(buffer).map_err(|e| {
            prometheus::Error::Msg(format!("Failed to encode metrics as UTF-8: {}", e))
        })
    }

    /// Record a request metric
    pub fn record_request(method: &str, path: &str, status: u16, duration_secs: f64) {
        let status_str = status.to_string();
        let normalized_path = Self::normalize_path(path);

        HTTP_REQUESTS_TOTAL
            .with_label_values(&[method, &normalized_path, &status_str])
            .inc();

        HTTP_REQUEST_DURATION_SECONDS
            .with_label_values(&[method, &normalized_path])
            .observe(duration_secs);

        // Track errors (4xx and 5xx responses)
        if status >= 400 {
            let error_type = if status >= 500 {
                "server_error"
            } else {
                "client_error"
            };
            HTTP_ERRORS_TOTAL
                .with_label_values(&[method, &normalized_path, error_type])
                .inc();
        }
    }

    /// Normalize path for metric labels (replace IDs with placeholders)
    fn normalize_path(path: &str) -> String {
        // Replace UUIDs with :id placeholder
        let uuid_regex = regex::Regex::new(
            r"[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}",
        )
        .unwrap();
        let path = uuid_regex.replace_all(path, ":id");

        // Replace numeric IDs with :id placeholder
        let numeric_regex = regex::Regex::new(r"/\d+(/|$)").unwrap();
        numeric_regex.replace_all(&path, "/:id$1").to_string()
    }

    /// Update database pool metrics
    pub fn update_db_pool_metrics(active_connections: usize) {
        DB_POOL_CONNECTIONS.set(active_connections as f64);
    }

    /// Increment active connections
    pub fn connection_opened() {
        ACTIVE_CONNECTIONS.inc();
    }

    /// Decrement active connections
    pub fn connection_closed() {
        ACTIVE_CONNECTIONS.dec();
    }

    /// Check alert thresholds and generate alerts if needed
    /// Returns a list of triggered alerts (placeholder for webhook integration)
    pub fn check_alerts(&self) -> Vec<AlertPayload> {
        let mut alerts = Vec::new();
        let error_rate = Self::get_error_rate();

        for threshold in &self.alert_thresholds {
            if threshold.metric_name == "error_rate" {
                if error_rate >= threshold.critical_threshold {
                    alerts.push(AlertPayload {
                        severity: AlertSeverity::Critical,
                        title: "High Error Rate".to_string(),
                        message: format!(
                            "Error rate is {:.2}%, exceeding critical threshold of {:.2}%",
                            error_rate, threshold.critical_threshold
                        ),
                        metric_name: threshold.metric_name.clone(),
                        current_value: error_rate,
                        threshold: threshold.critical_threshold,
                        timestamp: chrono::Utc::now(),
                    });
                } else if error_rate >= threshold.warning_threshold {
                    alerts.push(AlertPayload {
                        severity: AlertSeverity::Warning,
                        title: "Elevated Error Rate".to_string(),
                        message: format!(
                            "Error rate is {:.2}%, exceeding warning threshold of {:.2}%",
                            error_rate, threshold.warning_threshold
                        ),
                        metric_name: threshold.metric_name.clone(),
                        current_value: error_rate,
                        threshold: threshold.warning_threshold,
                        timestamp: chrono::Utc::now(),
                    });
                }
            }
        }

        // Log alerts
        for alert in &alerts {
            match alert.severity {
                AlertSeverity::Critical => {
                    tracing::error!(
                        alert_title = %alert.title,
                        metric = %alert.metric_name,
                        value = %alert.current_value,
                        threshold = %alert.threshold,
                        "CRITICAL ALERT triggered"
                    );
                }
                AlertSeverity::Warning => {
                    tracing::warn!(
                        alert_title = %alert.title,
                        metric = %alert.metric_name,
                        value = %alert.current_value,
                        threshold = %alert.threshold,
                        "WARNING ALERT triggered"
                    );
                }
                AlertSeverity::Info => {
                    tracing::info!(
                        alert_title = %alert.title,
                        metric = %alert.metric_name,
                        value = %alert.current_value,
                        threshold = %alert.threshold,
                        "INFO ALERT triggered"
                    );
                }
            }
        }

        alerts
    }

    /// Placeholder for sending alerts to external systems (webhooks, Slack, PagerDuty, etc.)
    /// TODO: Implement actual webhook integration
    #[allow(dead_code)]
    pub async fn send_alert_webhook(
        &self,
        alert: &AlertPayload,
        webhook_url: &str,
    ) -> Result<(), reqwest::Error> {
        tracing::info!(
            webhook_url = %webhook_url,
            alert_title = %alert.title,
            severity = ?alert.severity,
            "Sending alert to webhook (placeholder)"
        );

        // Placeholder implementation - uncomment when ready to use
        // let client = reqwest::Client::new();
        // client
        //     .post(webhook_url)
        //     .json(alert)
        //     .send()
        //     .await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_path() {
        assert_eq!(
            MetricsService::normalize_path("/users/123e4567-e89b-12d3-a456-426614174000/profile"),
            "/users/:id/profile"
        );
        assert_eq!(
            MetricsService::normalize_path("/orders/12345"),
            "/orders/:id"
        );
        assert_eq!(MetricsService::normalize_path("/health"), "/health");
    }

    #[test]
    fn test_metrics_payload() {
        MetricsService::init();
        let payload = MetricsService::get_metrics_payload();
        assert!(payload.uptime >= 0);
        assert!(payload.error_rate >= 0.0);
    }
}
