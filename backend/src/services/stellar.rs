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
