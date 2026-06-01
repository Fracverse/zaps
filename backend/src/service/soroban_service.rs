use axum::async_trait;

use crate::{
    api_error::ApiError,
    config::Config,
    models::{BuildTransactionDto, SignedTransactionResponse, TransactionStatus},
};
use base64::{engine::general_purpose, Engine as _};
use reqwest::Client as HttpClient;
use serde_json::{json, Value as JsonValue};
use std::sync::Arc;
use tokio::time::{sleep, Duration};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use ed25519_dalek::{Keypair, PublicKey, SecretKey, Signer};
use hex as hex_crate;

// Mocking Stellar SDK types for now as we don't have the full crate docs loaded
// In a real scenario, these would be imports from a Stellar SDK/crate
pub struct StellarClient {
    pub network_passphrase: String,
    pub rpc_url: String,
    http: HttpClient,
}

impl StellarClient {
    pub fn new(network_passphrase: String, rpc_url: String) -> Self {
        Self {
            network_passphrase,
            rpc_url,
            http: HttpClient::new(),
        }
    }

    /// Submit a signed transaction XDR (base64) to the Soroban RPC using
    /// the JSON-RPC `send_transaction` method. Returns the transaction hash
    /// on success or an error message.
    pub async fn submit_transaction(&self, tx_envelope: &str) -> Result<String, String> {
        let body = json!({
            "jsonrpc": "2.0",
            "id": "1",
            "method": "send_transaction",
            "params": [tx_envelope]
        });

        let res = self
            .http
            .post(&self.rpc_url)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("rpc request failed: {}", e))?;

        let status = res.status();
        let text = res
            .text()
            .await
            .map_err(|e| format!("reading response failed: {}", e))?;

        if !status.is_success() {
            return Err(format!("rpc returned {}: {}", status, text));
        }

        let v: JsonValue = serde_json::from_str(&text)
            .map_err(|e| format!("invalid json from rpc: {} -- {}", e, text))?;

        if let Some(err) = v.get("error") {
            return Err(format!("rpc error: {}", err));
        }

        // Try to extract a hash from result
        if let Some(hash) = v.get("result").and_then(|r| r.get("hash")).and_then(|h| h.as_str()) {
            return Ok(hash.to_string());
        }

        // Fallback: sometimes result may directly be a string hash
        if let Some(s) = v.get("result").and_then(|r| r.as_str()) {
            return Ok(s.to_string());
        }

