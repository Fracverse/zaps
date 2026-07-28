use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;
use stellar_base::{
    amount::Stroops,
    network::Network,
    operations::Operation,
    transaction::{Transaction, MIN_BASE_FEE},
    xdr::XDRSerialize,
    Asset, PublicKey,
};

// Stellar/Soroban Horizon & RPC operations client stub
// This client interacts with Stellar RPC nodes and Horizon endpoints.

pub struct StellarClient {
    pub rpc_url: String,
    pub http_client: reqwest::Client,
}

impl StellarClient {
    pub fn new(rpc_url: String) -> Self {
        Self {
            rpc_url,
            http_client: reqwest::Client::new(),
        }
    }

    /// Send an RPC request with retry mechanism (BE-042)
    pub async fn send_rpc_request(
        &self,
        method: &str,
        params: Value,
    ) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        let max_attempts = 3;
        let mut attempts = 0;

        loop {
            attempts += 1;

            let payload = json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": method,
                "params": params
            });

            let response = self
                .http_client
                .post(&self.rpc_url)
                .json(&payload)
                .send()
                .await;

            match response {
                Ok(resp) => {
                    let status = resp.status();
                    if status.is_success() {
                        let json_resp: Value = resp.json().await?;
                        return Ok(json_resp);
                    } else if status.as_u16() == 503
                        || status.as_u16() == 504
                        || status.as_u16() == 429
                    {
                        // Server errors / rate limits, eligible for retry
                    } else {
                        return Err(format!("RPC call failed with status: {}", status).into());
                    }
                }
                Err(e) => {
                    if e.is_timeout() || e.is_connect() {
                        // Network timeout or connect errors, eligible for retry
                    } else {
                        return Err(e.into());
                    }
                }
            }

            if attempts >= max_attempts {
                return Err("Max retry attempts reached".into());
            }

            // Exponential backoff
            tokio::time::sleep(Duration::from_secs(2_u64.pow(attempts - 1))).await;
        }
    }

    /// Simulate a transaction on Soroban RPC to estimate gas and footprint (BE-041)
    pub async fn simulate_transaction(
        &self,
        tx_envelope: &str,
    ) -> Result<SimulateTransactionResponse, Box<dyn std::error::Error + Send + Sync>> {
        let params = json!({
            "transaction": tx_envelope
        });

        let response = self.send_rpc_request("simulateTransaction", params).await?;

        if let Some(error) = response.get("error") {
            return Err(format!("RPC error: {}", error).into());
        }

        if let Some(result) = response.get("result") {
            let sim_response: SimulateTransactionResponse = serde_json::from_value(result.clone())?;
            return Ok(sim_response);
        }

        Err("Invalid RPC response format".into())
    }

    /// Retrieve the latest ledger sequence from Soroban RPC
    pub async fn get_latest_ledger(&self) -> Result<u32, Box<dyn std::error::Error + Send + Sync>> {
        // TODO: Implement BE-013 (Perform RPC query for ledger)
        Ok(1234567)
    }

    /// Broadcast a transaction envelope to the network
    pub async fn submit_transaction(
        &self,
        _tx_envelope: &str,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        Ok("tx_hash_placeholder".to_string())
    }
}

