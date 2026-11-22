//! #1226 — Circle reconciliation and transaction monitoring.
//!
//! Periodically compares local contribution/payout records with the
//! authoritative Stellar transaction state. Local processing state and
//! confirmed on-chain state are stored separately. A local success is never
//! inferred from a missing or failed RPC result; discrepancies are recorded and
//! alerted on.

use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use std::{env, sync::Arc, time::Duration};
use uuid::Uuid;

use super::stellar::StellarClient;

#[derive(Debug, Clone)]
pub struct ReconciliationConfig {
    pub interval: Duration,
    pub delayed_after: Duration,
    pub alert_webhook: Option<String>,
    pub batch_limit: i64,
}

impl ReconciliationConfig {
    pub fn from_env() -> Self {
        Self {
            interval: Duration::from_secs(
                env::var("CIRCLE_RECONCILIATION_INTERVAL_SECS")
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(300)
                    .max(30),
            ),
            delayed_after: Duration::from_secs(
                env::var("CIRCLE_RECONCILIATION_DELAYED_AFTER_SECS")
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(1800),
            ),
            alert_webhook: env::var("CIRCLE_RECONCILIATION_ALERT_WEBHOOK").ok(),
            batch_limit: 100,
        }
    }
}

/// Spawn the periodic reconciliation worker.
pub fn spawn(pool: PgPool, rpc_url: String, config: ReconciliationConfig) {
    let client = Arc::new(StellarClient::new(rpc_url));
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(config.interval);
        loop {
            ticker.tick().await;
            match reconcile_once(&pool, &client, &config).await {
                Ok(summary) => tracing::info!(
                    event = "circle_reconciliation_completed",
                    checked = summary.checked,
                    discrepancies = summary.discrepancies,
                    "circle reconciliation pass complete"
                ),
                Err(e) => tracing::warn!(
                    event = "circle_reconciliation_failed",
                    error = %e,
                    "circle reconciliation pass failed"
                ),
            }
        }
    });
}

#[derive(Default, Debug)]
pub struct ReconciliationSummary {
    pub checked: usize,
    pub discrepancies: usize,
}

struct LocalRecord {
    record_type: String,
    id: Uuid,
    circle_id: Uuid,
    tx_hash: Option<String>,
    local_state: String,
    age: Duration,
}

