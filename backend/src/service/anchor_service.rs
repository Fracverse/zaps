/// Stellar Anchor integration service — SEP-10, SEP-12, SEP-24, SEP-31.
///
/// # Protocol Summary
/// - SEP-10  : Web Authentication — proves key ownership via a signed challenge JWT.
/// - SEP-12  : KYC data exchange — used here to check a user's `"CLEARED"` status.
/// - SEP-24  : Interactive withdrawal — Anchor hosts a UI; we obtain a signed URL for the user.
/// - SEP-31  : Cross-border payment — backend-to-backend POST directly to the Anchor.
use crate::{api_error::ApiError, config::Config};
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use deadpool_postgres::Pool;
use reqwest::Client;
use ring::hmac;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{error, info, warn};
use uuid::Uuid;

// ──────────────────────────────────────────────────────────────────────────────
// Public types
// ──────────────────────────────────────────────────────────────────────────────

/// KYC clearance status as reported by the Anchor (SEP-12).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum KycStatus {
    /// User has passed KYC at the Anchor — withdrawals are permitted.
    Cleared,
    /// KYC has been submitted but not yet reviewed.
    Pending,
    /// KYC was rejected; withdrawals must be blocked.
    Rejected,
    /// No KYC record exists at the Anchor for this user.
    NotFound,
}

impl std::fmt::Display for KycStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KycStatus::Cleared => write!(f, "CLEARED"),
            KycStatus::Pending => write!(f, "PENDING"),
            KycStatus::Rejected => write!(f, "REJECTED"),
            KycStatus::NotFound => write!(f, "NOT_FOUND"),
        }
    }
}

/// Response from `POST /transactions/withdraw/interactive` (SEP-24).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sep24InteractiveResponse {
    /// The URL to redirect the user to so they can complete the withdrawal in the Anchor's UI.
    pub url: String,
    /// Anchor-assigned transaction ID — stored as `anchor_tx_id` in our DB.
    pub anchor_tx_id: String,
}

/// Parameters required to initiate a SEP-31 cross-border payout.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sep31PayoutParams {
    pub amount: String,
    pub asset_code: String,
    /// ISO 3166-1 alpha-3 asset issuer address or omit for stellar-native
    pub asset_issuer: Option<String>,
    pub sender_id: String,
    pub receiver_id: String,
    /// Optional memo to attach to the Stellar transaction
    pub memo: Option<String>,
}

/// Response from `POST /transactions` (SEP-31).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sep31PayoutResponse {
    /// Anchor-assigned transaction ID.
    pub anchor_tx_id: String,
    /// Stellar account to which the sending side should send funds.
    pub stellar_account_id: Option<String>,
    /// Memo type required (`text`, `id`, or `hash`).
    pub stellar_memo_type: Option<String>,
    /// Memo value.
    pub stellar_memo: Option<String>,
}

/// Unified anchor transaction status (covers both SEP-24 and SEP-31).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AnchorTxStatus {
    Incomplete,
    PendingStellar,
    PendingAnchor,
    PendingExternal,
    PendingUser,
    PendingUserTransferStart,
    Completed,
    Refunded,
    Expired,
    Error,
}

/// Lightweight DB model returned after DB operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WithdrawalRecord {
    pub id: String,
    pub user_id: String,
    pub destination_address: String,
    pub amount: i64,
    pub asset: String,
    pub status: String,
    pub anchor_tx_id: Option<String>,
    pub kyc_status: String,
    pub sep24_interactive_url: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Parameters for creating a new withdrawal record in our DB.
#[derive(Debug, Clone)]
pub struct CreateWithdrawalParams {
    pub user_id: String,
    pub destination_address: String,
    pub amount: i64,
    pub asset: String,
    pub anchor_tx_id: Option<String>,
    pub kyc_status: KycStatus,
    pub sep24_interactive_url: Option<String>,
}

