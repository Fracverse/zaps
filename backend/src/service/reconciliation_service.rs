use crate::config::Config;
use crate::models::{PaymentReconciliation, ReconciliationAuditLog, ReconciliationSource, ReconciliationStatus};
use chrono::DateTime;
use deadpool_postgres::Pool;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use std::sync::Arc;
use uuid::Uuid;

/// Reconciliation request
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ReconciliationRequest {
    pub source: ReconciliationSource,
    pub records: Vec<ExternalRecord>,
}

/// External record to reconcile
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ExternalRecord {
    pub external_id: String,
    pub amount: i64,
    pub currency: String,
    pub date: DateTime<chrono::Utc>,
}

/// Reconciliation result
#[derive(Debug, Clone, Serialize)]
pub struct ReconciliationResult {
    pub total_records: i64,
    pub matched: i64,
    pub mismatched: i64,
    pub pending: i64,
    pub discrepancies: Vec<Discrepancy>,
}

/// Discrepancy details
#[derive(Debug, Clone, Serialize)]
pub struct Discrepancy {
    pub external_id: Option<String>,
    pub payment_id: Option<String>,
    pub reason: String,
    pub expected_amount: Option<i64>,
    pub actual_amount: Option<i64>,
}

/// Reconciliation service for payment reconciliation
#[derive(Clone)]
#[allow(dead_code)]
pub struct ReconciliationService {
    db_pool: Arc<Pool>,
    config: Config,
}

impl ReconciliationService {
    pub fn new(db_pool: Pool, config: Config) -> Self {
        ReconciliationService {
            db_pool: Arc::new(db_pool),
            config,
        }
    }

    /// Run reconciliation for a source
    pub async fn run_reconciliation(
        &self,
        request: ReconciliationRequest,
        actor_id: String,
    ) -> Result<ReconciliationResult, Box<dyn std::error::Error>> {
        let client = self.db_pool.get().await?;
        let mut result = ReconciliationResult {
            total_records: request.records.len() as i64,
            matched: 0,
            mismatched: 0,
            pending: 0,
            discrepancies: Vec::new(),
        };

        for record in request.records {
            // Try to find matching payment
            let matching_payment = client
                .query_opt(
                    "SELECT id, send_amount FROM payments
                     WHERE tx_hash = $1 OR id = $1
                     LIMIT 1",
                    &[&record.external_id],
                )
                .await?;

            let (status, discrepancy, payment_id_option) = if let Some(row) = matching_payment {
                let payment_id: String = row.get("id");
                let payment_amount: i64 = row.get("send_amount");

                if payment_amount == record.amount {
                    result.matched += 1;
                    (ReconciliationStatus::Matched, None, Some(payment_id))
                } else {
                    result.mismatched += 1;
                    (
                        ReconciliationStatus::Mismatched,
                        Some(Discrepancy {
                            external_id: Some(record.external_id.clone()),
                            payment_id: Some(payment_id.clone()),
                            reason: "Amount mismatch".to_string(),
                            expected_amount: Some(payment_amount),
                            actual_amount: Some(record.amount),
                        }),
                        Some(payment_id),
                    )
                }
            } else {
                result.pending += 1;
                (
                    ReconciliationStatus::Pending,
                    Some(Discrepancy {
                        external_id: Some(record.external_id.clone()),
                        payment_id: None,
                        reason: "No matching payment found".to_string(),
                        expected_amount: None,
                        actual_amount: Some(record.amount),
                    }),
                    None,
                )
            };

            // Create reconciliation record
            let reconciliation_id = Uuid::new_v4().to_string();
            client.execute(
                "INSERT INTO payment_reconciliations
                 (id, payment_id, external_id, source, amount, currency, status, created_at, updated_at)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, NOW(), NOW())",
                &[
                    &reconciliation_id,
                    &payment_id_option,
                    &Some(record.external_id),
                    &match request.source {
                        ReconciliationSource::Stellar => "stellar",
                        ReconciliationSource::Bank => "bank",
                        ReconciliationSource::Manual => "manual",
                    },
                    &record.amount,
                    &record.currency,
                    &status.to_string(),
                ],
            ).await?;

            // Add to audit log
            self.log_audit(&client, &reconciliation_id, &actor_id, "created", None, Some(status.clone()), None).await?;

            if let Some(discrepancy) = discrepancy {
                result.discrepancies.push(discrepancy);
            }
        }

