use axum::async_trait;

use crate::{
    api_error::ApiError,
    config::Config,
    models::{BuildTransactionDto, SignedTransactionResponse, TransactionStatus},
};
use std::sync::Arc;

// Mocking Stellar SDK types for now as we don't have the full crate docs loaded
// In a real scenario, these would be imports from stellar-sdk
pub struct StellarClient {
    pub network_passphrase: String,
    pub rpc_url: String,
}

impl StellarClient {
    pub fn new(network_passphrase: String, rpc_url: String) -> Self {
        Self {
            network_passphrase,
            rpc_url,
        }
    }

    pub async fn submit_transaction(&self, _tx_envelope: &str) -> Result<String, String> {
        // Mock submission
        Ok("mock_tx_hash".to_string())
    }
}

#[derive(Clone)]
pub struct SorobanService {
    config: Config,
    client: Arc<StellarClient>,
}

#[async_trait]
pub trait TransactionBuilder {
    async fn build_transaction(&self, dto: BuildTransactionDto) -> Result<String, ApiError>; // Returns base64 XDR
}

#[async_trait]
pub trait Signer {
    async fn sign_transaction(&self, tx_xdr: &str) -> Result<String, ApiError>; // Returns signed XDR
}

pub struct CustodialSigner {
    pub secret_key: String,
}

impl CustodialSigner {
    pub fn new(secret_key: String) -> Self {
        Self { secret_key }
    }
}

#[async_trait]
impl Signer for CustodialSigner {
    async fn sign_transaction(&self, tx_xdr: &str) -> Result<String, ApiError> {
        // Mock signing logic
        // In reality: Parse XDR, sign with key, return new XDR
        Ok(format!("{}_signed_by_custodial", tx_xdr))
    }
}

impl SorobanService {
    pub fn new(config: Config) -> Self {
        let client = Arc::new(StellarClient::new(
            config.stellar_network.passphrase.clone(),
            config.stellar_network.rpc_url.clone(),
        ));
        Self { config, client }
    }

    pub fn get_network_config(&self) -> &crate::config::StellarNetwork {
        &self.config.stellar_network
    }

    pub async fn submit_transaction(
        &self,
        signed_tx_xdr: String,
    ) -> Result<SignedTransactionResponse, ApiError> {
        match self.client.submit_transaction(&signed_tx_xdr).await {
            Ok(hash) => Ok(SignedTransactionResponse {
                tx_hash: hash,
                status: TransactionStatus::PENDING,
            }),
            Err(e) => Err(self.normalize_error(e)),
        }
    }

    fn normalize_error(&self, _: String) -> ApiError {
        // Normalize Soroban/Stellar errors into ApiError
        // This is a basic implementation
        ApiError::InternalServerError
    }
}

#[async_trait]
impl TransactionBuilder for SorobanService {
    async fn build_transaction(&self, dto: BuildTransactionDto) -> Result<String, ApiError> {
        // Mock transaction building
        // In reality: Use SDK to build InvokeHostFunctionOp
        let tx_xdr = format!(
            "mock_xdr_invoke_{}_{}_{:?}",
            dto.contract_id, dto.method, dto.args
        );
        Ok(tx_xdr)
    }
}
