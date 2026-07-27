//! BE-554: background queue worker for bulk disbursement batches.
//!
//! Callers create a `payout_batches` row with its `batch_recipients`; this
//! worker drains them asynchronously so an HTTP request never blocks on
//! thousands of sequential SDP submissions.
//!
//! ## Why the database is the queue
//!
//! The issue suggests a DB-backed task list or Redis streams. This uses the
//! database, for one reason: the payout rows and the queue state have to move
//! together. With Redis the job and the row it describes live in separate
//! systems, and a crash between "SDP accepted the payment" and "Redis acked the
//! job" leaves them disagreeing — which for a payout system means paying twice
//! or not at all. `SELECT ... FOR UPDATE SKIP LOCKED` gives the same
//! competing-consumer semantics with the claim in the same transaction as the
//! state it guards, and adds no new infrastructure.
//!
//! ## Delivery semantics
//!
//! At-least-once, made safe by idempotency rather than by trying to be
//! exactly-once:
//!
//! - Recipients are claimed with `SKIP LOCKED`, so N workers never hand the same
//!   row to SDP twice concurrently.
//! - Each submission carries a deterministic idempotency key derived from the
//!   recipient row id, so a retry after an ambiguous failure is deduplicated by
//!   SDP rather than paying again.
//! - A row that fails is retried up to `max_attempts`, then marked FAILED and
//!   left alone. A permanently-failing recipient must not block the batch.
//!
//! ## Ordering
//!
//! Submissions within a batch are sequential, as the issue requires. That is
//! deliberate beyond just following the spec: SDP applies rate limits per
//! account, and a burst of parallel submissions from one source account
//! produces sequence-number contention on the Stellar side.

use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::time::Duration;
use uuid::Uuid;

use crate::db::models::BatchRecipient;

const DEFAULT_POLL_INTERVAL_SECS: u64 = 15;
const DEFAULT_MAX_ATTEMPTS: i32 = 3;
/// Recipients claimed per cycle. Bounded so a 100k-recipient batch does not
/// hold a single transaction open for its entire duration.
const DEFAULT_CLAIM_SIZE: i64 = 100;
/// A claim older than this is assumed to belong to a dead worker.
const DEFAULT_LEASE_TIMEOUT_SECS: i64 = 900;

pub struct DisbursementWorkerConfig {
    pub poll_interval: Duration,
    pub max_attempts: i32,
    pub claim_size: i64,
    pub lease_timeout_secs: i64,
    /// SDP API base URL, e.g. `https://sdp.example.org`.
    pub sdp_base_url: String,
    /// Bearer token for the SDP API. Without it the worker runs in dry-run mode.
    pub sdp_api_token: Option<String>,
    /// Identifies this process in `batch_recipients.locked_by`.
    pub worker_id: String,
}

