use crate::api_error::ApiError;
use crate::config::Config;
use chrono::{DateTime, Utc};
use deadpool_postgres::Pool;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::info;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BatchStatus {
    Pending,
    Processing,
    Completed,
    Failed,
    PartialFailure,
}

impl std::fmt::Display for BatchStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BatchStatus::Pending => write!(f, "pending"),
            BatchStatus::Processing => write!(f, "processing"),
            BatchStatus::Completed => write!(f, "completed"),
            BatchStatus::Failed => write!(f, "failed"),
            BatchStatus::PartialFailure => write!(f, "partial_failure"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchConfig {
    pub batch_size: usize,
    pub batch_timeout_secs: u64,
    pub max_retries: u32,
}

impl Default for BatchConfig {
    fn default() -> Self {
        Self {
            batch_size: 100,
            batch_timeout_secs: 300,
            max_retries: 3,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentBatch {
    pub id: String,
    pub batch_key: String,
    pub merchant_id: String,
    pub status: String,
    pub total_amount: i64,
    pub total_count: i32,
    pub processed_count: i32,
    pub failed_count: i32,
    pub asset: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchItem {
    pub id: String,
    pub batch_id: String,
    pub payment_id: String,
    pub status: String,
    pub error_message: Option<String>,
    pub retry_count: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchReport {
    pub batch_id: String,
    pub total_items: i32,
    pub processed_items: i32,
    pub failed_items: i32,
    pub success_rate: f64,
    pub total_amount: i64,
    pub status: String,
}

#[derive(Clone)]
pub struct BatchService {
    db_pool: Arc<Pool>,
    config: Config,
}

impl BatchService {
    pub fn new(db_pool: Arc<Pool>, config: Config) -> Self {
        Self { db_pool, config }
    }

    /// Create a new payment batch
    pub async fn create_batch(
        &self,
        merchant_id: &str,
        asset: &str,
        batch_size: usize,
    ) -> Result<PaymentBatch, ApiError> {
        let client = self.db_pool.get().await?;
        let batch_id = Uuid::new_v4().to_string();
        let batch_key = format!("{}-{}", merchant_id, Uuid::new_v4());

        let row = client
            .query_one(
                "INSERT INTO payment_batches (id, batch_key, merchant_id, status, total_amount, total_count, asset)
                 VALUES ($1, $2, $3, $4, $5, $6, $7)
                 RETURNING id, batch_key, merchant_id, status, total_amount, total_count, processed_count, failed_count, asset, created_at, updated_at, completed_at",
                &[
                    &batch_id,
                    &batch_key,
                    &merchant_id,
                    &"pending",
                    &0i64,
                    &(batch_size as i32),
                    &asset,
                ],
            )
            .await?;

        Ok(PaymentBatch {
            id: row.get("id"),
            batch_key: row.get("batch_key"),
            merchant_id: row.get("merchant_id"),
            status: row.get("status"),
            total_amount: row.get("total_amount"),
            total_count: row.get("total_count"),
            processed_count: row.get("processed_count"),
            failed_count: row.get("failed_count"),
            asset: row.get("asset"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
            completed_at: row.get("completed_at"),
        })
    }

    /// Add a payment to a batch
    pub async fn add_payment_to_batch(
        &self,
        batch_id: &str,
        payment_id: &str,
    ) -> Result<BatchItem, ApiError> {
        let client = self.db_pool.get().await?;
        let item_id = Uuid::new_v4().to_string();

        let row = client
            .query_one(
                "INSERT INTO batch_items (id, batch_id, payment_id, status)
                 VALUES ($1, $2, $3, $4)
                 RETURNING id, batch_id, payment_id, status, error_message, retry_count, created_at, updated_at",
                &[&item_id, &batch_id, &payment_id, &"pending"],
            )
            .await?;

        Ok(BatchItem {
            id: row.get("id"),
            batch_id: row.get("batch_id"),
            payment_id: row.get("payment_id"),
            status: row.get("status"),
            error_message: row.get("error_message"),
            retry_count: row.get("retry_count"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
        })
    }

    /// Update batch item status
    pub async fn update_batch_item_status(
        &self,
        item_id: &str,
        status: &str,
        error_message: Option<&str>,
    ) -> Result<(), ApiError> {
        let client = self.db_pool.get().await?;

        client
            .execute(
                "UPDATE batch_items SET status = $1, error_message = $2 WHERE id = $3",
                &[&status, &error_message.unwrap_or(""), &item_id],
            )
            .await?;

        Ok(())
    }

    /// Update batch status
    pub async fn update_batch_status(
        &self,
        batch_id: &str,
        status: &str,
    ) -> Result<(), ApiError> {
        let client = self.db_pool.get().await?;

        let completed_at = if status == "completed" || status == "failed" {
            Some(Utc::now())
        } else {
            None
        };

        client
            .execute(
                "UPDATE payment_batches SET status = $1, completed_at = $2 WHERE id = $3",
                &[&status, &completed_at.map(|dt| dt.to_rfc3339()), &batch_id],
            )
            .await?;

        info!(batch_id = %batch_id, status = %status, "Batch status updated");

        Ok(())
    }

    /// Get batch by ID
    pub async fn get_batch(&self, batch_id: &str) -> Result<Option<PaymentBatch>, ApiError> {
        let client = self.db_pool.get().await?;

        let row = client
            .query_opt(
                "SELECT id, batch_key, merchant_id, status, total_amount, total_count, processed_count, failed_count, asset, created_at, updated_at, completed_at
                 FROM payment_batches WHERE id = $1",
                &[&batch_id],
            )
            .await?;

        Ok(row.map(|r| PaymentBatch {
            id: r.get("id"),
            batch_key: r.get("batch_key"),
            merchant_id: r.get("merchant_id"),
            status: r.get("status"),
            total_amount: r.get("total_amount"),
            total_count: r.get("total_count"),
            processed_count: r.get("processed_count"),
            failed_count: r.get("failed_count"),
            asset: r.get("asset"),
            created_at: r.get("created_at"),
            updated_at: r.get("updated_at"),
            completed_at: r.get("completed_at"),
        }))
    }

    /// Get batch items
    pub async fn get_batch_items(&self, batch_id: &str) -> Result<Vec<BatchItem>, ApiError> {
        let client = self.db_pool.get().await?;

        let rows = client
            .query(
                "SELECT id, batch_id, payment_id, status, error_message, retry_count, created_at, updated_at
                 FROM batch_items WHERE batch_id = $1 ORDER BY created_at ASC",
                &[&batch_id],
            )
            .await?;

        Ok(rows
            .iter()
            .map(|r| BatchItem {
                id: r.get("id"),
                batch_id: r.get("batch_id"),
                payment_id: r.get("payment_id"),
                status: r.get("status"),
                error_message: r.get("error_message"),
                retry_count: r.get("retry_count"),
                created_at: r.get("created_at"),
                updated_at: r.get("updated_at"),
            })
            .collect())
    }

    /// Get batch report
    pub async fn get_batch_report(&self, batch_id: &str) -> Result<Option<BatchReport>, ApiError> {
        let client = self.db_pool.get().await?;

        let row = client
            .query_opt(
                "SELECT id, total_count, processed_count, failed_count, total_amount, status
                 FROM payment_batches WHERE id = $1",
                &[&batch_id],
            )
            .await?;

        Ok(row.map(|r| {
            let total: i32 = r.get("total_count");
            let processed: i32 = r.get("processed_count");
            let failed: i32 = r.get("failed_count");
            let success_rate = if total > 0 {
                (processed as f64 / total as f64) * 100.0
            } else {
                0.0
            };

            BatchReport {
                batch_id: r.get("id"),
                total_items: total,
                processed_items: processed,
                failed_items: failed,
                success_rate,
                total_amount: r.get("total_amount"),
                status: r.get("status"),
            }
        }))
    }

    /// Process batch with retry logic
    pub async fn process_batch_with_retry(
        &self,
        batch_id: &str,
        max_retries: u32,
    ) -> Result<BatchReport, ApiError> {
        let client = self.db_pool.get().await?;

        // Get all pending items
        let rows = client
            .query(
                "SELECT id, retry_count FROM batch_items WHERE batch_id = $1 AND status = 'pending'",
                &[&batch_id],
            )
            .await?;

        let mut processed = 0;
        let mut failed = 0;

        for row in rows {
            let item_id: String = row.get("id");
            let retry_count: i32 = row.get("retry_count");

            if retry_count < max_retries as i32 {
                // Attempt to process
                match self.process_batch_item(&item_id).await {
                    Ok(_) => {
                        self.update_batch_item_status(&item_id, "completed", None)
                            .await?;
                        processed += 1;
                    }
                    Err(e) => {
                        let error_msg = e.to_string();
                        client
                            .execute(
                                "UPDATE batch_items SET retry_count = retry_count + 1, error_message = $1 WHERE id = $2",
                                &[&error_msg, &item_id],
                            )
                            .await?;
                        failed += 1;
                    }
                }
            } else {
                self.update_batch_item_status(&item_id, "failed", Some("Max retries exceeded"))
                    .await?;
                failed += 1;
            }
        }

        // Update batch counts
        client
            .execute(
                "UPDATE payment_batches SET processed_count = processed_count + $1, failed_count = failed_count + $2 WHERE id = $3",
                &[&processed, &failed, &batch_id],
            )
            .await?;

        // Determine final status
        let final_status = if failed == 0 {
            "completed"
        } else if processed > 0 {
            "partial_failure"
        } else {
            "failed"
        };

        self.update_batch_status(batch_id, final_status).await?;

        self.get_batch_report(batch_id)
            .await?
            .ok_or_else(|| ApiError::NotFound("Batch not found".to_string()))
    }

    /// Process individual batch item
    async fn process_batch_item(&self, _item_id: &str) -> Result<(), ApiError> {
        // This would contain the actual payment processing logic
        // For now, it's a placeholder that succeeds
        Ok(())
    }

    /// Get pending batches for a merchant
    pub async fn get_pending_batches(
        &self,
        merchant_id: &str,
    ) -> Result<Vec<PaymentBatch>, ApiError> {
        let client = self.db_pool.get().await?;

        let rows = client
            .query(
                "SELECT id, batch_key, merchant_id, status, total_amount, total_count, processed_count, failed_count, asset, created_at, updated_at, completed_at
                 FROM payment_batches WHERE merchant_id = $1 AND status IN ('pending', 'processing')
                 ORDER BY created_at ASC",
                &[&merchant_id],
            )
            .await?;

        Ok(rows
            .iter()
            .map(|r| PaymentBatch {
                id: r.get("id"),
                batch_key: r.get("batch_key"),
                merchant_id: r.get("merchant_id"),
                status: r.get("status"),
                total_amount: r.get("total_amount"),
                total_count: r.get("total_count"),
                processed_count: r.get("processed_count"),
                failed_count: r.get("failed_count"),
                asset: r.get("asset"),
                created_at: r.get("created_at"),
                updated_at: r.get("updated_at"),
                completed_at: r.get("completed_at"),
            })
            .collect())
    }
}
