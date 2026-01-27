use crate::{
    api_error::ApiError,
    config::Config,
    models::{BridgeTransaction, BridgeTransactionStatus},
};
use deadpool_postgres::Pool;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone)]
pub struct BridgeService {
    db_pool: Arc<Pool>,
    config: Config,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BridgeTransferRequest {
    pub from_chain: String,
    pub to_chain: String,
    pub asset: String,
    pub amount: u64,
    pub destination_address: String,
    pub user_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BridgeTransactionResponse {
    pub id: Uuid,
    pub from_chain: String,
    pub to_chain: String,
    pub asset: String,
    pub amount: u64,
    pub status: BridgeTransactionStatus,
    pub tx_hash: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl BridgeService {
    pub fn new(db_pool: Arc<Pool>, config: Config) -> Self {
        Self { db_pool, config }
    }

    pub async fn initiate_bridge_transfer(
        &self,
        request: BridgeTransferRequest,
    ) -> Result<BridgeTransactionResponse, ApiError> {
        // Validate bridge configuration
        self.validate_bridge_request(&request)?;

        let client = self.db_pool.get().await?;

        // In production, this would interact with actual bridge contracts
        // In production, this would interact with actual bridge contracts
        // For now, we'll simulate the bridge transaction
        let tx_id = Uuid::new_v4();
        let bridge_tx = BridgeTransaction {
            id: tx_id.to_string(),
            from_chain: request.from_chain.clone(),
            to_chain: request.to_chain.clone(),
            asset: request.asset.clone(),
            amount: request.amount,
            destination_address: request.destination_address.clone(),
            user_id: request.user_id.clone(),
            status: BridgeTransactionStatus::Pending,
            tx_hash: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        // Store in database (we'll need to create a bridge_transactions table)
        client
            .execute(
                r#"
                INSERT INTO bridge_transactions (
                    id, from_chain, to_chain, asset, amount,
                    destination_address, user_id, status, created_at, updated_at
                ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
                "#,
                &[
                    &tx_id,
                    &bridge_tx.from_chain,
                    &bridge_tx.to_chain,
                    &bridge_tx.asset,
                    &(bridge_tx.amount as i64),
                    &bridge_tx.destination_address,
                    &bridge_tx.user_id,
                    &bridge_tx.status.to_string_lose(),
                    &bridge_tx.created_at.naive_utc(),
                    &bridge_tx.updated_at.naive_utc(),
                ],
            )
            .await?;

        Ok(BridgeTransactionResponse {
            id: tx_id,
            from_chain: bridge_tx.from_chain,
            to_chain: bridge_tx.to_chain,
            asset: bridge_tx.asset,
            amount: bridge_tx.amount,
            status: bridge_tx.status,
            tx_hash: bridge_tx.tx_hash,
            created_at: bridge_tx.created_at,
        })
    }

    pub async fn get_bridge_transaction(
        &self,
        id: Uuid,
    ) -> Result<BridgeTransactionResponse, ApiError> {
        let client = self.db_pool.get().await?;

        let row = client
            .query_one(
                r#"
                SELECT id, from_chain, to_chain, asset, amount, destination_address,
                       user_id, status, tx_hash, created_at, updated_at
                FROM bridge_transactions WHERE id = $1
                "#,
                &[&id],
            )
            .await
            .map_err(|_| ApiError::NotFound("Bridge transaction not found".to_string()))?;

        Ok(BridgeTransactionResponse {
            id: row.get(0),
            from_chain: row.get(1),
            to_chain: row.get(2),
            asset: row.get(3),
            amount: row.get::<_, i64>(4) as u64,
            status: BridgeTransactionStatus::from_string(row.get(7)),
            tx_hash: row.get(8),
            created_at: row.get(9),
        })
    }

    pub async fn confirm_bridge_transaction(
        &self,
        id: Uuid,
        tx_hash: String,
    ) -> Result<(), ApiError> {
        let client = self.db_pool.get().await?;

        client
            .execute(
                "UPDATE bridge_transactions SET status = $1, tx_hash = $2, updated_at = NOW() WHERE id = $3",
                &[&BridgeTransactionStatus::Completed.to_string_lose(), &tx_hash, &id],
            )
            .await?;

        Ok(())
    }

    pub async fn get_supported_assets(&self) -> Vec<String> {
        self.config.bridge_config.supported_assets.clone()
    }

    fn validate_bridge_request(&self, request: &BridgeTransferRequest) -> Result<(), ApiError> {
        let bridge_config = &self.config.bridge_config;

        // Check if asset is supported
        if !bridge_config.supported_assets.contains(&request.asset) {
            return Err(ApiError::Validation(format!(
                "Asset {} is not supported for bridging",
                request.asset
            )));
        }

        // Check amount limits
        if request.amount < bridge_config.min_bridge_amount {
            return Err(ApiError::Validation(format!(
                "Amount {} is below minimum bridge amount {}",
                request.amount, bridge_config.min_bridge_amount
            )));
        }

        if request.amount > bridge_config.max_bridge_amount {
            return Err(ApiError::Validation(format!(
                "Amount {} exceeds maximum bridge amount {}",
                request.amount, bridge_config.max_bridge_amount
            )));
        }

        // Validate chains
        let supported_chains = ["ethereum", "polygon", "bsc", "stellar"];
        if !supported_chains.contains(&request.from_chain.as_str()) {
            return Err(ApiError::Validation(format!(
                "Unsupported source chain: {}",
                request.from_chain
            )));
        }

        if request.to_chain != "stellar" {
            return Err(ApiError::Validation(
                "Only bridging to Stellar is currently supported".to_string(),
            ));
        }

        Ok(())
    }
}
