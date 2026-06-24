// Allbridge API Integration Client
// This client calls Allbridge Core REST API endpoints to fetch quotes, calculate fees,
// and trace cross-chain bridge transaction updates.

use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Serialize, Deserialize)]
pub struct AllbridgeQuoteRequest {
    pub source_chain: String,
    pub source_token: String,
    pub amount: String,
    pub destination_chain: String,
    pub destination_token: String,
    pub destination_address: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AllbridgeQuoteResponse {
    pub fee: String,
    pub receive_amount: String,
    pub bridge_tx_data: String,
}

#[derive(Debug, Deserialize)]
struct AllbridgeQuoteApiResponse {
    #[serde(default)]
    fee: Option<String>,
    #[serde(default)]
    receive_amount: Option<String>,
    #[serde(default)]
    bridge_tx_data: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AllbridgeProxyError {
    pub message: String,
}

impl std::fmt::Display for AllbridgeProxyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for AllbridgeProxyError {}

/// Normalized lifecycle status of a cross-chain transfer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BridgeStatusKind {
    Pending,
    Success,
    Failed,
}

impl BridgeStatusKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            BridgeStatusKind::Pending => "PENDING",
            BridgeStatusKind::Success => "SUCCESS",
            BridgeStatusKind::Failed => "FAILED",
        }
    }

    /// Terminal states no longer need to be polled.
    pub fn is_terminal(&self) -> bool {
        matches!(self, BridgeStatusKind::Success | BridgeStatusKind::Failed)
    }
}

impl fmt::Display for BridgeStatusKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Result of polling Allbridge for a single transfer.
#[derive(Debug, Clone)]
pub struct BridgeTransferStatus {
    pub status: BridgeStatusKind,
    pub confirmations: i32,
}

// ---- Allbridge Core API response shapes (parsed leniently) --------------------
// The status endpoint returns the source ("send") leg and, once the funds have
// been delivered on the destination chain, a "receive" leg. Presence of a
// completed receive leg means the transfer succeeded.

#[derive(Debug, Deserialize)]
struct AllbridgeStatusResponse {
    #[serde(default)]
    send: Option<AllbridgeLeg>,
    #[serde(default)]
    receive: Option<AllbridgeLeg>,
}

#[derive(Debug, Deserialize)]
struct AllbridgeLeg {
    #[serde(default)]
    confirmations: Option<i64>,
    #[serde(default)]
    status: Option<String>,
}

impl AllbridgeLeg {
    fn is_failed(&self) -> bool {
        matches!(
            self.status.as_deref().map(str::to_ascii_lowercase).as_deref(),
            Some("failed") | Some("error") | Some("refunded") | Some("reverted")
        )
    }
}

impl AllbridgeStatusResponse {
    fn into_transfer_status(self) -> BridgeTransferStatus {
        let send_conf = self
            .send
            .as_ref()
            .and_then(|leg| leg.confirmations)
            .unwrap_or(0);
        let receive_conf = self
            .receive
            .as_ref()
            .and_then(|leg| leg.confirmations)
            .unwrap_or(0);
        let confirmations = send_conf.max(receive_conf).max(0) as i32;

        // Any leg explicitly reporting a failure marks the whole transfer failed.
        let failed = self.send.as_ref().map(AllbridgeLeg::is_failed).unwrap_or(false)
            || self.receive.as_ref().map(AllbridgeLeg::is_failed).unwrap_or(false);

        let status = if failed {
            BridgeStatusKind::Failed
        } else if self.receive.is_some() {
            // Funds delivered on the destination chain.
            BridgeStatusKind::Success
        } else {
            BridgeStatusKind::Pending
        };

        BridgeTransferStatus {
            status,
            confirmations,
        }
    }
}

pub struct AllbridgeClient {
    pub api_url: String,
    client: reqwest::Client,
}

impl AllbridgeClient {
    pub fn new(api_url: String) -> Self {
        Self {
            api_url,
            client: reqwest::Client::new(),
        }
    }

    /// Retrieve fee calculations and routing parameters from Allbridge.
    pub async fn get_price_quote(
        &self,
        request: &AllbridgeQuoteRequest,
    ) -> Result<AllbridgeQuoteResponse, AllbridgeProxyError> {
        let url = format!(
            "{}/quote",
            self.api_url.trim_end_matches('/')
        );

        let response = self
            .client
            .post(&url)
            .json(request)
            .send()
            .await
            .map_err(|err| AllbridgeProxyError {
                message: format!("allbridge quote request failed: {err}"),
            })?;

        if !response.status().is_success() {
            return Err(AllbridgeProxyError {
                message: format!("allbridge upstream returned status {}", response.status()),
            });
        }

        let payload: AllbridgeQuoteApiResponse = response.json().await.map_err(|err| AllbridgeProxyError {
            message: format!("allbridge quote response parsing failed: {err}"),
        })?;

        Ok(AllbridgeQuoteResponse {
            fee: payload.fee.unwrap_or_else(|| "0".to_string()),
            receive_amount: payload.receive_amount.unwrap_or_else(|| "0".to_string()),
            bridge_tx_data: payload.bridge_tx_data.unwrap_or_else(|| "".to_string()),
        })
    }

    /// Poll Allbridge backend for status updates on a specific cross-chain transaction.
    ///
    /// `source_chain` is the Allbridge chain symbol of the deposit (e.g. "STLR", "ETH")
    /// and `tx_hash` is the source-chain transaction hash. A transfer that Allbridge
    /// hasn't indexed yet (404) is reported as `PENDING` rather than an error so callers
    /// can keep polling.
    pub async fn poll_transaction_status(
        &self,
        source_chain: &str,
        tx_hash: &str,
    ) -> Result<BridgeTransferStatus, reqwest::Error> {
        let url = format!(
            "{}/chain/{}/{}",
            self.api_url.trim_end_matches('/'),
            source_chain,
            tx_hash
        );

        let response = self.client.get(&url).send().await?;

        // Not yet indexed by Allbridge => still in flight.
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(BridgeTransferStatus {
                status: BridgeStatusKind::Pending,
                confirmations: 0,
            });
        }

        let response = response.error_for_status()?;
        let body: AllbridgeStatusResponse = response.json().await?;
        Ok(body.into_transfer_status())
    }
}