impl DisbursementWorkerConfig {
    pub fn from_env() -> Self {
        let poll_secs = std::env::var("DISBURSEMENT_POLL_INTERVAL_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_POLL_INTERVAL_SECS);
        let max_attempts = std::env::var("DISBURSEMENT_MAX_ATTEMPTS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_MAX_ATTEMPTS);
        let claim_size = std::env::var("DISBURSEMENT_CLAIM_SIZE")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_CLAIM_SIZE);
        let lease_timeout_secs = std::env::var("DISBURSEMENT_LEASE_TIMEOUT_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_LEASE_TIMEOUT_SECS);
        let sdp_base_url = std::env::var("SDP_BASE_URL")
            .unwrap_or_else(|_| "https://sdp.stellar.org".into());
        let sdp_api_token = std::env::var("SDP_API_TOKEN").ok();
        // Hostname keeps lock ownership meaningful across replicas; the uuid
        // suffix disambiguates multiple workers on the same host.
        let worker_id = format!(
            "{}-{}",
            std::env::var("HOSTNAME").unwrap_or_else(|_| "worker".into()),
            Uuid::new_v4()
        );

        Self {
            poll_interval: Duration::from_secs(poll_secs),
            max_attempts,
            claim_size,
            lease_timeout_secs,
            sdp_base_url,
            sdp_api_token,
            worker_id,
        }
    }
}

/// Payload posted to SDP for a single disbursement.
#[derive(Debug, Serialize)]
struct SdpDisbursementRequest<'a> {
    /// Deterministic per-recipient key. SDP deduplicates on this, which is what
    /// makes an at-least-once retry safe.
    idempotency_key: String,
    destination: &'a str,
    amount: i64,
    currency: &'a str,
}

#[derive(Debug, Deserialize)]
struct SdpDisbursementResponse {
    #[serde(default)]
    payment_id: Option<String>,
    #[serde(default)]
    tx_hash: Option<String>,
}

/// Outcome of one dispatch attempt.
enum DispatchOutcome {
    Submitted {
        sdp_payment_id: Option<String>,
        tx_hash: Option<String>,
    },
    /// Transient — worth another attempt.
    Retryable(String),
    /// Permanent — retrying will not help (bad address, rejected amount).
    Permanent(String),
}

/// Entry point. Runs until the process exits.
pub async fn run(pool: PgPool, config: DisbursementWorkerConfig) {
    tracing::info!(
        worker_id = %config.worker_id,
        "Starting disbursement worker (interval={:?}, claim_size={}, max_attempts={})",
        config.poll_interval,
        config.claim_size,
        config.max_attempts
    );

    if config.sdp_api_token.is_none() {
        tracing::warn!(
            "SDP_API_TOKEN not set; disbursement worker running in dry-run mode \
             (recipients will be marked SUBMITTED without contacting SDP)"
        );
    }

    let http = reqwest::Client::new();
    let mut interval = tokio::time::interval(config.poll_interval);

    loop {
        interval.tick().await;

        if let Err(err) = reclaim_stale_leases(&pool, config.lease_timeout_secs).await {
            tracing::error!("Failed to reclaim stale leases: {err:?}");
        }

        if let Err(err) = process_cycle(&pool, &config, &http).await {
            tracing::error!("Disbursement cycle failed: {err:?}");
        }
    }
}

/// Releases claims held by workers that died mid-dispatch.
///
/// Without this a crashed worker's rows stay locked forever and the batch never
/// completes. `attempt_count` is left as-is so a row that keeps killing its
/// worker still exhausts its retries rather than looping indefinitely.
async fn reclaim_stale_leases(pool: &PgPool, lease_timeout_secs: i64) -> Result<(), sqlx::Error> {
    let reclaimed = sqlx::query(
        r#"
        UPDATE batch_recipients
           SET locked_at = NULL,
               locked_by = NULL,
               updated_at = NOW()
         WHERE status = 'PENDING'
           AND locked_at IS NOT NULL
           AND locked_at < NOW() - ($1 || ' seconds')::interval
        "#,
    )
    .bind(lease_timeout_secs.to_string())
    .execute(pool)
    .await?
    .rows_affected();

    if reclaimed > 0 {
        tracing::warn!("Reclaimed {reclaimed} recipient(s) from expired worker leases");
    }
    Ok(())
}

/// Claims one batch and drains up to `claim_size` of its recipients.
async fn process_cycle(
    pool: &PgPool,
    config: &DisbursementWorkerConfig,
    http: &reqwest::Client,
) -> Result<(), sqlx::Error> {
    let Some(batch_id) = claim_next_batch(pool, &config.worker_id).await? else {
        tracing::debug!("Disbursement: no batches awaiting processing");
        return Ok(());
    };

    let recipients = claim_recipients(pool, batch_id, config).await?;
    if recipients.is_empty() {
        // Nothing left to send — settle the batch's terminal status.
        finalize_batch(pool, batch_id).await?;
        return Ok(());
    }

    tracing::info!(
        batch_id = %batch_id,
        count = recipients.len(),
        "Dispatching payout batch"
    );

    // Sequential on purpose: SDP rate-limits per account, and parallel
    // submissions from one source account contend on the Stellar sequence
    // number.
    for recipient in recipients {
        let attempt = recipient.attempt_count + 1;
        let outcome = dispatch_one(&recipient, config, http).await;

        match outcome {
            DispatchOutcome::Submitted {
                sdp_payment_id,
                tx_hash,
            } => {
                mark_submitted(pool, &recipient, sdp_payment_id.as_deref(), tx_hash.as_deref())
                    .await?;
                log_dispatch(pool, batch_id, Some(recipient.id), attempt, "SUBMITTED", None, None)
                    .await?;
            }
            DispatchOutcome::Retryable(err) if attempt < config.max_attempts => {
                mark_retry(pool, &recipient, &err).await?;
                log_dispatch(
                    pool,
                    batch_id,
                    Some(recipient.id),
                    attempt,
                    "RETRY_SCHEDULED",
                    None,
                    Some(&err),
                )
                .await?;
            }
            DispatchOutcome::Retryable(err) | DispatchOutcome::Permanent(err) => {
                // Either permanently bad, or out of retries. Fail this row only
                // — one dead recipient must not strand the rest of the batch.
                mark_failed(pool, &recipient, &err).await?;
                log_dispatch(
                    pool,
                    batch_id,
                    Some(recipient.id),
                    attempt,
                    "FAILED",
                    None,
                    Some(&err),
                )
                .await?;
            }
        }
    }

    finalize_batch(pool, batch_id).await?;
    Ok(())
}

/// Claims the oldest batch awaiting work.
///
/// `FOR UPDATE SKIP LOCKED` is what makes this safe to run on N replicas: a
/// batch already being claimed by a peer is skipped rather than blocking.
async fn claim_next_batch(pool: &PgPool, worker_id: &str) -> Result<Option<Uuid>, sqlx::Error> {
    let row: Option<(Uuid,)> = sqlx::query_as(
        r#"
        WITH next_batch AS (
            SELECT id
              FROM payout_batches
             WHERE status IN ('PENDING', 'PROCESSING')
             ORDER BY created_at
             FOR UPDATE SKIP LOCKED
             LIMIT 1
        )
        UPDATE payout_batches b
           SET status = 'PROCESSING',
               started_at = COALESCE(b.started_at, NOW()),
               updated_at = NOW()
          FROM next_batch
         WHERE b.id = next_batch.id
        RETURNING b.id
        "#,
    )
    .fetch_optional(pool)
    .await?;

    if let Some((batch_id,)) = row {
        log_dispatch(pool, batch_id, None, 1, "CLAIMED", None, Some(worker_id)).await?;
        return Ok(Some(batch_id));
    }
    Ok(None)
}

/// Claims a bounded slice of a batch's pending recipients.
async fn claim_recipients(
    pool: &PgPool,
    batch_id: Uuid,
    config: &DisbursementWorkerConfig,
) -> Result<Vec<BatchRecipient>, sqlx::Error> {
    sqlx::query_as::<_, BatchRecipient>(
        r#"
        WITH claimed AS (
            SELECT id
              FROM batch_recipients
             WHERE batch_id = $1
               AND status = 'PENDING'
               AND locked_at IS NULL
             ORDER BY created_at
             FOR UPDATE SKIP LOCKED
             LIMIT $2
        )
        UPDATE batch_recipients r
           SET locked_at = NOW(),
               locked_by = $3,
               updated_at = NOW()
          FROM claimed
         WHERE r.id = claimed.id
        RETURNING r.*
        "#,
    )
    .bind(batch_id)
    .bind(config.claim_size)
    .bind(&config.worker_id)
    .fetch_all(pool)
    .await
}

/// Submits a single recipient to SDP.
async fn dispatch_one(
    recipient: &BatchRecipient,
    config: &DisbursementWorkerConfig,
    http: &reqwest::Client,
) -> DispatchOutcome {
    let Some(destination) = recipient.destination_address.as_deref() else {
        // The schema guarantees a user_id when there is no address, but this
        // worker sends to addresses. A row that reached here without one was
        // never resolvable, so retrying cannot help.
        return DispatchOutcome::Permanent(
            "recipient has no destination_address; resolve user_id to an address first".into(),
        );
    };

    let Some(token) = config.sdp_api_token.as_deref() else {
        // Dry run: exercise the full state machine without moving money.
        return DispatchOutcome::Submitted {
            sdp_payment_id: Some(format!("dry-run-{}", recipient.id)),
            tx_hash: None,
        };
    };

    let request = SdpDisbursementRequest {
        // Derived from the row id, not from the attempt: every retry of this
        // recipient reuses the same key so SDP deduplicates instead of paying
        // again.
        idempotency_key: idempotency_key_for(recipient.id),
        destination,
        amount: recipient.amount,
        currency: "USDC",
    };

    let response = http
        .post(format!("{}/disbursements", config.sdp_base_url.trim_end_matches('/')))
        .bearer_auth(token)
        .json(&request)
        .send()
        .await;

    let response = match response {
        Ok(r) => r,
        // Network-level failure: no way to know whether SDP saw it, so retry
        // and let the idempotency key deduplicate.
        Err(err) => return DispatchOutcome::Retryable(format!("SDP request failed: {err}")),
    };

    let status = response.status();
    if status.is_success() {
        return match response.json::<SdpDisbursementResponse>().await {
            Ok(body) => DispatchOutcome::Submitted {
                sdp_payment_id: body.payment_id,
                tx_hash: body.tx_hash,
            },
            // SDP accepted it; we just could not parse the body. Treating this
            // as a failure would risk a double payment on retry, so record the
            // submission and let reconciliation fill in the ids.
            Err(err) => {
                tracing::warn!(
                    recipient_id = %recipient.id,
                    error = ?err,
                    "SDP returned success with an unparseable body"
                );
                DispatchOutcome::Submitted {
                    sdp_payment_id: None,
                    tx_hash: None,
                }
            }
        };
    }

    let detail = response.text().await.unwrap_or_default();

    // 4xx other than 408/429 means SDP rejected the request itself — a bad
    // address or a malformed amount will be rejected identically forever.
    if status.is_client_error()
        && status != reqwest::StatusCode::REQUEST_TIMEOUT
        && status != reqwest::StatusCode::TOO_MANY_REQUESTS
    {
        return DispatchOutcome::Permanent(format!("SDP rejected request ({status}): {detail}"));
    }

    DispatchOutcome::Retryable(format!("SDP error ({status}): {detail}"))
}

/// Deterministic idempotency key for a recipient row.
pub fn idempotency_key_for(recipient_id: Uuid) -> String {
    format!("zaps-payout-{recipient_id}")
}

// ─── State transitions ───────────────────────────────────────────────────────

async fn mark_submitted(
    pool: &PgPool,
    recipient: &BatchRecipient,
    sdp_payment_id: Option<&str>,
    tx_hash: Option<&str>,
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;

    sqlx::query(
        r#"
        UPDATE batch_recipients
           SET status = 'SUBMITTED',
               sdp_payment_id = COALESCE($2, sdp_payment_id),
               tx_hash = COALESCE($3, tx_hash),
               attempt_count = attempt_count + 1,
               last_error = NULL,
               locked_at = NULL,
               locked_by = NULL,
               updated_at = NOW()
         WHERE id = $1
        "#,
    )
    .bind(recipient.id)
    .bind(sdp_payment_id)
    .bind(tx_hash)
    .execute(&mut *tx)
    .await?;

    // Counter and row move together, so a crash here cannot leave the batch
    // claiming more successes than it has.
    sqlx::query(
        r#"
        UPDATE payout_batches
           SET succeeded_count = succeeded_count + 1,
               updated_at = NOW()
         WHERE id = $1
        "#,
    )
    .bind(recipient.batch_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await
}

/// Releases the lease and increments the attempt so the row is picked up again.
async fn mark_retry(
    pool: &PgPool,
    recipient: &BatchRecipient,
    error: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        UPDATE batch_recipients
           SET attempt_count = attempt_count + 1,
               last_error = $2,
               locked_at = NULL,
               locked_by = NULL,
               updated_at = NOW()
         WHERE id = $1
        "#,
    )
    .bind(recipient.id)
    .bind(error)
    .execute(pool)
    .await?;
    Ok(())
}

async fn mark_failed(
    pool: &PgPool,
    recipient: &BatchRecipient,
    error: &str,
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;

    sqlx::query(
        r#"
        UPDATE batch_recipients
           SET status = 'FAILED',
               attempt_count = attempt_count + 1,
               last_error = $2,
               locked_at = NULL,
               locked_by = NULL,
               updated_at = NOW()
         WHERE id = $1
        "#,
    )
    .bind(recipient.id)
    .bind(error)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        r#"
        UPDATE payout_batches
           SET failed_count = failed_count + 1,
               updated_at = NOW()
         WHERE id = $1
        "#,
    )
    .bind(recipient.batch_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await
}

/// Settles a batch's terminal status once no PENDING recipients remain.
///
/// Derived from the recipient rows rather than the denormalised counters, so a
/// counter that drifted cannot strand a finished batch in PROCESSING.
async fn finalize_batch(pool: &PgPool, batch_id: Uuid) -> Result<(), sqlx::Error> {
    let result = sqlx::query(
        r#"
        WITH tally AS (
            SELECT COUNT(*) FILTER (WHERE status = 'PENDING')  AS pending,
                   COUNT(*) FILTER (WHERE status = 'FAILED')   AS failed,
                   COUNT(*)                                    AS total
              FROM batch_recipients
             WHERE batch_id = $1
        )
        UPDATE payout_batches b
           SET status = CASE
                            WHEN tally.failed = 0            THEN 'COMPLETED'
                            WHEN tally.failed = tally.total  THEN 'FAILED'
                            ELSE 'PARTIALLY_FAILED'
                        END,
               completed_at = NOW(),
               updated_at = NOW()
          FROM tally
         WHERE b.id = $1
           AND tally.pending = 0
           AND b.status = 'PROCESSING'
        "#,
    )
    .bind(batch_id)
    .execute(pool)
    .await?;

    if result.rows_affected() > 0 {
        tracing::info!(batch_id = %batch_id, "Payout batch finished");
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn log_dispatch(
    pool: &PgPool,
    batch_id: Uuid,
    recipient_id: Option<Uuid>,
    attempt: i32,
    event: &str,
    sdp_response_code: Option<&str>,
    detail: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO dispatch_logs
            (batch_id, recipient_id, attempt, event, sdp_response_code, detail)
        VALUES ($1, $2, $3, $4, $5, $6)
        "#,
    )
    .bind(batch_id)
    .bind(recipient_id)
    .bind(attempt)
    .bind(event)
    .bind(sdp_response_code)
    .bind(detail)
    .execute(pool)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idempotency_key_is_stable_across_retries() {
        let id = Uuid::new_v4();
        // The key must not vary with attempt number — that is what stops a
        // retry from being paid as a second disbursement.
        assert_eq!(idempotency_key_for(id), idempotency_key_for(id));
        assert!(idempotency_key_for(id).contains(&id.to_string()));
    }

    #[test]
    fn idempotency_keys_differ_between_recipients() {
        assert_ne!(
            idempotency_key_for(Uuid::new_v4()),
            idempotency_key_for(Uuid::new_v4())
        );
    }

    #[test]
    fn config_defaults_are_sensible() {
        let config = DisbursementWorkerConfig {
            poll_interval: Duration::from_secs(DEFAULT_POLL_INTERVAL_SECS),
            max_attempts: DEFAULT_MAX_ATTEMPTS,
            claim_size: DEFAULT_CLAIM_SIZE,
            lease_timeout_secs: DEFAULT_LEASE_TIMEOUT_SECS,
            sdp_base_url: "https://sdp.example.org".into(),
            sdp_api_token: None,
            worker_id: "test-worker".into(),
        };

        assert!(config.max_attempts >= 1, "a row must get at least one attempt");
        assert!(config.claim_size > 0);
        // The lease has to outlast a full claim of sequential submissions, or
        // workers reclaim rows that are still legitimately in flight.
        assert!(config.lease_timeout_secs > config.poll_interval.as_secs() as i64);
    }
}
