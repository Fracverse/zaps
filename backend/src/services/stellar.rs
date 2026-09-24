use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};
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

/// #947: How long an endpoint that timed out or returned a retryable error is
/// skipped before it becomes eligible for traffic again.
const ENDPOINT_COOLDOWN: Duration = Duration::from_secs(30);

/// Comma-separated backup Horizon/Soroban RPC endpoints, tried in order after
/// the primary (`STELLAR_RPC_URL`) fails.
const BACKUP_URLS_ENV: &str = "STELLAR_RPC_BACKUP_URLS";

pub struct StellarClient {
    pub rpc_url: String,
    /// Ordered failover endpoints. The first endpoint remains `rpc_url` for
    /// backwards compatibility with existing callers and diagnostics.
    pub rpc_urls: Vec<String>,
    pub http_client: reqwest::Client,
    /// #947: Index of the endpoint currently serving traffic. Sticky: it only
    /// moves when that endpoint fails, so a healthy primary keeps all load.
    current_endpoint: AtomicUsize,
    /// Per-endpoint cooldown deadline; `Some(t)` means "unhealthy until `t`".
    unhealthy_until: Vec<Mutex<Option<Instant>>>,
}

impl StellarClient {
    pub fn new(rpc_url: String) -> Self {
        Self::with_rpc_urls(vec![rpc_url])
    }

    /// #947: `rpc_url` as primary, followed by any backups listed in
    /// `STELLAR_RPC_BACKUP_URLS`.
    pub fn with_env_backups(rpc_url: String) -> Self {
        let backups = std::env::var(BACKUP_URLS_ENV).unwrap_or_default();
        Self::with_rpc_urls(
            std::iter::once(rpc_url)
                .chain(backups.split(',').map(|url| url.trim().to_string()))
                .collect(),
        )
    }

    /// Build a client with an ordered pool of Horizon/Soroban endpoints: the
    /// first is the primary, the rest are backups. On timeout, connection
    /// failure, 5xx or 429 the failing endpoint is put on cooldown and the
    /// request fails over to the next healthy endpoint (round-robin).
    pub fn with_rpc_urls(rpc_urls: Vec<String>) -> Self {
        let mut endpoints: Vec<String> = Vec::new();
        for url in rpc_urls {
            let url = url.trim().trim_end_matches('/').to_string();
            if !url.is_empty() && !endpoints.contains(&url) {
                endpoints.push(url);
            }
        }
        if endpoints.is_empty() {
            endpoints.push("https://soroban-testnet.stellar.org".to_string());
        }
        let rpc_url = endpoints[0].clone();
        let unhealthy_until = endpoints.iter().map(|_| Mutex::new(None)).collect();
        Self {
            rpc_url,
            rpc_urls: endpoints,
            http_client: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .expect("valid reqwest client configuration"),
            current_endpoint: AtomicUsize::new(0),
            unhealthy_until,
        }
    }

    /// #947: The endpoint to try next — the current one if it is healthy,
    /// otherwise the next healthy one in pool order. If every endpoint is
    /// cooling down, the current one is used anyway rather than failing
    /// without trying.
    fn select_endpoint(&self) -> usize {
        let len = self.rpc_urls.len();
        let start = self.current_endpoint.load(Ordering::Acquire) % len;
        let now = Instant::now();
        (0..len)
            .map(|offset| (start + offset) % len)
            .find(|&idx| self.is_healthy(idx, now))
            .unwrap_or(start)
    }

    fn is_healthy(&self, idx: usize, now: Instant) -> bool {
        match *self.unhealthy_until[idx].lock().unwrap() {
            Some(until) => now >= until,
            None => true,
        }
    }

    fn mark_healthy(&self, idx: usize) {
        *self.unhealthy_until[idx].lock().unwrap() = None;
        self.current_endpoint.store(idx, Ordering::Release);
    }

