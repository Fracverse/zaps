use axum::async_trait;

use crate::{
    api_error::ApiError,
    config::Config,
    models::{BuildTransactionDto, SignedTransactionResponse, TransactionStatus},
};
use base64::{engine::general_purpose, Engine as _};
use serde_json::json;
use std::sync::Arc;

// Mocking Stellar SDK types for now as we don't have the full crate docs loaded
// In a real scenario, these would be imports from a Stellar SDK/crate
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
    fee_payer_signer: Option<CustodialSigner>,
}

#[async_trait]
pub trait TransactionBuilder {
    async fn build_transaction(&self, dto: BuildTransactionDto) -> Result<String, ApiError>; // Returns base64 XDR
}

#[async_trait]
pub trait Signer {
    async fn sign_transaction(&self, tx_xdr: &str) -> Result<String, ApiError>; // Returns signed XDR
}

#[derive(Clone)]
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

        let fee_payer_signer = config
            .stellar_network
            .fee_payer_secret
            .clone()
            .map(|s| CustodialSigner::new(s));

        Self {
            config,
            client,
            fee_payer_signer,
        }
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
        ApiError::InternalServerError
    }

    // Validate asset strings. Accepts "XLM" for native, or "CODE:ISSUER" where ISSUER is a Stellar address
    pub fn validate_asset(&self, asset: &str) -> Result<(), ApiError> {
        if asset == "XLM" {
            return Ok(());
        }

        let parts: Vec<&str> = asset.split(':').collect();
        if parts.len() != 2 {
            return Err(ApiError::Validation(
                "Invalid asset format. Use XLM or CODE:ISSUER".to_string(),
            ));
        }
        let code = parts[0];
        let issuer = parts[1];
        if code.is_empty() || issuer.len() != 56 || !issuer.starts_with('G') {
            return Err(ApiError::Validation(
                "Invalid issued asset, issuer must be a Stellar G... address and code non-empty"
                    .to_string(),
            ));
        }
        Ok(())
    }

    // Build a (mock) payment XDR and return base64 representation. In production this would use a real SDK.
    pub async fn build_payment_xdr(
        &self,
        from: &str,
        to: &str,
        asset: &str,
        amount: i64,
        memo: Option<&str>,
    ) -> Result<String, ApiError> {
        // Validate asset
        self.validate_asset(asset)?;

        let payload = json!({
            "type": "payment",
            "from": from,
            "to": to,
            "asset": asset,
            "amount": amount,
            "memo": memo.unwrap_or("")
        });

        let raw = payload.to_string();
        let encoded = general_purpose::STANDARD.encode(raw.as_bytes());
        Ok(encoded)
    }

    // Simulate a transaction to estimate fee and footprint (mocked)
    pub async fn simulate_transaction(&self, tx_xdr_base64: &str) -> Result<(u32, u32), ApiError> {
        // In production: call /simulate on RPC to get accurate fee/footprint
        // Here, decode and give basic estimates
        if tx_xdr_base64.is_empty() {
            return Err(ApiError::Validation("Empty transaction XDR".to_string()));
        }

        // Simple heuristic: native payments cheaper than issued
        let decoded = general_purpose::STANDARD
            .decode(tx_xdr_base64)
            .map_err(|_| ApiError::Validation("Invalid XDR encoding".to_string()))?;
        let s = String::from_utf8_lossy(&decoded);
        let fee = if s.contains("\"asset\":\"XLM\"") {
            100
        } else {
            200
        };
        let footprint = if s.contains("\"asset\":\"XLM\"") {
            1
        } else {
            2
        };

        Ok((fee, footprint))
    }

    // Sign transaction as fee payer (fee sponsorship) using server-side signer
    pub async fn sign_transaction_as_fee_payer(
        &self,
        tx_xdr_base64: &str,
    ) -> Result<String, ApiError> {
        if self.fee_payer_signer.is_none() {
            return Err(ApiError::Validation(
                "Fee payer not configured on server".to_string(),
            ));
        }
        let signer = self.fee_payer_signer.as_ref().unwrap();
        signer.sign_transaction(tx_xdr_base64).await
    }
}

#[async_trait]
impl TransactionBuilder for SorobanService {
    async fn build_transaction(&self, dto: BuildTransactionDto) -> Result<String, ApiError> {
        // Mock transaction building for contract invocations
        let tx_xdr = format!(
            "mock_xdr_invoke_{}_{}_{:?}",
            dto.contract_id, dto.method, dto.args
        );
        Ok(tx_xdr)
    }
}
