/// Anchor webhook handler — receives event callbacks from the Stellar Anchor.
///
/// The Anchor POSTs to this endpoint whenever the state of a transaction changes
/// (e.g. `pending_external` → `completed`).  We verify the HMAC-SHA256 signature
/// before processing to ensure authenticity.
use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{error, info, warn};

use crate::{api_error::ApiError, service::ServiceContainer};

// ──────────────────────────────────────────────────────────────────────────────
// Webhook payload shape
// ──────────────────────────────────────────────────────────────────────────────

/// Minimal shape of the anchor's webhook POST body.
/// Different anchor implementations may vary — extend as needed.
#[derive(Debug, Deserialize)]
pub struct AnchorWebhookPayload {
    /// The anchor's transaction ID (matches `anchor_tx_id` in our DB).
    pub transaction_id: String,
    /// New transaction status (e.g. `"completed"`, `"error"`, `"pending_external"`).
    pub status: String,
    /// Optional human-readable message from the Anchor.
    pub message: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct WebhookAck {
    pub received: bool,
}

// ──────────────────────────────────────────────────────────────────────────────
// Handler
// ──────────────────────────────────────────────────────────────────────────────

/// `POST /anchor/webhook`
///
/// 1. Verify the `X-Stellar-Signature` HMAC-SHA256 header.
/// 2. Parse the JSON body.
/// 3. Look up the withdrawal by `anchor_tx_id` and update its status.
///
/// Returns `200 OK` with `{"received": true}` on success so the Anchor stops
/// retrying.  Returns `401` on signature failure so misconfigured senders are
/// clearly rejected.
pub async fn anchor_webhook(
    State(services): State<Arc<ServiceContainer>>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<(StatusCode, Json<WebhookAck>), ApiError> {
    // ── Step 1: Verify signature ───────────────────────────────────────────────
    let sig = headers
        .get("X-Stellar-Signature")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if sig.is_empty() {
        warn!("Anchor webhook received without X-Stellar-Signature header");
        return Err(ApiError::Authentication(
            "Missing webhook signature".to_string(),
        ));
    }

    services.anchor.verify_webhook_signature(&body, sig)?;

    // ── Step 2: Parse payload ─────────────────────────────────────────────────
    let payload: AnchorWebhookPayload = serde_json::from_slice(&body).map_err(|e| {
        error!(error = %e, "Failed to parse anchor webhook payload");
        ApiError::Validation("Invalid webhook payload".to_string())
    })?;

    info!(
        anchor_tx_id = %payload.transaction_id,
        status = %payload.status,
        "Anchor webhook received"
    );

    // ── Step 3: Sync withdrawal status ────────────────────────────────────────
    // Map anchor status strings to our internal withdrawal status values
    let internal_status = match payload.status.as_str() {
        "completed" => "completed",
        "error" | "expired" => "failed",
        "refunded" => "refunded",
        "pending_stellar"
        | "pending_anchor"
        | "pending_external"
        | "pending_user"
        | "pending_user_transfer_start" => "processing",
        _ => "pending",
    };

    // Find the withdrawal by anchor_tx_id and update it
    match find_withdrawal_by_anchor_tx_id(&services, &payload.transaction_id).await {
        Some(withdrawal_id) => {
            services
                .anchor
                .update_withdrawal_status(&withdrawal_id, internal_status, None)
                .await?;

            if let Some(ref msg) = payload.message {
                info!(
                    withdrawal_id = %withdrawal_id,
                    anchor_message = %msg,
                    "Anchor webhook message logged"
                );
            }
        }
        None => {
            warn!(
                anchor_tx_id = %payload.transaction_id,
                "Anchor webhook received for unknown transaction — ignoring"
            );
        }
    }

    Ok((StatusCode::OK, Json(WebhookAck { received: true })))
}

/// Look up a withdrawal by its `anchor_tx_id` column.
/// Returns the withdrawal's internal UUID as a string, or `None` if not found.
async fn find_withdrawal_by_anchor_tx_id(
    services: &Arc<ServiceContainer>,
    anchor_tx_id: &str,
) -> Option<String> {
    let client = services.db_pool.get().await.ok()?;

    let row = client
        .query_opt(
            "SELECT id FROM withdrawals WHERE anchor_tx_id = $1",
            &[&anchor_tx_id],
        )
        .await
        .ok()??;

    Some(row.get::<_, String>("id"))
}
