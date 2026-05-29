use crate::{
    api_error::ApiError,
    config::Config,
    models::{Payout, PayoutBatch, PayoutReconciliation, PayoutStatus},
};
use chrono::{Duration, Utc};
use deadpool_postgres::Pool;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone)]
#[allow(dead_code)]
pub struct PayoutService {
    db_pool: Arc<Pool>,
    config: Config,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreatePayoutRequest {
    pub merchant_id: String,
    pub amount: i64,
    pub currency: String,
    pub destination_address: String,
    pub scheduled_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreatePayoutBatchRequest {
    pub merchant_id: String,
    pub payouts: Vec<CreatePayoutRequest>,
    pub scheduled_at: chrono::DateTime<chrono::Utc>,
}

impl PayoutService {
    pub fn new(db_pool: Arc<Pool>, config: Config) -> Self {
        Self { db_pool, config }
    }

    /// Create a single payout
    pub async fn create_payout(
        &self,
        request: CreatePayoutRequest,
    ) -> Result<Payout, ApiError> {
        let client = self.db_pool.get().await?;

        let payout_id = Uuid::new_v4().to_string();
        let now = Utc::now();
        let scheduled_at = request.scheduled_at.unwrap_or(now);

        let row = client
            .query_one(
                r#"
                INSERT INTO payouts (
                    id, merchant_id, amount, currency, destination_address,
                    status, retry_count, scheduled_at
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                RETURNING id, merchant_id, batch_id, amount, currency, destination_address,
                         status, tx_hash, failure_reason, retry_count, scheduled_at,
                         processed_at, created_at, updated_at
                "#,
                &[
                    &payout_id,
                    &request.merchant_id,
                    &request.amount,
                    &request.currency,
                    &request.destination_address,
                    &"pending".to_string(),
                    &0i32,
                    &scheduled_at,
                ],
            )
            .await?;

        Ok(Payout {
            id: row.get(0),
            merchant_id: row.get(1),
            batch_id: row.get(2),
            amount: row.get(3),
            currency: row.get(4),
            destination_address: row.get(5),
            status: PayoutStatus::from_str(row.get::<_, String>(6)).unwrap(),
            tx_hash: row.get(7),
            failure_reason: row.get(8),
            retry_count: row.get(9),
            scheduled_at: row.get(10),
            processed_at: row.get(11),
            created_at: row.get::<_, chrono::DateTime<chrono::Utc>>(12),
            updated_at: row.get::<_, chrono::DateTime<chrono::Utc>>(13),
        })
    }

    /// Create a batch of payouts
    pub async fn create_payout_batch(
        &self,
        request: CreatePayoutBatchRequest,
    ) -> Result<PayoutBatch, ApiError> {
        let client = self.db_pool.get().await?;

        let batch_id = Uuid::new_v4().to_string();
        let total_amount: i64 = request.payouts.iter().map(|p| p.amount).sum();
        let payout_count = request.payouts.len() as i32;

        // Create batch record
        let batch_row = client
            .query_one(
                r#"
                INSERT INTO payout_batches (
                    id, merchant_id, total_amount, currency, payout_count,
                    status, scheduled_at
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7)
                RETURNING id, merchant_id, total_amount, currency, payout_count,
                         status, scheduled_at, processed_at, created_at, updated_at
                "#,
                &[
                    &batch_id,
                    &request.merchant_id,
                    &total_amount,
                    &request.payouts[0].currency,
                    &payout_count,
                    &"pending".to_string(),
                    &request.scheduled_at,
                ],
            )
            .await?;

        // Create individual payouts linked to batch
        for payout_req in request.payouts {
            let payout_id = Uuid::new_v4().to_string();
            client
                .execute(
                    r#"
                    INSERT INTO payouts (
                        id, merchant_id, batch_id, amount, currency,
                        destination_address, status, retry_count, scheduled_at
                    )
                    VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
                    "#,
                    &[
                        &payout_id,
                        &request.merchant_id,
                        &batch_id,
                        &payout_req.amount,
                        &payout_req.currency,
                        &payout_req.destination_address,
                        &"pending".to_string(),
                        &0i32,
                        &request.scheduled_at,
                    ],
                )
                .await?;
        }

        Ok(PayoutBatch {
            id: batch_row.get(0),
            merchant_id: batch_row.get(1),
            total_amount: batch_row.get(2),
            currency: batch_row.get(3),
            payout_count: batch_row.get(4),
            status: PayoutStatus::from_str(batch_row.get::<_, String>(5)).unwrap(),
            scheduled_at: batch_row.get::<_, chrono::DateTime<chrono::Utc>>(6),
            processed_at: batch_row.get(7),
            created_at: batch_row.get::<_, chrono::DateTime<chrono::Utc>>(8),
            updated_at: batch_row.get::<_, chrono::DateTime<chrono::Utc>>(9),
        })
    }

    /// Get payout by ID
    pub async fn get_payout(&self, payout_id: &str) -> Result<Payout, ApiError> {
        let client = self.db_pool.get().await?;

        let row = client
            .query_one(
                r#"
                SELECT id, merchant_id, batch_id, amount, currency, destination_address,
                       status, tx_hash, failure_reason, retry_count, scheduled_at,
                       processed_at, created_at, updated_at
                FROM payouts WHERE id = $1
                "#,
                &[&payout_id],
            )
            .await
            .map_err(|_| ApiError::NotFound("Payout not found".to_string()))?;

        Ok(Payout {
            id: row.get(0),
            merchant_id: row.get(1),
            batch_id: row.get(2),
            amount: row.get(3),
            currency: row.get(4),
            destination_address: row.get(5),
            status: PayoutStatus::from_str(row.get::<_, String>(6)).unwrap(),
            tx_hash: row.get(7),
            failure_reason: row.get(8),
            retry_count: row.get(9),
            scheduled_at: row.get(10),
            processed_at: row.get(11),
            created_at: row.get::<_, chrono::DateTime<chrono::Utc>>(12),
            updated_at: row.get::<_, chrono::DateTime<chrono::Utc>>(13),
        })
    }

    /// Update payout status
    pub async fn update_payout_status(
        &self,
        payout_id: &str,
        status: PayoutStatus,
        tx_hash: Option<String>,
        failure_reason: Option<String>,
    ) -> Result<(), ApiError> {
        let client = self.db_pool.get().await?;

        let processed_at = if status == PayoutStatus::Completed || status == PayoutStatus::Failed {
            Some(Utc::now())
        } else {
            None
        };

        client
            .execute(
                r#"
                UPDATE payouts
                SET status = $1, tx_hash = $2, failure_reason = $3,
                    processed_at = $4, updated_at = NOW()
                WHERE id = $5
                "#,
                &[
                    &status.to_string(),
                    &tx_hash,
                    &failure_reason,
                    &processed_at,
                    &payout_id,
                ],
            )
            .await?;

        Ok(())
    }

    /// Get scheduled payouts ready for processing
    pub async fn get_scheduled_payouts(&self) -> Result<Vec<Payout>, ApiError> {
        let client = self.db_pool.get().await?;

        let rows = client
            .query(
                r#"
                SELECT id, merchant_id, batch_id, amount, currency, destination_address,
                       status, tx_hash, failure_reason, retry_count, scheduled_at,
                       processed_at, created_at, updated_at
                FROM payouts
                WHERE status IN ('pending', 'scheduled')
                  AND scheduled_at <= NOW()
                ORDER BY scheduled_at ASC
                LIMIT 100
                "#,
                &[],
            )
            .await?;

        Ok(rows
            .into_iter()
            .map(|row| Payout {
                id: row.get(0),
                merchant_id: row.get(1),
                batch_id: row.get(2),
                amount: row.get(3),
                currency: row.get(4),
                destination_address: row.get(5),
                status: PayoutStatus::from_str(row.get::<_, String>(6)).unwrap(),
                tx_hash: row.get(7),
                failure_reason: row.get(8),
                retry_count: row.get(9),
                scheduled_at: row.get(10),
                processed_at: row.get(11),
                created_at: row.get::<_, chrono::DateTime<chrono::Utc>>(12),
                updated_at: row.get::<_, chrono::DateTime<chrono::Utc>>(13),
            })
            .collect())
    }

    /// Retry failed payouts
    pub async fn retry_failed_payout(&self, payout_id: &str) -> Result<(), ApiError> {
        let client = self.db_pool.get().await?;

        client
            .execute(
                r#"
                UPDATE payouts
                SET status = 'pending', retry_count = retry_count + 1,
                    failure_reason = NULL, updated_at = NOW()
                WHERE id = $1 AND retry_count < 3
                "#,
                &[&payout_id],
            )
            .await?;

        Ok(())
    }

    /// Get payout batch by ID
    pub async fn get_payout_batch(&self, batch_id: &str) -> Result<PayoutBatch, ApiError> {
        let client = self.db_pool.get().await?;

        let row = client
            .query_one(
                r#"
                SELECT id, merchant_id, total_amount, currency, payout_count,
                       status, scheduled_at, processed_at, created_at, updated_at
                FROM payout_batches WHERE id = $1
                "#,
                &[&batch_id],
            )
            .await
            .map_err(|_| ApiError::NotFound("Payout batch not found".to_string()))?;

        Ok(PayoutBatch {
            id: row.get(0),
            merchant_id: row.get(1),
            total_amount: row.get(2),
            currency: row.get(3),
            payout_count: row.get(4),
            status: PayoutStatus::from_str(row.get::<_, String>(5)).unwrap(),
            scheduled_at: row.get::<_, chrono::DateTime<chrono::Utc>>(6),
            processed_at: row.get(7),
            created_at: row.get::<_, chrono::DateTime<chrono::Utc>>(8),
            updated_at: row.get::<_, chrono::DateTime<chrono::Utc>>(9),
        })
    }

    /// Create payout reconciliation record
    pub async fn create_payout_reconciliation(
        &self,
        payout_id: &str,
        anchor_tx_id: Option<String>,
    ) -> Result<PayoutReconciliation, ApiError> {
        let client = self.db_pool.get().await?;

        let recon_id = Uuid::new_v4().to_string();

        let row = client
            .query_one(
                r#"
                INSERT INTO payout_reconciliations (
                    id, payout_id, anchor_tx_id, status
                )
                VALUES ($1, $2, $3, $4)
                RETURNING id, payout_id, anchor_tx_id, status, discrepancy,
                         reconciled_at, created_at
                "#,
                &[&recon_id, &payout_id, &anchor_tx_id, &"pending".to_string()],
            )
            .await?;

        Ok(PayoutReconciliation {
            id: row.get(0),
            payout_id: row.get(1),
            anchor_tx_id: row.get(2),
            status: row.get(3),
            discrepancy: row.get(4),
            reconciled_at: row.get(5),
            created_at: row.get::<_, chrono::DateTime<chrono::Utc>>(6),
        })
    }

    /// Update payout reconciliation status
    pub async fn update_payout_reconciliation(
        &self,
        recon_id: &str,
        status: &str,
        discrepancy: Option<String>,
    ) -> Result<(), ApiError> {
        let client = self.db_pool.get().await?;

        let reconciled_at = if status == "reconciled" {
            Some(Utc::now())
        } else {
            None
        };

        client
            .execute(
                r#"
                UPDATE payout_reconciliations
                SET status = $1, discrepancy = $2, reconciled_at = $3
                WHERE id = $4
                "#,
                &[&status, &discrepancy, &reconciled_at, &recon_id],
            )
            .await?;

        Ok(())
    }
}
