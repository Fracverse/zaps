use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// Telemetry configuration options
pub struct TelemetryConfig {
    /// Enable JSON logging format (for production/log aggregation)
    pub json_format: bool,
    /// Log level filter (e.g., "info", "debug", "zaps_backend=debug,tower_http=info")
    pub log_filter: Option<String>,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            json_format: std::env::var("LOG_FORMAT")
                .map(|v| v.to_lowercase() == "json")
                .unwrap_or(false),
            log_filter: None,
        }
    }
}

/// Initialize tracing/logging with the default configuration
pub fn init_tracing() -> Result<(), Box<dyn std::error::Error>> {
    init_tracing_with_config(TelemetryConfig::default())
}

/// Initialize tracing/logging with custom configuration
/// 
/// Supports two logging formats:
/// - Compact: Human-readable format for development
/// - JSON: Structured format for production and log aggregation (Datadog, Elasticsearch, etc.)
/// 
/// Set `LOG_FORMAT=json` environment variable to enable JSON logging.
pub fn init_tracing_with_config(config: TelemetryConfig) -> Result<(), Box<dyn std::error::Error>> {
    let env_filter = match config.log_filter {
        Some(filter) => EnvFilter::try_new(filter)?,
        None => EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new("zaps_backend=info,tower_http=info")),
    };

    if config.json_format {
        // JSON format for production - structured logging
        let fmt_layer = fmt::layer()
            .json()
            .with_target(true)
            .with_thread_ids(true)
            .with_file(true)
            .with_line_number(true)
            .with_current_span(true)
            .flatten_event(true);

        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt_layer)
            .init();

        tracing::info!(
            service.name = "zaps-backend",
            service.version = env!("CARGO_PKG_VERSION"),
            log.format = "json",
            "Telemetry initialized with JSON structured logging"
        );
    } else {
        // Compact format for development - human-readable
        let fmt_layer = fmt::layer()
            .with_target(false)
            .with_thread_ids(false)
            .with_thread_names(false)
            .compact();

        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt_layer)
            .init();

        tracing::info!(
            service.name = "zaps-backend",
            service.version = env!("CARGO_PKG_VERSION"),
            log.format = "compact",
            "Telemetry initialized with compact logging"
        );
    }

    Ok(())
}

/// Create a span for tracing a specific operation
/// 
/// Example usage:
/// ```ignore
/// let _span = create_operation_span("process_payment", &[("payment_id", "123")]);
/// // ... operation code ...
/// ```
#[macro_export]
macro_rules! create_operation_span {
    ($operation:expr) => {
        tracing::info_span!("operation", operation = $operation)
    };
    ($operation:expr, $($key:expr => $value:expr),*) => {
        tracing::info_span!("operation", operation = $operation, $($key = $value),*)
    };
}

/// Log a structured event with common fields
#[macro_export]
macro_rules! log_event {
    ($level:ident, $event_type:expr, $($key:expr => $value:expr),*) => {
        tracing::$level!(
            event.type = $event_type,
            $($key = $value),*
        );
    };
}

