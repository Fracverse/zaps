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
            id: tx_id,
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
                    &bridge_tx.status.to_string(),
                    &bridge_tx.created_at,
                    &bridge_tx.updated_at,
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

    fn validate_bridge_request(&self, request: &BridgeTransferRequest) -> Result<(), ApiError> {
        // Validate supported assets
        if !self
            .config
            .bridge_config
            .supported_assets
            .contains(&request.asset)
        {
            return Err(ApiError::Validation("Asset not supported".to_string()));
        }

        // Validate amounts
        if request.amount < self.config.bridge_config.min_bridge_amount
            || request.amount > self.config.bridge_config.max_bridge_amount
        {
            return Err(ApiError::Validation("Invalid amount".to_string()));
        }

        Ok(())
    }
}