        Err(format!("unexpected rpc response: {}", text))
    }

    /// Simulate a transaction via JSON-RPC `simulate_transaction`.
    /// Returns the raw simulation JSON on success.
    pub async fn simulate_transaction(&self, tx_envelope: &str) -> Result<JsonValue, String> {
        let body = json!({
            "jsonrpc": "2.0",
            "id": "1",
            "method": "simulate_transaction",
            "params": [tx_envelope, {"latestLedger":"true"}]
        });

        let res = self
            .http
            .post(&self.rpc_url)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("rpc request failed: {}", e))?;

        let status = res.status();
        let text = res
            .text()
            .await
            .map_err(|e| format!("reading response failed: {}", e))?;

        if !status.is_success() {
            return Err(format!("rpc returned {}: {}", status, text));
        }

        let v: JsonValue = serde_json::from_str(&text)
            .map_err(|e| format!("invalid json from rpc: {} -- {}", e, text))?;

        if let Some(err) = v.get("error") {
            return Err(format!("rpc error: {}", err));
        }

        Ok(v)
    }

    /// Query transaction status via JSON-RPC `get_transaction` and return raw JSON.
    pub async fn get_transaction(&self, hash: &str) -> Result<JsonValue, String> {
        let body = json!({
            "jsonrpc": "2.0",
            "id": "1",
            "method": "get_transaction",
            "params": [hash]
        });

        let res = self
            .http
            .post(&self.rpc_url)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("rpc request failed: {}", e))?;

        let status = res.status();
        let text = res
            .text()
            .await
            .map_err(|e| format!("reading response failed: {}", e))?;

        if !status.is_success() {
            return Err(format!("rpc returned {}: {}", status, text));
        }

        let v: JsonValue = serde_json::from_str(&text)
            .map_err(|e| format!("invalid json from rpc: {} -- {}", e, text))?;

        if let Some(err) = v.get("error") {
            return Err(format!("rpc error: {}", err));
        }

        Ok(v)
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
        // Basic sponsorship wrapper: if the incoming payload is base64-encoded
        // JSON (our builder returns base64 JSON), decode it and wrap it in a
        // sponsored envelope signed with an HMAC using the configured secret.
        // This avoids adding heavy XDR handling while providing a deterministic
        // server-side sponsorship artifact that can be submitted or returned
        // to clients.
        let decoded = match general_purpose::STANDARD.decode(tx_xdr) {
            Ok(bytes) => String::from_utf8_lossy(&bytes).to_string(),
            Err(_) => tx_xdr.to_string(),
        };

        let decoded_bytes = decoded.into_bytes();

        // Try hex seed ed25519 signing (32-byte seed in hex). If the
        // configured secret parses as hex 32 bytes, perform ed25519
        // signing and return envelope with `ed25519` signature. Otherwise
        // fallback to HMAC-based sponsorship.
        if let Ok(seed) = hex_crate::decode(&self.secret_key) {
            if seed.len() == 32 {
                let secret = SecretKey::from_bytes(&seed).map_err(|_| ApiError::InternalServerError)?;
                let public = PublicKey::from(&secret);
                let keypair = Keypair { secret, public };

                let sig = keypair.sign(&decoded_bytes);
                let sig_b64 = general_purpose::STANDARD.encode(sig.to_bytes());

                let envelope = json!({
                    "type": "sponsored_transaction",
                    "sponsored_by": "fee_payer_custodial",
                    "signature_type": "ed25519",
                    "envelope": general_purpose::STANDARD.encode(&decoded_bytes),
                    "signature": sig_b64,
                    "pubkey": general_purpose::STANDARD.encode(public.as_bytes()),
                });

                let raw = envelope.to_string();
                let out = general_purpose::STANDARD.encode(raw.as_bytes());
                return Ok(out);
            }
        }

        // Fallback HMAC-SHA256
        type HmacSha256 = Hmac<Sha256>;
        let mut mac = HmacSha256::new_from_slice(self.secret_key.as_bytes())
            .map_err(|_| ApiError::InternalServerError)?;
        mac.update(&decoded_bytes);
        let result = mac.finalize();
        let sig_bytes = result.into_bytes();
        let sig_b64 = general_purpose::STANDARD.encode(&sig_bytes);

        let envelope = json!({
            "type": "sponsored_transaction",
            "sponsored_by": "fee_payer_custodial",
            "signature_type": "hmac-sha256",
            "envelope": general_purpose::STANDARD.encode(&decoded_bytes),
            "signature": sig_b64,
        });

        let raw = envelope.to_string();
        let out = general_purpose::STANDARD.encode(raw.as_bytes());
        Ok(out)
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
        // Submit to RPC
        let hash = self
            .client
            .submit_transaction(&signed_tx_xdr)
            .await
            .map_err(|e| self.normalize_error(e))?;

        // Poll for final status with backoff
        let mut attempts = 0u32;
        loop {
            attempts += 1;
            // Query transaction status
            match self.client.get_transaction(&hash).await {
                Ok(json) => {
                    // Try to interpret known status fields
                    if let Some(result) = json.get("result") {
                        // Many implementations return `status` or nested status
                        if let Some(status_str) = result.get("status").and_then(|s| s.as_str()) {
                            match status_str {
                                "NOT_FOUND" | "not_found" => {
                                    // keep polling
                                }
                                "SUCCESS" | "success" | "CONFIRMED" | "confirmed" => {
                                    return Ok(SignedTransactionResponse {
                                        tx_hash: hash,
                                        status: TransactionStatus::CONFIRMED,
                                    });
                                }
                                "FAILED" | "failed" => {
                                    return Ok(SignedTransactionResponse {
                                        tx_hash: hash,
                                        status: TransactionStatus::FAILED,
                                    });
                                }
                                _ => {
                                    // Unexpected, continue polling a few times
                                }
                            }
                        } else if let Some(status_val) = result.get("status") {
                            // fallback: numeric or other
                            let s = status_val.to_string();
                            if s.to_lowercase().contains("failed") {
                                return Ok(SignedTransactionResponse {
                                    tx_hash: hash,
                                    status: TransactionStatus::FAILED,
                                });
                            }
                        }
                    }
                }
                Err(_) => {
                    // treat RPC read errors as transient and retry
                }
            }

            if attempts > 30 {
                // timeout
                return Ok(SignedTransactionResponse {
                    tx_hash: hash,
                    status: TransactionStatus::PENDING,
                });
            }

            sleep(Duration::from_millis(1000)).await;
        }
    }

    fn normalize_error(&self, raw: String) -> ApiError {
        // Try to parse structured RPC JSON first
        if let Ok(json): Result<serde_json::Value, _> = serde_json::from_str(&raw) {
            // JSON-RPC error object
            if let Some(err) = json.get("error") {
                // Try to extract a message/code
                let msg = if let Some(m) = err.get("message") {
                    m.as_str().unwrap_or(&err.to_string()).to_string()
                } else {
                    err.to_string()
                };

                let lower = msg.to_lowercase();
                if lower.contains("validation") || lower.contains("invalid") || lower.contains("bad request") {
                    return ApiError::Validation(msg);
                }
                return ApiError::Stellar(msg);
            }

            // Some RPCs place error info inside result.error
            if let Some(result) = json.get("result") {
                if let Some(err) = result.get("error") {
                    let msg = err.as_str().unwrap_or(&err.to_string()).to_string();
                    let lower = msg.to_lowercase();
                    if lower.contains("validation") || lower.contains("invalid") || lower.contains("bad request") {
                        return ApiError::Validation(msg);
                    }
                    return ApiError::Stellar(msg);
                }
            }
        }

        // Fallback: simple substring mapping on raw text
        let lower = raw.to_lowercase();
        if lower.contains("validation") || lower.contains("invalid") || lower.contains("bad request") {
            return ApiError::Validation(raw);
        }
        if lower.contains("stellar") || lower.contains("soroban") || lower.contains("rpc") || lower.contains("tx failed") {
            return ApiError::Stellar(raw);
        }

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
        // Try to call the RPC simulate endpoint for a better estimate.
        if tx_xdr_base64.is_empty() {
            return Err(ApiError::Validation("Empty transaction XDR".to_string()));
        }

        match self.client.simulate_transaction(tx_xdr_base64).await {
            Ok(json) => {
                // Parse minResourceFee if present
                let mut fee: u32 = 0;
                let mut footprint: u32 = 0;

                if let Some(result) = json.get("result") {
                    if let Some(min_fee) = result.get("minResourceFee").and_then(|v| v.as_str()) {
                        if let Ok(parsed) = min_fee.parse::<u32>() {
                            fee = parsed;
                        }
                    }

                    if let Some(txdata) = result.get("transactionData") {
                        // footprint size heuristic
                        if txdata.get("instructions").is_some() {
                            footprint = 1;
                        } else {
                            footprint = 1;
                        }
                    }
                }

                if fee == 0 {
                    fee = 100;
                }
                if footprint == 0 {
                    footprint = 1;
                }

                Ok((fee, footprint))
            }
            Err(e) => Err(self.normalize_error(e)),
        }
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

    /// Simulate a read-only contract call and return the raw simulation `retval` JSON if present.
    pub async fn simulate_contract_read(
        &self,
        contract_id: &str,
        method: &str,
        args: Vec<serde_json::Value>,
    ) -> Result<Option<serde_json::Value>, ApiError> {
        // Build the contract invocation payload (base64) using our builder
        let dto = BuildTransactionDto {
            contract_id: contract_id.to_string(),
            method: method.to_string(),
            args,
        };

        let tx_xdr = self.build_transaction(dto).await?;

        let sim_json = self
            .client
            .simulate_transaction(&tx_xdr)
            .await
            .map_err(|e| self.normalize_error(e))?;

        if let Some(result) = sim_json.get("result") {
            if let Some(error) = result.get("error") {
                return Err(ApiError::Stellar(format!("simulation error: {}", error)));
            }

            if let Some(retval) = result.get("retval") {
                return Ok(Some(retval.clone()));
            }
        }

        Ok(None)
    }
}

#[async_trait]
impl TransactionBuilder for SorobanService {
    async fn build_transaction(&self, dto: BuildTransactionDto) -> Result<String, ApiError> {
        // Build a lightweight JSON representation of the contract invocation
        // and return it as base64 so callers (clients) receive an opaque
        // payload they can simulate/sign. This mirrors the earlier mock
        // but provides structured data for potential RPC simulation.
        let payload = json!({
            "type": "contract_invoke",
            "contract_id": dto.contract_id,
            "method": dto.method,
            "args": dto.args,
        });

        let raw = payload.to_string();
        let encoded = general_purpose::STANDARD.encode(raw.as_bytes());
        Ok(encoded)
    }
}
