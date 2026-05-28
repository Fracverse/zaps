use crate::api_error::ApiError;
use crate::config::Config;
use crate::service::MetricsService;
use chrono::{DateTime, Utc};
use deadpool_postgres::Pool;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{error, info, warn};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StellarEvent {
    pub id: String,
    pub event_type: String,
    pub tx_hash: String,
    pub ledger_sequence: i64,
    pub source_account: String,
    pub destination_account: Option<String>,
    pub asset_code: Option<String>,
    pub amount: Option<i64>,
    pub status: String,
    pub raw_data: String,
    pub indexed_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HorizonTransaction {
    pub id: String,
    pub hash: String,
    pub ledger: i64,
    pub created_at: String,
    pub source_account: String,
    pub operations_count: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HorizonOperation {
    pub id: String,
    pub transaction_hash: String,
    pub operation_type: String,
    pub source_account: String,
    pub destination: Option<String>,
    pub asset_code: Option<String>,
    pub amount: Option<String>,
}

#[derive(Clone)]
pub struct IndexerService {
    db_pool: Arc<Pool>,
    config: Config,
    http_client: Client,
}

impl IndexerService {
    pub fn new(db_pool: Arc<Pool>, config: Config) -> Self {
        Self {
            db_pool,
            config,
            http_client: Client::new(),
        }
    }

    /// Start real-time indexing from Stellar Horizon
    pub async fn start_indexing(&self) -> Result<(), ApiError> {
        info!("Starting Stellar Horizon event indexing");

        // Get the latest ledger sequence from Horizon
        let latest_ledger = self.get_latest_ledger().await?;
        info!(ledger = %latest_ledger, "Starting indexing from ledger");

        // Store the starting ledger in the database
        self.store_indexer_state("latest_ledger", &latest_ledger.to_string())
            .await?;

        Ok(())
    }

    /// Index a specific transaction from Stellar
    pub async fn index_transaction(&self, tx_hash: &str) -> Result<(), ApiError> {
        info!(tx_hash = %tx_hash, "Indexing transaction");

        // Fetch transaction from Horizon
        let tx = self.fetch_transaction_from_horizon(tx_hash).await?;

        // Fetch operations for this transaction
        let operations = self.fetch_operations_for_transaction(tx_hash).await?;

        // Store transaction in database
        self.store_transaction(&tx, &operations).await?;

        info!(tx_hash = %tx_hash, "Transaction indexed successfully");

        Ok(())
    }

    /// Fetch latest ledger sequence from Horizon
    async fn get_latest_ledger(&self) -> Result<i64, ApiError> {
        let url = format!("{}/ledgers?limit=1&order=desc", self.config.stellar_network.horizon_url);

        let response = self
            .http_client
            .get(&url)
            .send()
            .await
            .map_err(|e| ApiError::Stellar(format!("Failed to fetch latest ledger: {}", e)))?;

        let body: serde_json::Value = response
            .json()
            .await
            .map_err(|e| ApiError::Stellar(format!("Failed to parse ledger response: {}", e)))?;

        body["_embedded"]["records"][0]["sequence"]
            .as_i64()
            .ok_or_else(|| ApiError::Stellar("Invalid ledger sequence in response".to_string()))
    }

    /// Fetch transaction details from Horizon
    async fn fetch_transaction_from_horizon(&self, tx_hash: &str) -> Result<HorizonTransaction, ApiError> {
        let url = format!(
            "{}/transactions/{}",
            self.config.stellar_network.horizon_url, tx_hash
        );

        let response = self
            .http_client
            .get(&url)
            .send()
            .await
            .map_err(|e| ApiError::Stellar(format!("Failed to fetch transaction: {}", e)))?;

        let body: serde_json::Value = response
            .json()
            .await
            .map_err(|e| ApiError::Stellar(format!("Failed to parse transaction: {}", e)))?;

        Ok(HorizonTransaction {
            id: body["id"].as_str().unwrap_or("").to_string(),
            hash: body["hash"].as_str().unwrap_or("").to_string(),
            ledger: body["ledger"].as_i64().unwrap_or(0),
            created_at: body["created_at"].as_str().unwrap_or("").to_string(),
            source_account: body["source_account"].as_str().unwrap_or("").to_string(),
            operations_count: body["operation_count"].as_i64().unwrap_or(0) as i32,
        })
    }

    /// Fetch operations for a transaction from Horizon
    async fn fetch_operations_for_transaction(
        &self,
        tx_hash: &str,
    ) -> Result<Vec<HorizonOperation>, ApiError> {
        let url = format!(
            "{}/transactions/{}/operations",
            self.config.stellar_network.horizon_url, tx_hash
        );

        let response = self
            .http_client
            .get(&url)
            .send()
            .await
            .map_err(|e| ApiError::Stellar(format!("Failed to fetch operations: {}", e)))?;

        let body: serde_json::Value = response
            .json()
            .await
            .map_err(|e| ApiError::Stellar(format!("Failed to parse operations: {}", e)))?;

        let mut operations = Vec::new();

        if let Some(records) = body["_embedded"]["records"].as_array() {
            for record in records {
                operations.push(HorizonOperation {
                    id: record["id"].as_str().unwrap_or("").to_string(),
                    transaction_hash: record["transaction_hash"].as_str().unwrap_or("").to_string(),
                    operation_type: record["type"].as_str().unwrap_or("").to_string(),
                    source_account: record["source_account"].as_str().unwrap_or("").to_string(),
                    destination: record["destination"].as_str().map(|s| s.to_string()),
                    asset_code: record["asset_code"].as_str().map(|s| s.to_string()),
                    amount: record["amount"].as_str().map(|s| s.to_string()),
                });
            }
        }

        Ok(operations)
    }

    /// Store transaction and operations in database
    async fn store_transaction(
        &self,
        tx: &HorizonTransaction,
        operations: &[HorizonOperation],
    ) -> Result<(), ApiError> {
        let client = self.db_pool.get().await?;
        let event_id = Uuid::new_v4().to_string();

        for op in operations {
            let amount = op
                .amount
                .as_ref()
                .and_then(|a| a.parse::<i64>().ok())
                .unwrap_or(0);

            client
                .execute(
                    "INSERT INTO stellar_events (id, event_type, tx_hash, ledger_sequence, source_account, destination_account, asset_code, amount, status, raw_data)
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
                     ON CONFLICT (tx_hash) DO NOTHING",
                    &[
                        &event_id,
                        &op.operation_type,
                        &tx.hash,
                        &tx.ledger,
                        &op.source_account,
                        &op.destination.as_ref().unwrap_or(&"".to_string()),
                        &op.asset_code.as_ref().unwrap_or(&"".to_string()),
                        &amount,
                        &"confirmed",
                        &serde_json::to_string(&op).unwrap_or_default(),
                    ],
                )
                .await?;
        }

        info!(tx_hash = %tx.hash, op_count = %operations.len(), "Transaction stored");

        Ok(())
    }

    /// Store indexer state (e.g., latest processed ledger)
    async fn store_indexer_state(&self, key: &str, value: &str) -> Result<(), ApiError> {
        let client = self.db_pool.get().await?;

        client
            .execute(
                "INSERT INTO indexer_state (key, value) VALUES ($1, $2)
                 ON CONFLICT (key) DO UPDATE SET value = $2, updated_at = NOW()",
                &[&key, &value],
            )
            .await?;

        Ok(())
    }

    /// Get indexer state
    pub async fn get_indexer_state(&self, key: &str) -> Result<Option<String>, ApiError> {
        let client = self.db_pool.get().await?;

        let row = client
            .query_opt(
                "SELECT value FROM indexer_state WHERE key = $1",
                &[&key],
            )
            .await?;

        Ok(row.map(|r| r.get("value")))
    }

    /// Process payment events from Stellar
    pub async fn process_payment_events(&self) -> Result<u64, ApiError> {
        let client = self.db_pool.get().await?;

        // Get unprocessed payment events
        let rows = client
            .query(
                "SELECT id, tx_hash, source_account, destination_account, amount, asset_code
                 FROM stellar_events WHERE event_type = 'payment' AND status = 'confirmed' AND processed = false
                 LIMIT 100",
                &[],
            )
            .await?;

        let mut processed = 0;

        for row in rows {
            let event_id: String = row.get("id");
            let tx_hash: String = row.get("tx_hash");
            let source: String = row.get("source_account");
            let destination: Option<String> = row.get("destination_account");
            let _amount: i64 = row.get("amount");
            let _asset: Option<String> = row.get("asset_code");

            // Update corresponding payment record if it exists
            if let Some(_dest) = destination {
                // Find the most recent matching payment and update it by id to avoid UPDATE ... LIMIT
                let payment_row = client
                    .query_opt(
                        "SELECT id, merchant_id, send_asset, send_amount, created_at
                         FROM payments
                         WHERE from_address = $1 AND status IN ('pending','processing')
                         ORDER BY created_at DESC
                         LIMIT 1",
                        &[&source],
                    )
                    .await?;

                if let Some(payment_row) = payment_row {
                    let payment_id: String = payment_row.get("id");
                    let merchant_id: String = payment_row.get("merchant_id");
                    let send_asset: String = payment_row.get("send_asset");
                    let send_amount: i64 = payment_row.get("send_amount");
                    let created_at: chrono::DateTime<chrono::Utc> = payment_row.get("created_at");

                    let update_result = client
                        .execute(
                            "UPDATE payments SET tx_hash = $1, status = 'completed', updated_at = NOW()
                             WHERE id = $2",
                            &[&tx_hash, &payment_id],
                        )
                        .await?;

                    if update_result == 0 {
                        continue;
                    }

                    // Mark event as processed
                    client
                        .execute(
                            "UPDATE stellar_events SET processed = true WHERE id = $1",
                            &[&event_id],
                        )
                        .await?;

                    processed += 1;
                    let duration_secs =
                        (chrono::Utc::now() - created_at).num_milliseconds() as f64 / 1000.0;
                    MetricsService::record_payment_transaction(
                        &merchant_id,
                        "completed",
                        "stellar_indexer",
                        &send_asset,
                        send_amount,
                    );
                    MetricsService::record_payment_processing_duration(
                        "stellar_indexer",
                        "completed",
                        duration_secs.max(0.0),
                    );
                    info!(tx_hash = %tx_hash, "Payment event processed");
                }
            }
        }

        Ok(processed)
    }

    /// Handle reconnection with exponential backoff
    pub async fn reconnect_with_backoff(&self, max_retries: u32) -> Result<(), ApiError> {
        let mut retry_count = 0;
        let mut backoff_ms = 1000u64;

        loop {
            match self.start_indexing().await {
                Ok(_) => {
                    info!("Reconnected to Stellar Horizon");
                    return Ok(());
                }
                Err(e) => {
                    retry_count += 1;
                    if retry_count >= max_retries {
                        error!("Max retries exceeded for Horizon connection");
                        return Err(e);
                    }

                    warn!(
                        retry_count = %retry_count,
                        backoff_ms = %backoff_ms,
                        "Reconnection attempt failed, retrying..."
                    );

                    tokio::time::sleep(tokio::time::Duration::from_millis(backoff_ms)).await;
                    backoff_ms = std::cmp::min(backoff_ms * 2, 60000); // Cap at 60 seconds
                }
            }
        }
    }
}