async fn fetch_records(pool: &PgPool, limit: i64) -> Result<Vec<LocalRecord>, sqlx::Error> {
    let mut out = Vec::new();

    let contributions = sqlx::query(
        r#"SELECT c.id, c.circle_id, c.tx_hash,
                  EXTRACT(EPOCH FROM (NOW() - c.created_at))::BIGINT AS age
             FROM circle_contributions c
            WHERE NOT EXISTS (
                SELECT 1 FROM circle_reconciliation_records r
                 WHERE r.record_type = 'contribution' AND r.local_record_id = c.id
                   AND r.on_chain_state = 'success'
            )
            ORDER BY c.created_at DESC LIMIT $1"#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;
    for row in contributions {
        out.push(LocalRecord {
            record_type: "contribution".to_string(),
            id: row.get("id"),
            circle_id: row.get("circle_id"),
            tx_hash: row.get("tx_hash"),
            local_state: "recorded".to_string(),
            age: Duration::from_secs(row.get::<i64, _>("age").max(0) as u64),
        });
    }

    let payouts = sqlx::query(
        r#"SELECT p.id, p.circle_id, p.tx_hash, p.status, p.is_executed,
                  EXTRACT(EPOCH FROM (NOW() - p.created_at))::BIGINT AS age
             FROM circle_payout_requests p
            WHERE NOT EXISTS (
                SELECT 1 FROM circle_reconciliation_records r
                 WHERE r.record_type = 'payout' AND r.local_record_id = p.id
                   AND r.on_chain_state = 'success'
            )
            ORDER BY p.created_at DESC LIMIT $1"#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;
    for row in payouts {
        let executed: bool = row.get("is_executed");
        let status: String = row.get("status");
        out.push(LocalRecord {
            record_type: "payout".to_string(),
            id: row.get("id"),
            circle_id: row.get("circle_id"),
            tx_hash: row.get("tx_hash"),
            local_state: if executed {
                "executed".to_string()
            } else {
                status
            },
            age: Duration::from_secs(row.get::<i64, _>("age").max(0) as u64),
        });
    }
    Ok(out)
}

/// Query Stellar for a transaction hash. Only an explicit `SUCCESS` status is
/// treated as confirmation; RPC errors (`not_found`, transport failures) are
/// reported as `unknown`/`not_found`, never as success.
async fn on_chain_state(client: &StellarClient, tx_hash: &str) -> (String, String) {
    match client
        .send_rpc_request("getTransaction", json!({ "hash": tx_hash }))
        .await
    {
        Ok(value) => classify_rpc_response(&value),
        Err(e) => ("unknown".to_string(), format!("RPC error: {e}")),
    }
}

fn classify_rpc_response(value: &Value) -> (String, String) {
    let status = value
        .get("transaction")
        .and_then(|tx| tx.get("status"))
        .and_then(Value::as_str)
        .map(str::to_ascii_lowercase);
    match status.as_deref() {
        Some("success") => (
            "success".to_string(),
            "transaction status SUCCESS".to_string(),
        ),
        Some("failed") => (
            "failed".to_string(),
            "transaction status FAILED".to_string(),
        ),
        Some("not_found") => (
            "not_found".to_string(),
            "transaction not found on Stellar".to_string(),
        ),
        other => (
            "unknown".to_string(),
            format!("unexpected RPC status: {}", other.unwrap_or("none")),
        ),
    }
}

async fn alert(config: &ReconciliationConfig, record_type: &str, discrepancy: &str, details: &str) {
    tracing::warn!(
        event = "circle_reconciliation_discrepancy",
        record_type,
        discrepancy,
        details,
        "circle financial record requires review"
    );
    if let Some(webhook) = &config.alert_webhook {
        match reqwest::Client::new()
            .post(webhook)
            .json(&json!({
                "event": "circle_reconciliation_discrepancy",
                "record_type": record_type,
                "discrepancy": discrepancy,
                "details": details,
            }))
            .send()
            .await
        {
            Ok(_) => {}
            Err(e) => {
                tracing::warn!(event = "circle_reconciliation_alert_failed", error = %e, "alert webhook failed")
            }
        }
    }
}

/// Run a single reconciliation pass.
pub async fn reconcile_once(
    pool: &PgPool,
    client: &StellarClient,
    config: &ReconciliationConfig,
) -> Result<ReconciliationSummary, Box<dyn std::error::Error + Send + Sync>> {
    let records = fetch_records(pool, config.batch_limit).await?;
    let mut summary = ReconciliationSummary::default();

    for record in records {
        summary.checked += 1;

        // Duplicate hashes: more than one local financial record claims the
        // same on-chain transaction — never silently accept both.
        let duplicates: i64 = if let Some(hash) = &record.tx_hash {
            let table = if record.record_type == "contribution" {
                "circle_contributions"
            } else {
                "circle_payout_requests"
            };
            sqlx::query_scalar(&format!(
                "SELECT COUNT(*)::BIGINT FROM {table} WHERE tx_hash = $1"
            ))
            .bind(hash)
            .fetch_one(pool)
            .await?
        } else {
            0
        };

        let (on_chain_state, mut details) = match &record.tx_hash {
            Some(hash) if !hash.is_empty() => on_chain_state(client, hash).await,
            _ => (
                "unknown".to_string(),
                "no transaction hash recorded locally".to_string(),
            ),
        };

        let discrepancy = if duplicates > 1 {
            "duplicate"
        } else if record.tx_hash.is_none() && record.local_state == "executed" {
            "missing"
        } else if on_chain_state == "not_found" && record.age >= config.delayed_after {
            "delayed"
        } else if on_chain_state == "failed" || on_chain_state == "unknown" {
            "inconsistent"
        } else if on_chain_state == "success" {
            "none"
        } else {
            // Still pending on-chain: not a discrepancy yet.
            "none"
        };

        if discrepancy != "none" {
            summary.discrepancies += 1;
            details = format!(
                "{details}; local_state={}, duplicates={duplicates}",
                record.local_state
            );
            alert(config, &record.record_type, discrepancy, &details).await;
        }

        sqlx::query(
            r#"INSERT INTO circle_reconciliation_records
                   (id, record_type, local_record_id, circle_id, tx_hash,
                    local_state, on_chain_state, discrepancy, details, checked_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, NOW())
               ON CONFLICT (record_type, local_record_id) DO UPDATE SET
                   local_state = EXCLUDED.local_state,
                   on_chain_state = EXCLUDED.on_chain_state,
                   discrepancy = EXCLUDED.discrepancy,
                   details = EXCLUDED.details,
                   checked_at = NOW()"#,
        )
        .bind(Uuid::new_v4())
        .bind(&record.record_type)
        .bind(record.id)
        .bind(record.circle_id)
        .bind(&record.tx_hash)
        .bind(&record.local_state)
        .bind(&on_chain_state)
        .bind(discrepancy)
        .bind(details)
        .execute(pool)
        .await?;
    }
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_explicit_success_is_confirmed() {
        assert_eq!(
            classify_rpc_response(&json!({"transaction": {"status": "SUCCESS"}})).0,
            "success"
        );
        assert_eq!(
            classify_rpc_response(&json!({"transaction": {"status": "FAILED"}})).0,
            "failed"
        );
        assert_eq!(
            classify_rpc_response(&json!({"error": "not found"})).0,
            "unknown"
        );
        assert_eq!(classify_rpc_response(&json!({})).0, "unknown");
    }
}