/// #543: build a base64-encoded, unsigned Stellar XDR `TransactionEnvelope`
/// (v1) paying `amount_stroops` of native XLM from `source_account` to
/// `destination_account`, for the payout-by-username route.
///
/// Sequence is a `0` sentinel — same convention as
/// `api::yield::build_stellar_envelope_xdr` — the signing wallet (or a
/// pre-submission `getAccount` call) must substitute the real sequence + 1
/// before signing. Network is selected from `STELLAR_NETWORK` the same way.
pub fn build_payout_envelope_xdr(
    source_account: &str,
    destination_account: &str,
    amount_stroops: i64,
) -> Result<String, String> {
    if amount_stroops <= 0 {
        return Err("amount must be positive".to_string());
    }

    let source_pk = PublicKey::from_account_id(source_account)
        .map_err(|e| format!("invalid source account: {e}"))?;
    let destination_pk = PublicKey::from_account_id(destination_account)
        .map_err(|e| format!("invalid destination account: {e}"))?;

    let payment_op = Operation::new_payment()
        .with_destination(destination_pk)
        .with_asset(Asset::new_native())
        .with_amount(Stroops::new(amount_stroops))
        .map_err(|e| format!("payment op amount error: {e}"))?
        .build()
        .map_err(|e| format!("payment op error: {e}"))?;

    // Sequence 0 is a sentinel; wallets must substitute the real value.
    let sequence: i64 = 0;
    let tx = Transaction::builder(source_pk, sequence, MIN_BASE_FEE)
        .add_operation(payment_op)
        .into_transaction()
        .map_err(|e| format!("transaction build error: {e}"))?;

    // Select network from environment (default: testnet) — kept for parity
    // with build_stellar_envelope_xdr even though it isn't consumed further
    // here; XDR serialization itself isn't network-dependent.
    let _network = match std::env::var("STELLAR_NETWORK")
        .unwrap_or_default()
        .to_lowercase()
        .as_str()
    {
        "mainnet" | "public" => Network::new_public(),
        _ => Network::new_test(),
    };

    let envelope = tx.into_envelope();
    envelope
        .xdr_base64()
        .map_err(|e| format!("XDR serialization error: {e}"))
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SimulateTransactionResponse {
    pub results: Option<Vec<SimulateTransactionResult>>,
    pub footprint: Option<String>,
    pub cost: Option<SimulateTransactionCost>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SimulateTransactionResult {
    pub xdr: String,
    pub auth: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SimulateTransactionCost {
    #[serde(rename = "cpuInsns")]
    pub cpu_insns: String,
    #[serde(rename = "memBytes")]
    pub mem_bytes: String,
}

// ─── Stellar Disbursement Platform (SDP) Client ─────────────────────────────

/// Client for the Stellar Disbursement Platform REST API.
///
/// Wraps SDP endpoints for bulk payouts with authentication, error handling,
/// and retry logic. Issue BE-552.
pub struct SdpClient {
    base_url: String,
    api_token: Option<String>,
    http_client: reqwest::Client,
}

impl SdpClient {
    /// Create a new SDP client.
    ///
    /// If `api_token` is None, calls return dry-run results without contacting SDP.
    pub fn new(base_url: String, api_token: Option<String>) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            api_token,
            http_client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap(),
        }
    }

    /// Create an SDP client from environment variables.
    ///
    /// Reads `SDP_BASE_URL` (default: https://sdp.stellar.org) and
    /// `SDP_API_TOKEN` (optional).
    pub fn from_env() -> Self {
        let base_url =
            std::env::var("SDP_BASE_URL").unwrap_or_else(|_| "https://sdp.stellar.org".into());
        let api_token = std::env::var("SDP_API_TOKEN").ok();
        Self::new(base_url, api_token)
    }

    /// Submit a disbursement to SDP with idempotency support.
    ///
    /// Returns the outcome of the submission attempt. Transient errors
    /// (network failures, 5xx) return `SdpOutcome::Retryable`. Permanent
    /// errors (4xx) return `SdpOutcome::Permanent`. Success returns
    /// `SdpOutcome::Submitted`.
    pub async fn submit_disbursement(
        &self,
        request: &SdpDisbursementRequest<'_>,
    ) -> SdpOutcome {
        let Some(token) = self.api_token.as_deref() else {
            // Dry-run mode: return synthetic success without contacting SDP
            return SdpOutcome::Submitted {
                payment_id: Some(format!("dry-run-{}", request.idempotency_key)),
                tx_hash: None,
            };
        };

        let response = self
            .http_client
            .post(format!("{}/disbursements", self.base_url))
            .bearer_auth(token)
            .json(request)
            .send()
            .await;

        let response = match response {
            Ok(r) => r,
            Err(err) => {
                // Network-level failure: no way to know if SDP saw it, so
                // retry and let the idempotency key deduplicate.
                return SdpOutcome::Retryable(format!("SDP request failed: {err}"));
            }
        };

        let status = response.status();
        if status.is_success() {
            return match response.json::<SdpDisbursementResponse>().await {
                Ok(body) => SdpOutcome::Submitted {
                    payment_id: body.payment_id,
                    tx_hash: body.tx_hash,
                },
                // SDP accepted it; we just could not parse the body. Treating
                // this as a failure would risk a double payment on retry.
                Err(err) => {
                    tracing::warn!(
                        idempotency_key = %request.idempotency_key,
                        error = ?err,
                        "SDP returned success with unparseable body"
                    );
                    SdpOutcome::Submitted {
                        payment_id: None,
                        tx_hash: None,
                    }
                }
            };
        }

        let detail = response.text().await.unwrap_or_default();

        // 4xx other than 408/429 means SDP rejected the request itself — a
        // bad address or malformed amount will be rejected identically forever.
        if status.is_client_error()
            && status != reqwest::StatusCode::REQUEST_TIMEOUT
            && status != reqwest::StatusCode::TOO_MANY_REQUESTS
        {
            return SdpOutcome::Permanent(format!("SDP rejected request ({status}): {detail}"));
        }

        SdpOutcome::Retryable(format!("SDP error ({status}): {detail}"))
    }

    /// Get the status of a disbursement by payment ID.
    pub async fn get_disbursement_status(
        &self,
        payment_id: &str,
    ) -> Result<SdpDisbursementStatus, String> {
        let Some(token) = self.api_token.as_deref() else {
            return Err("SDP API token not configured".into());
        };

        let response = self
            .http_client
            .get(format!("{}/disbursements/{}", self.base_url, payment_id))
            .bearer_auth(token)
            .send()
            .await
            .map_err(|e| format!("Failed to query SDP: {e}"))?;

        if !response.status().is_success() {
            let status = response.status();
            let detail = response.text().await.unwrap_or_default();
            return Err(format!("SDP status query failed ({status}): {detail}"));
        }

        response
            .json::<SdpDisbursementStatus>()
            .await
            .map_err(|e| format!("Failed to parse SDP status response: {e}"))
    }

    /// List recent disbursements with optional filters.
    pub async fn list_disbursements(
        &self,
        params: &SdpListParams,
    ) -> Result<SdpDisbursementList, String> {
        let Some(token) = self.api_token.as_deref() else {
            return Err("SDP API token not configured".into());
        };

        let mut url = format!("{}/disbursements", self.base_url);
        let mut query_parts = Vec::new();

        if let Some(limit) = params.limit {
            query_parts.push(format!("limit={}", limit));
        }
        if let Some(offset) = params.offset {
            query_parts.push(format!("offset={}", offset));
        }
        if let Some(ref status) = params.status {
            query_parts.push(format!("status={}", status));
        }

        if !query_parts.is_empty() {
            url.push_str("?");
            url.push_str(&query_parts.join("&"));
        }

        let response = self
            .http_client
            .get(&url)
            .bearer_auth(token)
            .send()
            .await
            .map_err(|e| format!("Failed to list disbursements: {e}"))?;

        if !response.status().is_success() {
            let status = response.status();
            let detail = response.text().await.unwrap_or_default();
            return Err(format!("SDP list query failed ({status}): {detail}"));
        }

        response
            .json::<SdpDisbursementList>()
            .await
            .map_err(|e| format!("Failed to parse SDP list response: {e}"))
    }
}

// ─── SDP Request/Response Types ──────────────────────────────────────────────

/// Request payload for submitting a disbursement to SDP.
#[derive(Debug, Serialize)]
pub struct SdpDisbursementRequest<'a> {
    /// Deterministic per-recipient key. SDP deduplicates on this.
    pub idempotency_key: String,
    /// Destination Stellar address.
    pub destination: &'a str,
    /// Amount in stroops (smallest unit).
    pub amount: i64,
    /// Currency code (e.g., "USDC", "NGN").
    pub currency: &'a str,
}