    /// Put `idx` on cooldown and advance the pool past it. Uses a CAS so that
    /// concurrent requests failing on the same endpoint advance it only once.
    fn mark_unhealthy(&self, idx: usize) {
        *self.unhealthy_until[idx].lock().unwrap() = Some(Instant::now() + ENDPOINT_COOLDOWN);
        let next = (idx + 1) % self.rpc_urls.len();
        let _ = self.current_endpoint.compare_exchange(
            idx,
            next,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
    }

    /// Send an RPC request with retry mechanism (BE-042) and endpoint
    /// failover (#947).
    ///
    /// A timeout, connection error, 5xx or 429 puts the endpoint on cooldown
    /// and the request is retried immediately against the next healthy
    /// endpoint. Exponential backoff (1s, 2s, 4s, ...) is applied only once
    /// every endpoint in the pool has been tried, so with a single endpoint
    /// the behaviour is the original retry-with-backoff.
    pub async fn send_rpc_request(
        &self,
        method: &str,
        params: Value,
    ) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        let pool_size = self.rpc_urls.len();
        let max_attempts = pool_size.max(3);
        let mut attempts = 0;
        let mut backoff_rounds = 0u32;

        let payload = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": params
        });

        loop {
            attempts += 1;

            let endpoint_index = self.select_endpoint();
            let endpoint = &self.rpc_urls[endpoint_index];
            let response = self.http_client.post(endpoint).json(&payload).send().await;

            let failure = match response {
                Ok(resp) => {
                    let status = resp.status();
                    if status.is_success() {
                        self.mark_healthy(endpoint_index);
                        let json_resp: Value = resp.json().await?;
                        return Ok(json_resp);
                    } else if is_retryable_status(status) {
                        format!("HTTP {status}")
                    } else {
                        return Err(format!("RPC call failed with status: {}", status).into());
                    }
                }
                Err(e) => {
                    if e.is_timeout() || e.is_connect() {
                        e.to_string()
                    } else {
                        return Err(e.into());
                    }
                }
            };

            self.mark_unhealthy(endpoint_index);
            tracing::warn!(
                endpoint = %endpoint,
                attempt = attempts,
                "Stellar RPC endpoint failed ({failure}), failing over"
            );

            if attempts >= max_attempts {
                return Err(format!(
                    "Max retry attempts reached across {pool_size} RPC endpoint(s): {failure}"
                )
                .into());
            }

            // Fail over to a backup immediately; back off only after a full
            // pass over the pool.
            if attempts % pool_size == 0 {
                tokio::time::sleep(Duration::from_secs(2_u64.pow(backoff_rounds))).await;
                backoff_rounds += 1;
            }
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

    /// Broadcast a transaction envelope to the network.
    ///
    /// The actual HTTP call goes through `send_rpc_request`, which already
    /// implements automatic retry with exponential backoff (1s, 2s, 4s) for
    /// temporary failures — HTTP 503 and timeouts — as required by issue #729.
    pub async fn submit_transaction(
        &self,
        tx_envelope: &str,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let params = json!({ "transaction": tx_envelope });

        let response = self.send_rpc_request("sendTransaction", params).await?;

        if let Some(error) = response.get("error") {
            return Err(format!("RPC error submitting transaction: {error}").into());
        }

        let hash = match response
            .get("result")
            .and_then(|r| r.get("hash"))
            .and_then(|h| h.as_str())
        {
            Some(h) => h.to_string(),
            None => return Err("Missing transaction hash in RPC response".into()),
        };

        Ok(hash)
    }
}

/// #947: Any 5xx is a node-side failure worth failing over on; 429 means the
/// node is shedding load, so another node may still serve the request.
fn is_retryable_status(status: reqwest::StatusCode) -> bool {
    status.is_server_error() || status == reqwest::StatusCode::TOO_MANY_REQUESTS
}

#[cfg(test)]
mod failover_tests {
    use super::*;

    fn pool() -> StellarClient {
        StellarClient::with_rpc_urls(vec![
            "https://primary.example".into(),
            "https://backup-1.example/".into(),
            "https://backup-2.example".into(),
        ])
    }

    #[test]
    fn normalizes_and_dedupes_endpoints() {
        let client = StellarClient::with_rpc_urls(vec![
            " https://a.example/ ".into(),
            "".into(),
            "https://a.example".into(),
            "https://b.example".into(),
        ]);
        assert_eq!(client.rpc_urls, vec!["https://a.example", "https://b.example"]);
        assert_eq!(client.rpc_url, "https://a.example");
    }

    #[test]
    fn empty_pool_falls_back_to_testnet() {
        let client = StellarClient::with_rpc_urls(vec![]);
        assert_eq!(client.rpc_urls, vec!["https://soroban-testnet.stellar.org"]);
    }

    #[test]
    fn healthy_primary_is_sticky() {
        let client = pool();
        assert_eq!(client.select_endpoint(), 0);
        client.mark_healthy(0);
        assert_eq!(client.select_endpoint(), 0);
    }

    #[test]
    fn fails_over_to_next_healthy_backup() {
        let client = pool();
        client.mark_unhealthy(0);
        assert_eq!(client.select_endpoint(), 1);
        client.mark_unhealthy(1);
        assert_eq!(client.select_endpoint(), 2);
    }

    #[test]
    fn skips_cooling_endpoint_even_if_current_points_at_it() {
        let client = pool();
        client.mark_unhealthy(1);
        // current is still 0 (healthy); after 0 fails, 1 is cooling down.
        client.mark_unhealthy(0);
        assert_eq!(client.select_endpoint(), 2);
    }

    #[test]
    fn concurrent_failures_on_same_endpoint_advance_once() {
        let client = pool();
        client.mark_unhealthy(0);
        client.mark_unhealthy(0);
        assert_eq!(client.current_endpoint.load(Ordering::Acquire), 1);
    }

    #[test]
    fn all_endpoints_down_still_returns_an_endpoint() {
        let client = pool();
        for idx in 0..3 {
            client.mark_unhealthy(idx);
        }
        assert!(client.select_endpoint() < 3);
    }

    #[test]
    fn success_clears_cooldown() {
        let client = pool();
        client.mark_unhealthy(0);
        client.mark_healthy(0);
        assert!(client.is_healthy(0, Instant::now()));
        assert_eq!(client.select_endpoint(), 0);
    }

    #[test]
    fn classifies_retryable_statuses() {
        use reqwest::StatusCode;
        for status in [
            StatusCode::INTERNAL_SERVER_ERROR,
            StatusCode::BAD_GATEWAY,
            StatusCode::SERVICE_UNAVAILABLE,
            StatusCode::GATEWAY_TIMEOUT,
            StatusCode::HTTP_VERSION_NOT_SUPPORTED,
            StatusCode::TOO_MANY_REQUESTS,
        ] {
            assert!(is_retryable_status(status), "{status} should fail over");
        }
        for status in [StatusCode::BAD_REQUEST, StatusCode::NOT_FOUND] {
            assert!(!is_retryable_status(status), "{status} should not retry");
        }
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
    pub async fn submit_disbursement(&self, request: &SdpDisbursementRequest<'_>) -> SdpOutcome {
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
            url.push('?');
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