        Ok(result)
    }

    /// Get reconciliation records
    pub async fn get_reconciliations(
        &self,
        status: Option<ReconciliationStatus>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<PaymentReconciliation>, Box<dyn std::error::Error>> {
        let client = self.db_pool.get().await?;
        let status_str = status.as_ref().map(|s| s.to_string());

        let query = if status.is_some() {
            "SELECT id, payment_id, external_id, source, amount, currency, status,
                    discrepancy_notes, resolved_by, resolved_at, created_at, updated_at
             FROM payment_reconciliations
             WHERE status = $1
             ORDER BY created_at DESC
             LIMIT $2 OFFSET $3"
        } else {
            "SELECT id, payment_id, external_id, source, amount, currency, status,
                    discrepancy_notes, resolved_by, resolved_at, created_at, updated_at
             FROM payment_reconciliations
             ORDER BY created_at DESC
             LIMIT $1 OFFSET $2"
        };

        let rows = if let Some(ref s) = status_str {
            client.query(query, &[s, &limit, &offset]).await?
        } else {
            client.query(query, &[&limit, &offset]).await?
        };

        let mut reconciliations = Vec::new();
        for row in rows {
            reconciliations.push(PaymentReconciliation {
                id: row.get("id"),
                payment_id: row.get("payment_id"),
                external_id: row.get("external_id"),
                source: match row.get::<_, String>("source").as_str() {
                    "stellar" => ReconciliationSource::Stellar,
                    "bank" => ReconciliationSource::Bank,
                    _ => ReconciliationSource::Manual,
                },
                amount: row.get("amount"),
                currency: row.get("currency"),
                status: ReconciliationStatus::from_str(&row.get::<_, String>("status")).unwrap(),
                discrepancy_notes: row.get("discrepancy_notes"),
                resolved_by: row.get("resolved_by"),
                resolved_at: row.get("resolved_at"),
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
            });
        }

        Ok(reconciliations)
    }

    /// Manually resolve a reconciliation
    pub async fn resolve_reconciliation(
        &self,
        reconciliation_id: String,
        actor_id: String,
        new_status: ReconciliationStatus,
        notes: Option<String>,
    ) -> Result<PaymentReconciliation, Box<dyn std::error::Error>> {
        let client = self.db_pool.get().await?;

        // Get current status
        let row = client.query_one(
            "SELECT status FROM payment_reconciliations WHERE id = $1",
            &[&reconciliation_id],
        ).await?;
        let old_status = ReconciliationStatus::from_str(&row.get::<_, String>("status")).unwrap();

        // Update reconciliation
        let row = client.query_one(
            "UPDATE payment_reconciliations
             SET status = $1,
                 resolved_by = $2,
                 resolved_at = NOW(),
                 discrepancy_notes = COALESCE($3, discrepancy_notes),
                 updated_at = NOW()
             WHERE id = $4
             RETURNING id, payment_id, external_id, source, amount, currency, status,
                       discrepancy_notes, resolved_by, resolved_at, created_at, updated_at",
            &[&new_status.to_string(), &actor_id, &notes, &reconciliation_id],
        ).await?;

        // Log audit
        self.log_audit(&client, &reconciliation_id, &actor_id, "resolved", Some(old_status), Some(new_status.clone()), notes).await?;

        Ok(PaymentReconciliation {
            id: row.get("id"),
            payment_id: row.get("payment_id"),
            external_id: row.get("external_id"),
            source: match row.get::<_, String>("source").as_str() {
                "stellar" => ReconciliationSource::Stellar,
                "bank" => ReconciliationSource::Bank,
                _ => ReconciliationSource::Manual,
            },
            amount: row.get("amount"),
            currency: row.get("currency"),
            status: new_status,
            discrepancy_notes: row.get("discrepancy_notes"),
            resolved_by: row.get("resolved_by"),
            resolved_at: row.get("resolved_at"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
        })
    }

    /// Get audit log for a reconciliation
    pub async fn get_audit_log(
        &self,
        reconciliation_id: String,
    ) -> Result<Vec<ReconciliationAuditLog>, Box<dyn std::error::Error>> {
        let client = self.db_pool.get().await?;

        let rows = client.query(
            "SELECT id, reconciliation_id, actor_id, action, old_status, new_status, notes, created_at
             FROM reconciliation_audit_logs
             WHERE reconciliation_id = $1
             ORDER BY created_at DESC",
            &[&reconciliation_id],
        ).await?;

        let mut logs = Vec::new();
        for row in rows {
            logs.push(ReconciliationAuditLog {
                id: row.get("id"),
                reconciliation_id: row.get("reconciliation_id"),
                actor_id: row.get("actor_id"),
                action: row.get("action"),
                old_status: row.get::<_, Option<String>>("old_status")
                    .and_then(|s| ReconciliationStatus::from_str(&s).ok()),
                new_status: row.get::<_, Option<String>>("new_status")
                    .and_then(|s| ReconciliationStatus::from_str(&s).ok()),
                notes: row.get("notes"),
                created_at: row.get("created_at"),
            });
        }

        Ok(logs)
    }

    /// Log audit action
    #[allow(clippy::too_many_arguments)]
    async fn log_audit(
        &self,
        client: &deadpool_postgres::Client,
        reconciliation_id: &str,
        actor_id: &str,
        action: &str,
        old_status: Option<ReconciliationStatus>,
        new_status: Option<ReconciliationStatus>,
        notes: Option<String>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        client.execute(
            "INSERT INTO reconciliation_audit_logs
             (id, reconciliation_id, actor_id, action, old_status, new_status, notes, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, NOW())",
            &[
                &Uuid::new_v4().to_string(),
                &reconciliation_id,
                &actor_id,
                &action,
                &old_status.map(|s| s.to_string()),
                &new_status.map(|s| s.to_string()),
                &notes,
            ],
        ).await?;
        Ok(())
    }
}


