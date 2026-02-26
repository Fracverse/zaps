use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::info;
use uuid::Uuid;

use crate::{
    api_error::ApiError,
    middleware::auth::AuthenticatedUser,
    service::{
        anchor_service::{CreateWithdrawalParams, KycStatus, Sep31PayoutParams},
        ServiceContainer,
    },
};

// ──────────────────────────────────────────────────────────────────────────────
// Request / Response shapes
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CreateWithdrawalRequest {
    pub destination_address: String,
    /// Amount in the asset's smallest unit (e.g. stroops for XLM).
    pub amount: i64,
    /// Stellar asset code e.g. "USDC".
    pub asset: String,
}

#[derive(Debug, Deserialize)]
pub struct InitiateSep31PayoutRequest {
    pub amount: i64,
    pub asset_code: String,
    pub asset_issuer: Option<String>,
    pub receiver_id: String,
    pub memo: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct WithdrawalResponse {
    pub id: String,
    pub user_id: String,
    pub destination_address: String,
    pub amount: i64,
    pub asset: String,
    pub status: String,
    pub anchor_tx_id: Option<String>,
    pub kyc_status: String,
    /// The SEP-24 interactive URL the client must open in a browser/web-view.
    /// `null` for SEP-31 (backend-only) payouts.
    pub sep24_interactive_url: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize)]
pub struct WithdrawalStatusResponse {
    pub id: String,
    pub status: String,
    /// Live status from the Anchor (may differ from our DB copy until reconciled).
    pub anchor_status: Option<String>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize)]
pub struct Sep31PayoutInitResponse {
    pub anchor_tx_id: String,
    pub stellar_account_id: Option<String>,
    pub stellar_memo_type: Option<String>,
    pub stellar_memo: Option<String>,
}

// ──────────────────────────────────────────────────────────────────────────────
// Handlers
// ──────────────────────────────────────────────────────────────────────────────

/// `POST /withdrawals`
///
/// SEP-24 withdrawal flow:
/// 1. Gate on KYC status at the Anchor (`CLEARED` required if `kyc_required = true`).
/// 2. Obtain a SEP-24 interactive URL + `anchor_tx_id` from the Anchor.
/// 3. Persist the withdrawal record.
/// 4. Return the interactive URL to the client → client opens it in a browser/web-view.
pub async fn create_withdrawal(
    State(services): State<Arc<ServiceContainer>>,
    auth: AuthenticatedUser,
    Json(request): Json<CreateWithdrawalRequest>,
) -> Result<(StatusCode, Json<WithdrawalResponse>), ApiError> {
    let user_id = &auth.user_id;

    // Resolve the user's Stellar address from identity service
    let wallet = services
        .identity
        .get_user_wallet(user_id)
        .await
        .map_err(|_| ApiError::NotFound(format!("No wallet found for user {}", user_id)))?;
    let stellar_address = &wallet.address;

    // ── Step 1: KYC gate ──────────────────────────────────────────────────────
    let kyc_status = if services.config.anchor_config.kyc_required {
        let status = services
            .anchor
            .check_kyc_status(user_id, stellar_address)
            .await?;

        if status != KycStatus::Cleared {
            return Err(ApiError::Authorization(format!(
                "KYC check failed: your status is {}. \
                 Please complete identity verification at the anchor before withdrawing.",
                status
            )));
        }
        status
    } else {
        KycStatus::Cleared
    };

    // ── Step 2: Obtain SEP-24 interactive URL ─────────────────────────────────
    let sep24 = services
        .anchor
        .get_sep24_interactive_url(user_id, stellar_address, &request.asset, request.amount)
        .await?;

    info!(
        user_id,
        anchor_tx_id = %sep24.anchor_tx_id,
        "SEP-24 URL obtained — persisting withdrawal"
    );

    // ── Step 3: Persist withdrawal record ─────────────────────────────────────
    let record = services
        .anchor
        .create_withdrawal_record(CreateWithdrawalParams {
            user_id: user_id.clone(),
            destination_address: request.destination_address.clone(),
            amount: request.amount,
            asset: request.asset.clone(),
            anchor_tx_id: Some(sep24.anchor_tx_id.clone()),
            kyc_status,
            sep24_interactive_url: Some(sep24.url.clone()),
        })
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(WithdrawalResponse {
            id: record.id,
            user_id: record.user_id,
            destination_address: record.destination_address,
            amount: record.amount,
            asset: record.asset,
            status: record.status,
            anchor_tx_id: record.anchor_tx_id,
            kyc_status: record.kyc_status,
            sep24_interactive_url: record.sep24_interactive_url,
            created_at: record.created_at,
        }),
    ))
}