// ──────────────────────────────────────────────────────────────────────────────
// Internal Anchor API shapes (minimal — we only deserialise what we need)
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct AnchorKycResponse {
    /// Top-level status from a SEP-12 KYC check.
    status: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AnchorSep24Response {
    /// The interactive URL for the user.
    url: String,
    /// Anchor-assigned transaction ID.
    id: String,
}

#[derive(Debug, Deserialize)]
struct AnchorSep31Response {
    transaction: AnchorSep31Transaction,
}

#[derive(Debug, Deserialize)]
struct AnchorSep31Transaction {
    id: String,
    stellar_account_id: Option<String>,
    stellar_memo_type: Option<String>,
    stellar_memo: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AnchorTxStatusResponse {
    transaction: AnchorTxDetail,
}

#[derive(Debug, Deserialize)]
struct AnchorTxDetail {
    status: String,
}

// ──────────────────────────────────────────────────────────────────────────────
// AnchorService
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct AnchorService {
    db_pool: Arc<Pool>,
    config: Config,
    http: Client,
}

impl AnchorService {
    pub fn new(db_pool: Arc<Pool>, config: Config) -> Self {
        let http = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("Failed to build reqwest client");
        Self {
            db_pool,
            config,
            http,
        }
    }

    // ──────────────────────────────────────────────────────────────────────────
    // SEP-12: KYC Status Check
    // ──────────────────────────────────────────────────────────────────────────

    /// Check whether a user is cleared for withdrawals at the configured Anchor.
    ///
    /// Calls `GET {sep24_url}/kyc?account={stellar_address}` with a service-level
    /// SEP-10 bearer token.  Returns `KycStatus::Cleared` only when the Anchor
    /// responds with `"CLEARED"`.
    pub async fn check_kyc_status(
        &self,
        user_id: &str,
        stellar_address: &str,
    ) -> Result<KycStatus, ApiError> {
        info!(user_id, stellar_address, "Checking KYC status at anchor");

        let token = self.build_sep10_token(stellar_address)?;
        let url = format!(
            "{}/kyc?account={}",
            self.config.anchor_config.sep24_url, stellar_address
        );

        let response = self
            .http
            .get(&url)
            .bearer_auth(&token)
            .send()
            .await
            .map_err(|e| {
                error!(error = %e, "Failed to reach anchor KYC endpoint");
                ApiError::InternalServerError
            })?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            warn!(user_id, "No KYC record found at anchor");
            return Ok(KycStatus::NotFound);
        }

        let body: AnchorKycResponse = response.json().await.map_err(|e| {
            error!(error = %e, "Failed to parse anchor KYC response");
            ApiError::InternalServerError
        })?;

        let status = match body.status.as_deref() {
            Some("CLEARED") | Some("cleared") => KycStatus::Cleared,
            Some("PENDING") | Some("pending") => KycStatus::Pending,
            Some("REJECTED") | Some("rejected") => KycStatus::Rejected,
            _ => KycStatus::NotFound,
        };

        info!(user_id, kyc_status = %status, "KYC status check complete");
        Ok(status)
    }

    // ──────────────────────────────────────────────────────────────────────────
    // SEP-24: Interactive Withdrawal URL
    // ──────────────────────────────────────────────────────────────────────────

    /// Obtain a SEP-24 interactive withdrawal URL for the given user.
    ///
    /// The returned URL should be sent back to the client/mobile app.  The user
    /// opens it in a browser to complete the Anchor's KYC/bank-details form.
    /// The `anchor_tx_id` should be persisted immediately so we can reconcile
    /// incoming webhook callbacks.
    pub async fn get_sep24_interactive_url(
        &self,
        user_id: &str,
        stellar_address: &str,
        asset: &str,
        amount: i64,
    ) -> Result<Sep24InteractiveResponse, ApiError> {
        info!(user_id, asset, amount, "Requesting SEP-24 interactive URL");

        let token = self.build_sep10_token(stellar_address)?;
        let endpoint = format!(
            "{}/transactions/withdraw/interactive",
            self.config.anchor_config.sep24_url
        );

        let body = serde_json::json!({
            "asset_code": asset,
            "account": stellar_address,
            "amount": amount.to_string(),
            "lang": "en",
        });

        let response = self
            .http
            .post(&endpoint)
            .bearer_auth(&token)
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                error!(error = %e, "Failed to reach anchor SEP-24 endpoint");
                ApiError::InternalServerError
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            error!(http_status = %status, body = %text, "Anchor returned non-2xx for SEP-24");
            return Err(ApiError::InternalServerError);
        }

        let parsed: AnchorSep24Response = response.json().await.map_err(|e| {
            error!(error = %e, "Failed to parse SEP-24 anchor response");
            ApiError::InternalServerError
        })?;

        info!(
            user_id,
            anchor_tx_id = %parsed.id,
            "SEP-24 interactive URL obtained"
        );

        Ok(Sep24InteractiveResponse {
            url: parsed.url,
            anchor_tx_id: parsed.id,
        })
    }

    // ──────────────────────────────────────────────────────────────────────────
    // SEP-31: Backend-to-Backend Cross-Border Payout
    // ──────────────────────────────────────────────────────────────────────────

    /// Initiate a SEP-31 cross-border payout directly from our backend to the Anchor.
    ///
    /// This does NOT involve a browser UI — our server POSTs to the Anchor's
    /// `/transactions` endpoint.  The response includes a Stellar account + memo
    /// that we must use when submitting the on-chain payment.
    pub async fn initiate_sep31_payout(
        &self,
        params: &Sep31PayoutParams,
    ) -> Result<Sep31PayoutResponse, ApiError> {
        info!(
            sender_id = %params.sender_id,
            asset = %params.asset_code,
            amount = %params.amount,
            "Initiating SEP-31 payout"
        );

        let endpoint = format!("{}/transactions", self.config.anchor_config.sep31_url);
        let token = self.build_sep10_token(&params.sender_id)?;

        let mut body = serde_json::json!({
            "amount": params.amount,
            "asset_code": params.asset_code,
            "sender_id": params.sender_id,
            "receiver_id": params.receiver_id,
        });

        if let Some(issuer) = &params.asset_issuer {
            body["asset_issuer"] = serde_json::json!(issuer);
        }
        if let Some(memo) = &params.memo {
            body["memo"] = serde_json::json!(memo);
            body["memo_type"] = serde_json::json!("text");
        }

        let response = self
            .http
            .post(&endpoint)
            .bearer_auth(&token)
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                error!(error = %e, "Failed to reach anchor SEP-31 endpoint");
                ApiError::InternalServerError
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            error!(http_status = %status, body = %text, "Anchor returned non-2xx for SEP-31");
            return Err(ApiError::InternalServerError);
        }

        let parsed: AnchorSep31Response = response.json().await.map_err(|e| {
            error!(error = %e, "Failed to parse SEP-31 anchor response");
            ApiError::InternalServerError
        })?;

        info!(anchor_tx_id = %parsed.transaction.id, "SEP-31 payout initiated");

        Ok(Sep31PayoutResponse {
            anchor_tx_id: parsed.transaction.id,
            stellar_account_id: parsed.transaction.stellar_account_id,
            stellar_memo_type: parsed.transaction.stellar_memo_type,
            stellar_memo: parsed.transaction.stellar_memo,
        })
    }

    // ──────────────────────────────────────────────────────────────────────────
    // Transaction Status Polling
    // ──────────────────────────────────────────────────────────────────────────

    /// Poll the Anchor for the current status of a transaction (SEP-24 or SEP-31).
    ///
    /// Calls `GET {sep24_url}/transaction?id={anchor_tx_id}`.
    pub async fn poll_anchor_tx_status(
        &self,
        anchor_tx_id: &str,
    ) -> Result<AnchorTxStatus, ApiError> {
        let url = format!(
            "{}/transaction?id={}",
            self.config.anchor_config.sep24_url, anchor_tx_id
        );

        let response = self.http.get(&url).send().await.map_err(|e| {
            error!(error = %e, "Failed to poll anchor TX status");
            ApiError::InternalServerError
        })?;

        let body: AnchorTxStatusResponse = response.json().await.map_err(|e| {
            error!(error = %e, "Failed to parse anchor TX status response");
            ApiError::InternalServerError
        })?;

        let status = match body.transaction.status.as_str() {
            "pending_stellar" => AnchorTxStatus::PendingStellar,
            "pending_anchor" => AnchorTxStatus::PendingAnchor,
            "pending_external" => AnchorTxStatus::PendingExternal,
            "pending_user" => AnchorTxStatus::PendingUser,
            "pending_user_transfer_start" => AnchorTxStatus::PendingUserTransferStart,
            "completed" => AnchorTxStatus::Completed,
            "refunded" => AnchorTxStatus::Refunded,
            "expired" => AnchorTxStatus::Expired,
            "error" => AnchorTxStatus::Error,
            _ => AnchorTxStatus::Incomplete,
        };

        Ok(status)
    }

    // ──────────────────────────────────────────────────────────────────────────
    // Webhook Signature Verification
    // ──────────────────────────────────────────────────────────────────────────

    /// Verify the HMAC-SHA256 signature on an incoming anchor webhook.
    ///
    /// The Anchor sends the signature as a hex-encoded string in the
    /// `X-Stellar-Signature` header.  We recompute HMAC-SHA256 over the raw
    /// request body using `config.anchor_config.webhook_secret`.
    pub fn verify_webhook_signature(
        &self,
        payload: &[u8],
        signature_header: &str,
    ) -> Result<(), ApiError> {
        let key = hmac::Key::new(
            hmac::HMAC_SHA256,
            self.config.anchor_config.webhook_secret.as_bytes(),
        );

        // Signature may arrive as hex or base64 — try both
        let expected_tag = hmac::sign(&key, payload);
        let expected_hex = hex::encode(expected_tag.as_ref());
        let expected_b64 = B64.encode(expected_tag.as_ref());

        let header = signature_header.trim();
        // Strip common prefixes e.g. "sha256=" sent by some anchors
        let sig = header.strip_prefix("sha256=").unwrap_or(header);

        if sig != expected_hex && sig != expected_b64 {
            warn!("Anchor webhook signature mismatch");
            return Err(ApiError::Authentication(
                "Invalid webhook signature".to_string(),
            ));
        }

        Ok(())
    }

    // ──────────────────────────────────────────────────────────────────────────
    // Database helpers
    // ──────────────────────────────────────────────────────────────────────────

    /// Persist a new withdrawal record and return the created row.
    pub async fn create_withdrawal_record(
        &self,
        params: CreateWithdrawalParams,
    ) -> Result<WithdrawalRecord, ApiError> {
        let client = self.db_pool.get().await.map_err(|e| {
            error!(error = %e, "DB pool error");
            ApiError::InternalServerError
        })?;

        let id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now();

        client
            .execute(
                r#"
                INSERT INTO withdrawals
                    (id, user_id, destination_address, amount, asset, status,
                     anchor_tx_id, kyc_status, sep24_interactive_url, created_at, updated_at)
                VALUES
                    ($1, $2, $3, $4, $5, 'pending',
                     $6, $7, $8, $9, $10)
                "#,
                &[
                    &id,
                    &params.user_id,
                    &params.destination_address,
                    &params.amount,
                    &params.asset,
                    &params.anchor_tx_id,
                    &params.kyc_status.to_string(),
                    &params.sep24_interactive_url,
                    &now,
                    &now,
                ],
            )
            .await
            .map_err(|e| {
                error!(error = %e, "Failed to insert withdrawal record");
                ApiError::InternalServerError
            })?;

        Ok(WithdrawalRecord {
            id,
            user_id: params.user_id,
            destination_address: params.destination_address,
            amount: params.amount,
            asset: params.asset,
            status: "pending".to_string(),
            anchor_tx_id: params.anchor_tx_id,
            kyc_status: params.kyc_status.to_string(),
            sep24_interactive_url: params.sep24_interactive_url,
            created_at: now,
            updated_at: now,
        })
    }

    /// Fetch a withdrawal record by its internal ID.
    pub async fn get_withdrawal_by_id(
        &self,
        withdrawal_id: &str,
    ) -> Result<WithdrawalRecord, ApiError> {
        let client = self.db_pool.get().await.map_err(|e| {
            error!(error = %e, "DB pool error");
            ApiError::InternalServerError
        })?;

        let row = client
            .query_opt(
                r#"
                SELECT id, user_id, destination_address, amount, asset, status,
                       anchor_tx_id, kyc_status, sep24_interactive_url, created_at, updated_at
                FROM withdrawals
                WHERE id = $1
                "#,
                &[&withdrawal_id],
            )
            .await
            .map_err(|e| {
                error!(error = %e, "DB query failed");
                ApiError::InternalServerError
            })?
            .ok_or_else(|| ApiError::NotFound(format!("Withdrawal {} not found", withdrawal_id)))?;

        Ok(WithdrawalRecord {
            id: row.get("id"),
            user_id: row.get("user_id"),
            destination_address: row.get("destination_address"),
            amount: row.get("amount"),
            asset: row.get("asset"),
            status: row.get("status"),
            anchor_tx_id: row.get("anchor_tx_id"),
            kyc_status: row.get("kyc_status"),
            sep24_interactive_url: row.get("sep24_interactive_url"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
        })
    }

    /// Update the `status` and optionally `anchor_tx_id` of a withdrawal.
    pub async fn update_withdrawal_status(
        &self,
        withdrawal_id: &str,
        status: &str,
        anchor_tx_id: Option<&str>,
    ) -> Result<(), ApiError> {
        let client = self.db_pool.get().await.map_err(|e| {
            error!(error = %e, "DB pool error");
            ApiError::InternalServerError
        })?;

        let now = chrono::Utc::now();

        client
            .execute(
                r#"
                UPDATE withdrawals
                SET status = $1,
                    anchor_tx_id = COALESCE($2, anchor_tx_id),
                    updated_at = $3
                WHERE id = $4
                "#,
                &[&status, &anchor_tx_id, &now, &withdrawal_id],
            )
            .await
            .map_err(|e| {
                error!(error = %e, "Failed to update withdrawal status");
                ApiError::InternalServerError
            })?;

        info!(withdrawal_id, status, "Withdrawal status updated");
        Ok(())
    }

    // ──────────────────────────────────────────────────────────────────────────
    // Internal helpers
    // ──────────────────────────────────────────────────────────────────────────

    /// Build a minimal SEP-10 bearer token.
    ///
    /// In production this must be a proper signed JWT containing a challenge
    /// from the Anchor's `GET /auth` endpoint.  For now we produce a simple
    /// JWT-shaped bearer using the configured JWT secret, valid for 5 minutes.
    /// Replace this with a full SEP-10 challenge-response flow once you have
    /// the Anchor's auth endpoint wired up.
    fn build_sep10_token(&self, account: &str) -> Result<String, ApiError> {
        use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};

        #[derive(Serialize)]
        struct Sep10Claims<'a> {
            sub: &'a str,
            iat: i64,
            exp: i64,
        }

        let now = chrono::Utc::now();
        let claims = Sep10Claims {
            sub: account,
            iat: now.timestamp(),
            exp: (now + chrono::Duration::minutes(5)).timestamp(),
        };

        let key = EncodingKey::from_secret(self.config.jwt.secret.as_bytes());
        encode(&Header::new(Algorithm::HS256), &claims, &key).map_err(|e| {
            error!(error = %e, "Failed to generate SEP-10 token");
            ApiError::InternalServerError
        })
    }
}
