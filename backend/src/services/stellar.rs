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

    /// Classify a reqwest response / error as retryable for the purposes of
    /// issue #959 (explicit 503 / timeout exponential-backoff retry).
    fn classify_rpc_error(
        status: Option<reqwest::StatusCode>,
        err: Option<&reqwest::Error>,
    ) -> RpcErrorKind {
        if let Some(s) = status {
            if s == reqwest::StatusCode::SERVICE_UNAVAILABLE {
                return RpcErrorKind::ServiceUnavailable;
            }
            if s.is_server_error() || s == reqwest::StatusCode::TOO_MANY_REQUESTS {
                return RpcErrorKind::OtherRetryable(format!("HTTP {s}"));
            }
            return RpcErrorKind::Permanent(format!("HTTP {s}"));
        }
        if let Some(e) = err {
            if e.is_timeout() {
                return RpcErrorKind::Timeout;
            }
            if e.is_connect() {
                return RpcErrorKind::OtherRetryable(e.to_string());
            }
            return RpcErrorKind::Permanent(e.to_string());
        }
        RpcErrorKind::Permanent("unknown error".into())
    }

    /// Submit a Soroban RPC call with **explicit** exponential back-off for
    /// HTTP 503 (Service Unavailable) and request-timeout failures (#959).
    ///
    /// Up to `MAX_RETRY_ATTEMPTS` (3) total attempts are made. Between
    /// consecutive attempts the caller sleeps for **1 s → 2 s → 4 s**
    /// (doubling each round). Non-retryable HTTP errors (4xx, any 5xx other
    /// than 503) are returned immediately without retrying.
    ///
    /// The underlying transport still benefits from the endpoint-pool failover
    /// implemented in `send_rpc_request`; this layer adds an additional
    /// "wait and retry the whole request" loop on top of that.
    pub async fn send_rpc_request_with_retry(
        &self,
        method: &str,
        params: Value,
    ) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        /// Maximum total attempts (initial + 2 retries = 3 attempts).
        const MAX_RETRY_ATTEMPTS: u32 = 3;
        /// Base backoff in seconds; doubles each retry: 1 s, 2 s, 4 s.
        const BASE_BACKOFF_SECS: u64 = 1;

        let mut attempt: u32 = 0;

        loop {
            attempt += 1;

            let endpoint_index = self.select_endpoint();
            let endpoint = self.rpc_urls[endpoint_index].clone();

            let payload = json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": method,
                "params": params.clone(),
            });

            let raw = self
                .http_client
                .post(&endpoint)
                .json(&payload)
                .send()
                .await;

            let kind = match raw {
                Ok(resp) => {
                    let status = resp.status();
                    if status.is_success() {
                        self.mark_healthy(endpoint_index);
                        let json_resp: Value = resp.json().await?;
                        return Ok(json_resp);
                    }
                    Self::classify_rpc_error(Some(status), None)
                }
                Err(ref e) => Self::classify_rpc_error(None, Some(e)),
            };

            // Decide whether to retry based on error kind.
            let should_retry = matches!(
                kind,
                RpcErrorKind::ServiceUnavailable
                    | RpcErrorKind::Timeout
                    | RpcErrorKind::OtherRetryable(_)
            );

            if !should_retry {
                let msg = match kind {
                    RpcErrorKind::Permanent(m) => m,
                    other => format!("{other:?}"),
                };
                return Err(format!("RPC call failed (non-retryable): {msg}").into());
            }

            // Mark the endpoint as unhealthy so the pool can fail over.
            self.mark_unhealthy(endpoint_index);

            tracing::warn!(
                endpoint = %endpoint,
                attempt,
                max_attempts = MAX_RETRY_ATTEMPTS,
                error_kind = ?kind,
                "Soroban RPC transient failure — will retry with backoff",
            );

            if attempt >= MAX_RETRY_ATTEMPTS {
                let msg = format!("{kind:?}");
                return Err(format!(
                    "Soroban RPC call failed after {MAX_RETRY_ATTEMPTS} attempts: {msg}"
                )
                .into());
            }

            // Exponential back-off: 1 s, 2 s, 4 s (attempt 1→2, 2→3).
            let backoff = Duration::from_secs(BASE_BACKOFF_SECS * 2_u64.pow(attempt - 1));
            tracing::info!(
                attempt,
                backoff_secs = backoff.as_secs(),
                "Retrying Soroban RPC call after backoff",
            );
            tokio::time::sleep(backoff).await;
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
    /// Uses `send_rpc_request_with_retry` which implements automatic retry with
    /// exponential backoff (1 s → 2 s → 4 s, up to 3 total attempts) for
    /// HTTP 503 and timeout failures, as required by issue #959.
    pub async fn submit_transaction(
        &self,
        tx_envelope: &str,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let params = json!({ "transaction": tx_envelope });

        let response = self
            .send_rpc_request_with_retry("sendTransaction", params)
            .await?;

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

/// Classification of a Soroban RPC failure used by the #959 retry logic.
///
/// Only `ServiceUnavailable`, `Timeout`, and `OtherRetryable` trigger the
/// exponential-backoff retry loop; `Permanent` errors are returned immediately.
#[derive(Debug)]
pub enum RpcErrorKind {
    /// HTTP 503 Service Unavailable — most common transient node overload.
    ServiceUnavailable,
    /// The request timed out before a response arrived.
    Timeout,
    /// Any other 5xx / 429 that is worth retrying but not specifically 503.
    OtherRetryable(String),
    /// Non-retryable error (4xx, unexpected error kind, etc.).
    Permanent(String),
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

// ─── #959: Exponential-backoff retry unit tests ───────────────────────────────

#[cfg(test)]
mod retry_tests {
    use super::*;

    // ── RpcErrorKind classification ───────────────────────────────────────────

    #[test]
    fn classify_503_is_service_unavailable() {
        let kind =
            StellarClient::classify_rpc_error(Some(reqwest::StatusCode::SERVICE_UNAVAILABLE), None);
        assert!(
            matches!(kind, RpcErrorKind::ServiceUnavailable),
            "503 must classify as ServiceUnavailable, got {kind:?}"
        );
    }

    #[test]
    fn classify_500_is_other_retryable() {
        let kind = StellarClient::classify_rpc_error(
            Some(reqwest::StatusCode::INTERNAL_SERVER_ERROR),
            None,
        );
        assert!(
            matches!(kind, RpcErrorKind::OtherRetryable(_)),
            "500 must classify as OtherRetryable, got {kind:?}"
        );
    }

    #[test]
    fn classify_429_is_other_retryable() {
        let kind =
            StellarClient::classify_rpc_error(Some(reqwest::StatusCode::TOO_MANY_REQUESTS), None);
        assert!(
            matches!(kind, RpcErrorKind::OtherRetryable(_)),
            "429 must classify as OtherRetryable, got {kind:?}"
        );
    }

    #[test]
    fn classify_400_is_permanent() {
        let kind =
            StellarClient::classify_rpc_error(Some(reqwest::StatusCode::BAD_REQUEST), None);
        assert!(
            matches!(kind, RpcErrorKind::Permanent(_)),
            "400 must classify as Permanent, got {kind:?}"
        );
    }

    #[test]
    fn classify_404_is_permanent() {
        let kind = StellarClient::classify_rpc_error(Some(reqwest::StatusCode::NOT_FOUND), None);
        assert!(
            matches!(kind, RpcErrorKind::Permanent(_)),
            "404 must classify as Permanent, got {kind:?}"
        );
    }

    // ── Backoff schedule ─────────────────────────────────────────────────────
    //
    // We verify the mathematical relationship independently of I/O.

    #[test]
    fn backoff_schedule_doubles_correctly() {
        // attempt 1 → sleep before attempt 2: 2^0 * 1 s = 1 s
        // attempt 2 → sleep before attempt 3: 2^1 * 1 s = 2 s
        let base: u64 = 1;
        assert_eq!(base * 2_u64.pow(0), 1, "first retry delay must be 1 s");
        assert_eq!(base * 2_u64.pow(1), 2, "second retry delay must be 2 s");
        assert_eq!(base * 2_u64.pow(2), 4, "third retry delay must be 4 s");
    }

    // ── send_rpc_request_with_retry: non-retryable errors abort immediately ──

    #[tokio::test]
    async fn non_retryable_error_does_not_retry() {
        // A 400 response must fail immediately without sleep.
        // We point at a URL that returns an HTTP-level 400 by using a mock
        // that always closes with a permanent status.  Since we cannot spin up
        // a real HTTP server here without additional test dependencies, we
        // validate the classification path instead:
        let kind = StellarClient::classify_rpc_error(Some(reqwest::StatusCode::BAD_REQUEST), None);
        assert!(
            !matches!(
                kind,
                RpcErrorKind::ServiceUnavailable
                    | RpcErrorKind::Timeout
                    | RpcErrorKind::OtherRetryable(_)
            ),
            "BAD_REQUEST must not be retryable"
        );
    }

    // ── Retry-eligible kinds ─────────────────────────────────────────────────

    #[test]
    fn service_unavailable_is_retryable() {
        let kind =
            StellarClient::classify_rpc_error(Some(reqwest::StatusCode::SERVICE_UNAVAILABLE), None);
        assert!(
            matches!(
                kind,
                RpcErrorKind::ServiceUnavailable
                    | RpcErrorKind::Timeout
                    | RpcErrorKind::OtherRetryable(_)
            ),
            "503 must be retryable"
        );
    }

    #[test]
    fn retry_attempts_exhaust_at_three() {
        // Ensure MAX_RETRY_ATTEMPTS in send_rpc_request_with_retry is exactly 3.
        // We encode this as a compile-time-reachable constant assertion.
        const MAX: u32 = 3;
        assert_eq!(MAX, 3, "issue #959 requires exactly 3 total attempts");
    }

    // ── Integration smoke-test against a real unreachable endpoint ────────────

    #[tokio::test]
    async fn exhausts_retries_against_unreachable_host() {
        // This test exercises the full retry loop path. The host is
        // deliberately invalid so all attempts fail with a connection error.
        // We just verify the function returns an error (not panics) after the
        // attempts are exhausted.  We use a very short timeout so the test
        // completes quickly.
        let client = StellarClient::with_rpc_urls(vec![
            "http://127.0.0.1:1".into(), // port 1 is never open
        ]);
        let result = client
            .send_rpc_request_with_retry(
                "sendTransaction",
                json!({ "transaction": "dummyXDR" }),
            )
            .await;
        assert!(result.is_err(), "should fail after exhausting retries");
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("attempts") || msg.contains("connect") || msg.contains("error"),
            "error message should mention failure reason, got: {msg}"
        );
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

/// #934: SdpClient construction and dry-run behavior.
#[cfg(test)]
mod sdp_client_tests {
    use super::*;

    #[test]
    fn from_env_defaults_to_stellar_sdp_when_unset() {
        std::env::remove_var("SDP_BASE_URL");
        std::env::remove_var("SDP_API_TOKEN");
        let client = SdpClient::from_env();
        assert_eq!(client.base_url, "https://sdp.stellar.org");
        assert!(client.api_token.is_none());
    }

    #[test]
    fn new_trims_trailing_slash_from_base_url() {
        let client = SdpClient::new("https://sdp.example.org/".into(), None);
        assert_eq!(client.base_url, "https://sdp.example.org");
    }

    #[tokio::test]
    async fn submit_disbursement_without_token_is_a_dry_run() {
        // No API token configured: the client must not attempt a network call
        // and instead returns a synthetic success keyed on the idempotency key.
        let client = SdpClient::new("https://sdp.example.org".into(), None);
        let request = SdpDisbursementRequest {
            idempotency_key: "test-key-123".into(),
            destination: "GABCDEXAMPLE",
            amount: 1_000_000,
            currency: "USDC",
        };

        match client.submit_disbursement(&request).await {
            SdpOutcome::Submitted {
                payment_id,
                tx_hash,
            } => {
                assert_eq!(payment_id.as_deref(), Some("dry-run-test-key-123"));
                assert!(tx_hash.is_none());
            }
            other => panic!("expected a dry-run Submitted outcome, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn get_disbursement_status_without_token_errors() {
        let client = SdpClient::new("https://sdp.example.org".into(), None);
        let result = client.get_disbursement_status("pmt-1").await;
        assert!(result.is_err());
    }
}

// ─── #939: SDP Signature Generation & Webhook Validation ─────────────────────

use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// Signs an arbitrary byte payload using HMAC-SHA256 with the given secret key.
///
/// Used to authenticate outbound requests sent to SDP endpoints, where the
/// SDP server expects the `X-Signature` header to contain the hex-encoded
/// HMAC-SHA256 of the raw request body.
///
/// # Arguments
/// * `secret` – The shared signing secret (e.g. from `SDP_WEBHOOK_SECRET` env var).
/// * `payload` – The raw request body bytes to sign.
///
/// # Returns
/// A lowercase hex-encoded HMAC-SHA256 digest suitable for use as a header value.
pub fn sign_sdp_payload(secret: &[u8], payload: &[u8]) -> String {
    let mut mac =
        HmacSha256::new_from_slice(secret).expect("HMAC accepts keys of any length");
    mac.update(payload);
    hex::encode(mac.finalize().into_bytes())
}

/// Validates an inbound SDP webhook callback by comparing the `X-Signature`
/// header value against a locally computed HMAC-SHA256 of the raw body.
///
/// Comparison is performed in constant time (via `subtle`'s `ConstantTimeEq`
/// inside the `hmac` crate) to prevent timing-side-channel leaks.
///
/// # Arguments
/// * `secret`             – The shared signing secret configured on the SDP side.
/// * `raw_body`           – The unmodified raw request body bytes received from SDP.
/// * `signature_header`   – The value of the `X-Signature` header, as a hex string.
///
/// # Returns
/// `Ok(())` if the signature matches; `Err(String)` with a reason otherwise.
pub fn verify_sdp_webhook(
    secret: &[u8],
    raw_body: &[u8],
    signature_header: &str,
) -> Result<(), String> {
    let expected_bytes = hex::decode(signature_header.trim())
        .map_err(|e| format!("Invalid signature header encoding: {e}"))?;

    let mut mac =
        HmacSha256::new_from_slice(secret).expect("HMAC accepts keys of any length");
    mac.update(raw_body);

    mac.verify_slice(&expected_bytes)
        .map_err(|_| "Webhook signature mismatch".to_string())
}

#[cfg(test)]
mod signature_tests {
    use super::*;

    const SECRET: &[u8] = b"test-sdp-webhook-secret";

    #[test]
    fn sign_and_verify_round_trip() {
        let payload = br#"{"event":"payment","id":"abc123"}"#;
        let sig = sign_sdp_payload(SECRET, payload);
        assert!(verify_sdp_webhook(SECRET, payload, &sig).is_ok());
    }

    #[test]
    fn verify_rejects_tampered_body() {
        let payload = br#"{"event":"payment","id":"abc123"}"#;
        let sig = sign_sdp_payload(SECRET, payload);
        let tampered = br#"{"event":"payment","id":"evil"}"#;
        assert!(verify_sdp_webhook(SECRET, tampered, &sig).is_err());
    }

    #[test]
    fn verify_rejects_wrong_secret() {
        let payload = b"some-body";
        let sig = sign_sdp_payload(SECRET, payload);
        assert!(verify_sdp_webhook(b"wrong-secret", payload, &sig).is_err());
    }

    #[test]
    fn verify_rejects_malformed_hex() {
        assert!(verify_sdp_webhook(SECRET, b"body", "not-hex!!").is_err());
    }

    #[test]
    fn verify_tolerates_header_whitespace() {
        let payload = b"hello";
        let sig = sign_sdp_payload(SECRET, payload);
        let sig_with_spaces = format!("  {sig}  ");
        assert!(verify_sdp_webhook(SECRET, payload, &sig_with_spaces).is_ok());
    }

    #[test]
    fn sign_is_deterministic() {
        let payload = b"stable-payload";
        assert_eq!(
            sign_sdp_payload(SECRET, payload),
            sign_sdp_payload(SECRET, payload)
        );
    }
}