/// `GET /withdrawals/:id`
///
/// Fetch the current state of a withdrawal from our database.
pub async fn get_withdrawal(
    State(services): State<Arc<ServiceContainer>>,
    Path(withdrawal_id): Path<Uuid>,
) -> Result<Json<WithdrawalResponse>, ApiError> {
    let record = services
        .anchor
        .get_withdrawal_by_id(&withdrawal_id.to_string())
        .await?;

    Ok(Json(WithdrawalResponse {
        id: record.id,
        user_id: record.user_id,
        destination_address: record.destination_address,
        amount: record.amount,
        asset: record.asset,
        status: record.status,
        anchor_tx_id: record.anchor_tx_id,
        kyc_status: record.kyc_status,
        sep24_interactive_url: record.sep24_interactive_url,
        created_at: record.created_at,
    }))
}

/// `GET /withdrawals/:id/status`
///
/// Returns our DB status AND a live probe of the Anchor's status so the client
/// always has the freshest view.
pub async fn get_withdrawal_status(
    State(services): State<Arc<ServiceContainer>>,
    Path(withdrawal_id): Path<Uuid>,
) -> Result<Json<WithdrawalStatusResponse>, ApiError> {
    let record = services
        .anchor
        .get_withdrawal_by_id(&withdrawal_id.to_string())
        .await?;

    // Optionally probe the anchor for live status
    let anchor_status = if let Some(ref tx_id) = record.anchor_tx_id {
        match services.anchor.poll_anchor_tx_status(tx_id).await {
            Ok(status) => {
                let label = format!("{:?}", status).to_lowercase();
                // Reconcile: if anchor says completed/failed, sync our DB
                if matches!(
                    label.as_str(),
                    "completed" | "error" | "refunded" | "expired"
                ) {
                    let _ = services
                        .anchor
                        .update_withdrawal_status(&withdrawal_id.to_string(), &label, None)
                        .await;
                }
                Some(label)
            }
            Err(e) => {
                tracing::warn!(error = %e, "Failed to poll anchor status — returning cached");
                None
            }
        }
    } else {
        None
    };

    Ok(Json(WithdrawalStatusResponse {
        id: record.id,
        status: record.status,
        anchor_status,
        updated_at: record.updated_at,
    }))
}

/// `POST /withdrawals/sep31`
///
/// Initiate a SEP-31 backend-to-backend cross-border payout.
/// No interactive URL is generated — the caller is responsible for
/// submitting the on-chain Stellar payment to the returned `stellar_account_id`.
pub async fn initiate_sep31_payout(
    State(services): State<Arc<ServiceContainer>>,
    auth: AuthenticatedUser,
    Json(request): Json<InitiateSep31PayoutRequest>,
) -> Result<Json<Sep31PayoutInitResponse>, ApiError> {
    let result = services
        .anchor
        .initiate_sep31_payout(&Sep31PayoutParams {
            amount: request.amount.to_string(),
            asset_code: request.asset_code,
            asset_issuer: request.asset_issuer,
            sender_id: auth.user_id.clone(),
            receiver_id: request.receiver_id,
            memo: request.memo,
        })
        .await?;

    Ok(Json(Sep31PayoutInitResponse {
        anchor_tx_id: result.anchor_tx_id,
        stellar_account_id: result.stellar_account_id,
        stellar_memo_type: result.stellar_memo_type,
        stellar_memo: result.stellar_memo,
    }))
}
