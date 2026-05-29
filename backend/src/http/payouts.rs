use axum::{
    extract::{Path, State},
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::{
    api_error::ApiError,
    middleware::auth::AuthenticatedUser,
    models::Payout,
    service::{payout_service::CreatePayoutRequest, ServiceContainer},
};

#[derive(Debug, Serialize)]
pub struct PayoutResponse {
    pub id: String,
    pub merchant_id: String,
    pub batch_id: Option<String>,
    pub amount: i64,
    pub currency: String,
    pub destination_address: String,
    pub status: String,
    pub tx_hash: Option<String>,
    pub failure_reason: Option<String>,
    pub retry_count: i32,
    pub scheduled_at: Option<chrono::DateTime<chrono::Utc>>,
    pub processed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize)]
pub struct PayoutBatchResponse {
    pub id: String,
    pub merchant_id: String,
    pub total_amount: i64,
    pub currency: String,
    pub payout_count: i32,
    pub status: String,
    pub scheduled_at: chrono::DateTime<chrono::Utc>,
    pub processed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreatePayoutBatchRequest {
    pub merchant_id: String,
    pub payouts: Vec<CreatePayoutRequest>,
    pub scheduled_at: chrono::DateTime<chrono::Utc>,
}

fn payout_to_response(payout: Payout) -> PayoutResponse {
    PayoutResponse {
        id: payout.id,
        merchant_id: payout.merchant_id,
        batch_id: payout.batch_id,
        amount: payout.amount,
        currency: payout.currency,
        destination_address: payout.destination_address,
        status: payout.status.to_string(),
        tx_hash: payout.tx_hash,
        failure_reason: payout.failure_reason,
        retry_count: payout.retry_count,
        scheduled_at: payout.scheduled_at,
        processed_at: payout.processed_at,
        created_at: payout.created_at,
        updated_at: payout.updated_at,
    }
}

/// Create a single payout
pub async fn create_payout(
    State(services): State<Arc<ServiceContainer>>,
    Json(request): Json<CreatePayoutRequest>,
) -> Result<Json<PayoutResponse>, ApiError> {
    let payout = services.payout.create_payout(request).await?;

    // Log audit event
    let _ = services
        .audit
        .log_admin_action(
            "system",
            "payout_created",
            "payout",
            Some(payout.id.clone()),
            Some(serde_json::json!({
                "amount": payout.amount,
                "currency": payout.currency,
                "merchant_id": payout.merchant_id
            })),
            None,
            None,
        )
        .await;

    Ok(Json(payout_to_response(payout)))
}

/// Get payout by ID
pub async fn get_payout(
    State(services): State<Arc<ServiceContainer>>,
    Path(payout_id): Path<String>,
) -> Result<Json<PayoutResponse>, ApiError> {
    let payout = services.payout.get_payout(&payout_id).await?;
    Ok(Json(payout_to_response(payout)))
}

/// Create a batch of payouts
pub async fn create_payout_batch(
    State(services): State<Arc<ServiceContainer>>,
    Json(request): Json<CreatePayoutBatchRequest>,
) -> Result<Json<PayoutBatchResponse>, ApiError> {
    let batch_request = crate::service::payout_service::CreatePayoutBatchRequest {
        merchant_id: request.merchant_id.clone(),
        payouts: request.payouts,
        scheduled_at: request.scheduled_at,
    };

    let batch = services.payout.create_payout_batch(batch_request).await?;

    // Log audit event
    let _ = services
        .audit
        .log_admin_action(
            "system",
            "payout_batch_created",
            "payout_batch",
            Some(batch.id.clone()),
            Some(serde_json::json!({
                "total_amount": batch.total_amount,
                "payout_count": batch.payout_count,
                "merchant_id": batch.merchant_id
            })),
            None,
            None,
        )
        .await;

    Ok(Json(PayoutBatchResponse {
        id: batch.id,
        merchant_id: batch.merchant_id,
        total_amount: batch.total_amount,
        currency: batch.currency,
        payout_count: batch.payout_count,
        status: batch.status.to_string(),
        scheduled_at: batch.scheduled_at,
        processed_at: batch.processed_at,
        created_at: batch.created_at,
        updated_at: batch.updated_at,
    }))
}

/// Get payout batch by ID
pub async fn get_payout_batch(
    State(services): State<Arc<ServiceContainer>>,
    Path(batch_id): Path<String>,
) -> Result<Json<PayoutBatchResponse>, ApiError> {
    let batch = services.payout.get_payout_batch(&batch_id).await?;

    Ok(Json(PayoutBatchResponse {
        id: batch.id,
        merchant_id: batch.merchant_id,
        total_amount: batch.total_amount,
        currency: batch.currency,
        payout_count: batch.payout_count,
        status: batch.status.to_string(),
        scheduled_at: batch.scheduled_at,
        processed_at: batch.processed_at,
        created_at: batch.created_at,
        updated_at: batch.updated_at,
    }))
}

/// Retry a failed payout
pub async fn retry_payout(
    State(services): State<Arc<ServiceContainer>>,
    Path(payout_id): Path<String>,
) -> Result<Json<PayoutResponse>, ApiError> {
    services.payout.retry_failed_payout(&payout_id).await?;
    let payout = services.payout.get_payout(&payout_id).await?;

    // Log audit event
    let _ = services
        .audit
        .log_admin_action(
            "system",
            "payout_retry",
            "payout",
            Some(payout.id.clone()),
            Some(serde_json::json!({
                "retry_count": payout.retry_count
            })),
            None,
            None,
        )
        .await;

    Ok(Json(payout_to_response(payout)))
}
