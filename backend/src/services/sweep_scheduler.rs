use serde_json::json;
use sqlx::PgPool;
use std::time::Duration;

use crate::db::r#yield::{list_repeated_sweep_failures, mark_sweep_failure_alerted};
use crate::services::notifications::dispatch_http_webhook;

/// #737: Poll interval for the sweep-failure alert loop.
const DEFAULT_ALERT_POLL_INTERVAL_SECS: u64 = 300;
/// #737: A user is considered to be failing repeatedly once their recorded
/// failure count reaches this many consecutive sweep errors.
const DEFAULT_REPEATED_FAILURE_THRESHOLD: i32 = 3;

pub struct SweepSchedulerConfig {
    pub poll_interval: Duration,
    /// URL of the operations webhook (e.g. Slack/Discord) that receives
    /// repeated sweep-failure alerts. `None` disables alerting.
    pub ops_webhook_url: Option<String>,
    /// Number of consecutive failures that triggers an operations alert.
    pub repeated_failure_threshold: i32,
}

impl SweepSchedulerConfig {
    pub fn from_env() -> Self {
        let poll_secs = std::env::var("SWEEP_ALERT_POLL_INTERVAL_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_ALERT_POLL_INTERVAL_SECS);
        let threshold = std::env::var("SWEEP_REPEATED_FAILURE_THRESHOLD")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_REPEATED_FAILURE_THRESHOLD);

        Self {
            poll_interval: Duration::from_secs(poll_secs),
            ops_webhook_url: std::env::var("SWEEP_OPS_WEBHOOK_URL")
                .ok()
                .filter(|url| !url.trim().is_empty()),
            repeated_failure_threshold: threshold,
        }
    }
}

/// #737: Watch `sweep_failure_history` for users whose sweep keeps failing and
/// alert the operations channel (via webhook) once per failing episode.
///
/// The auto-sweep worker (`sweep_worker.rs`) is responsible for catching failed
/// sweep executions and inserting records into `sweep_failure_history`. This
/// scheduler turns those records into a human-readable alert without spamming:
/// an alert fires when a user crosses `repeated_failure_threshold`, and not
/// again until the next failing episode resets `last_alerted_at`.
pub async fn run(pool: PgPool, config: SweepSchedulerConfig) {
    tracing::info!(
        "Starting sweep-failure alert scheduler (interval={:?}, threshold={}, webhook={})",
        config.poll_interval,
        config.repeated_failure_threshold,
        config.ops_webhook_url.as_deref().map(|_| "configured").unwrap_or("disabled")
    );

    let mut interval = tokio::time::interval(config.poll_interval);

    loop {
        interval.tick().await;

        if let Err(err) = alert_on_repeated_failures(&pool, &config).await {
            tracing::error!("Sweep-failure alert cycle failed: {err:?}");
        }
    }
}

/// One alert cycle: find users past the threshold that haven't been alerted for
/// the current episode and dispatch a webhook for each.
async fn alert_on_repeated_failures(
    pool: &PgPool,
    config: &SweepSchedulerConfig,
) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
    let Some(webhook_url) = config.ops_webhook_url.as_deref() else {
        return Ok(0);
    };

    let failures = list_repeated_sweep_failures(pool, config.repeated_failure_threshold).await?;
    if failures.is_empty() {
        return Ok(0);
    }

    let client = reqwest::Client::new();
    let mut alerted = 0usize;

    for failure in failures {
        let payload = json!({
            "event": "sweep_repeated_failure",
            "user_id": failure.user_id.to_string(),
            "failure_count": failure.failure_count,
            "last_error": failure.last_error.as_deref().unwrap_or("unknown"),
            "last_failed_at": failure.last_failed_at.to_string(),
            "next_retry_at": failure.next_retry_at.to_string(),
            "message": format!(
                "Auto-sweep has failed {} consecutive times for user {}; next retry at {}. {}",
                failure.failure_count,
                failure.user_id,
                failure.next_retry_at,
                failure.last_error.as_deref().unwrap_or("unknown")
            ),
        });

        match dispatch_http_webhook(pool, &client, webhook_url, &payload).await {
            Ok(()) => {
                mark_sweep_failure_alerted(pool, failure.user_id).await?;
                alerted += 1;
                tracing::info!(
                    user_id = %failure.user_id,
                    failure_count = failure.failure_count,
                    "Alerted operations channel about repeated sweep failure"
                );
            }
            Err(err) => {
                // A webhook delivery failure must not abort the cycle; the row
                // stays un-alerted and is retried on the next poll.
                tracing::warn!(
                    user_id = %failure.user_id,
                    error = ?err,
                    "Failed to dispatch repeated sweep-failure alert"
                );
            }
        }
    }

    tracing::debug!("Sweep-failure alert cycle complete: alerted {alerted} user(s)");
    Ok(alerted)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_defaults_are_sensible() {
        let config = SweepSchedulerConfig {
            poll_interval: Duration::from_secs(DEFAULT_ALERT_POLL_INTERVAL_SECS),
            ops_webhook_url: None,
            repeated_failure_threshold: DEFAULT_REPEATED_FAILURE_THRESHOLD,
        };
        assert_eq!(config.poll_interval.as_secs(), 300);
        assert_eq!(config.repeated_failure_threshold, 3);
        assert!(config.ops_webhook_url.is_none());
    }

    #[test]
    fn threshold_requires_more_than_one_failure() {
        // A single transient failure shouldn't page operations; only repeated
        // failures (>= 3) are alert-worthy.
        assert!(DEFAULT_REPEATED_FAILURE_THRESHOLD > 1);
    }
}