/// Response from SDP after submitting a disbursement.
#[derive(Debug, Deserialize)]
pub struct SdpDisbursementResponse {
    #[serde(default)]
    pub payment_id: Option<String>,
    #[serde(default)]
    pub tx_hash: Option<String>,
}

/// Outcome of a disbursement submission attempt.
#[derive(Debug)]
pub enum SdpOutcome {
    /// Successfully submitted. May not have all IDs if response was unparseable.
    Submitted {
        payment_id: Option<String>,
        tx_hash: Option<String>,
    },
    /// Transient error — worth retrying.
    Retryable(String),
    /// Permanent error — retrying won't help.
    Permanent(String),
}

/// Status response for a disbursement.
#[derive(Debug, Deserialize)]
pub struct SdpDisbursementStatus {
    pub payment_id: String,
    pub status: String,
    #[serde(default)]
    pub tx_hash: Option<String>,
    #[serde(default)]
    pub amount: Option<i64>,
    #[serde(default)]
    pub destination: Option<String>,
}

/// Parameters for listing disbursements.
#[derive(Debug, Default)]
pub struct SdpListParams {
    pub limit: Option<u32>,
    pub offset: Option<u32>,
    pub status: Option<String>,
}

/// List response from SDP.
#[derive(Debug, Deserialize)]
pub struct SdpDisbursementList {
    pub disbursements: Vec<SdpDisbursementStatus>,
    #[serde(default)]
    pub total: Option<u64>,
}
