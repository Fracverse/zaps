use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::{
    api_error::ApiError,
    middleware::auth::AuthenticatedUser,
    service::{ServiceContainer, MetricsService},
};

#[derive(Debug, Deserialize)]
pub struct CreateBatchRequest {
    pub merchant_id: String,
    pub asset: String,
    pub batch_size: usize,
    pub timeout_secs: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct AddPaymentToBatchRequest {
    pub payment_id: String,
}

#[derive(Debug, Serialize)]
pub struct BatchResponse {
    pub id: String,
    pub batch_key: String,
    pub merchant_id: String,
    pub status: String,
    pub total_amount: i64,
    pub total_count: i32,
    pub processed_count: i32,
    pub failed_count: i32,
    pub asset: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize)]
pub struct BatchItemResponse {
    pub id: String,
    pub batch_id: String,
    pub payment_id: String,
    pub status: String,
    pub error_message: Option<String>,
    pub retry_count: i32,
}

#[derive(Debug, Serialize)]
pub struct BatchReportResponse {
    pub batch_id: String,
    pub total_items: i32,
    pub processed_items: i32,
    pub failed_items: i32,
    pub success_rate: f64,
    pub total_amount: i64,
    pub status: String,
}

/// `POST /batches`
/// Create a new payment batch
pub async fn create_batch(
    State(services): State<Arc<ServiceContainer>>,
    _auth: AuthenticatedUser,
    Json(request): Json<CreateBatchRequest>,
) -> Result<(StatusCode, Json<BatchResponse>), ApiError> {
    if request.batch_size == 0 || request.batch_size > 10000 {
        return Err(ApiError::Validation(
            "Batch size must be between 1 and 10000".to_string(),
        ));
    }

    let batch = services
        .batch
        .create_batch(&request.merchant_id, &request.asset, request.batch_size)
        .await?;

    MetricsService::record_business_event("batch", "created");

    Ok((
        StatusCode::CREATED,
        Json(BatchResponse {
            id: batch.id,
            batch_key: batch.batch_key,
            merchant_id: batch.merchant_id,
            status: batch.status,
            total_amount: batch.total_amount,
            total_count: batch.total_count,
            processed_count: batch.processed_count,
            failed_count: batch.failed_count,
            asset: batch.asset,
            created_at: batch.created_at,
            updated_at: batch.updated_at,
        }),
    ))
}

/// `POST /batches/:batch_id/items`
/// Add a payment to a batch
pub async fn add_payment_to_batch(
    State(services): State<Arc<ServiceContainer>>,
    _auth: AuthenticatedUser,
    Path(batch_id): Path<String>,
    Json(request): Json<AddPaymentToBatchRequest>,
) -> Result<(StatusCode, Json<BatchItemResponse>), ApiError> {
    // Verify batch exists
    let _batch = services
        .batch
        .get_batch(&batch_id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Batch not found".to_string()))?;

    // Verify batch is still pending
    if _batch.status != "pending" {
        return Err(ApiError::Validation(
            "Cannot add items to a non-pending batch".to_string(),
        ));
    }

    let item = services
        .batch
        .add_payment_to_batch(&batch_id, &request.payment_id)
        .await?;

    MetricsService::record_business_event("batch_item", "added");

    Ok((
        StatusCode::CREATED,
        Json(BatchItemResponse {
            id: item.id,
            batch_id: item.batch_id,
            payment_id: item.payment_id,
            status: item.status,
            error_message: item.error_message,
            retry_count: item.retry_count,
        }),
    ))
}

/// `GET /batches/:batch_id`
/// Get batch details
pub async fn get_batch(
    State(services): State<Arc<ServiceContainer>>,
    _auth: AuthenticatedUser,
    Path(batch_id): Path<String>,
) -> Result<Json<BatchResponse>, ApiError> {
    let batch = services
        .batch
        .get_batch(&batch_id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Batch not found".to_string()))?;

    Ok(Json(BatchResponse {
        id: batch.id,
        batch_key: batch.batch_key,
        merchant_id: batch.merchant_id,
        status: batch.status,
        total_amount: batch.total_amount,
        total_count: batch.total_count,
        processed_count: batch.processed_count,
        failed_count: batch.failed_count,
        asset: batch.asset,
        created_at: batch.created_at,
        updated_at: batch.updated_at,
    }))
}

/// `GET /batches/:batch_id/report`
/// Get batch processing report
pub async fn get_batch_report(
    State(services): State<Arc<ServiceContainer>>,
    _auth: AuthenticatedUser,
    Path(batch_id): Path<String>,
) -> Result<Json<BatchReportResponse>, ApiError> {
    let report = services
        .batch
        .get_batch_report(&batch_id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Batch not found".to_string()))?;

    Ok(Json(BatchReportResponse {
        batch_id: report.batch_id,
        total_items: report.total_items,
        processed_items: report.processed_items,
        failed_items: report.failed_items,
        success_rate: report.success_rate,
        total_amount: report.total_amount,
        status: report.status,
    }))
}

/// `POST /batches/:batch_id/process`
/// Process a batch with retry logic
pub async fn process_batch(
    State(services): State<Arc<ServiceContainer>>,
    _auth: AuthenticatedUser,
    Path(batch_id): Path<String>,
) -> Result<Json<BatchReportResponse>, ApiError> {
    // Verify batch exists
    let _batch = services
        .batch
        .get_batch(&batch_id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Batch not found".to_string()))?;

    // Update batch status to processing
    services.batch.update_batch_status(&batch_id, "processing").await?;

    // Process with retry logic
    let report = services.batch.process_batch_with_retry(&batch_id, 3).await?;

    MetricsService::record_business_event("batch", "processed");

    Ok(Json(BatchReportResponse {
        batch_id: report.batch_id,
        total_items: report.total_items,
        processed_items: report.processed_items,
        failed_items: report.failed_items,
        success_rate: report.success_rate,
        total_amount: report.total_amount,
        status: report.status,
    }))
}

/// `GET /batches/:batch_id/items`
/// List all items in a batch
pub async fn get_batch_items(
    State(services): State<Arc<ServiceContainer>>,
    _auth: AuthenticatedUser,
    Path(batch_id): Path<String>,
) -> Result<Json<Vec<BatchItemResponse>>, ApiError> {
    let items = services.batch.get_batch_items(&batch_id).await?;

    Ok(Json(
        items
            .into_iter()
            .map(|i| BatchItemResponse {
                id: i.id,
                batch_id: i.batch_id,
                payment_id: i.payment_id,
                status: i.status,
                error_message: i.error_message,
                retry_count: i.retry_count,
            })
            .collect(),
    ))
}

/// `GET /batches/merchant/:merchant_id`
/// Get pending batches for a merchant
pub async fn get_merchant_batches(
    State(services): State<Arc<ServiceContainer>>,
    _auth: AuthenticatedUser,
    Path(merchant_id): Path<String>,
) -> Result<Json<Vec<BatchResponse>>, ApiError> {
    let batches = services
        .batch
        .get_pending_batches(&merchant_id)
        .await?;

    Ok(Json(
        batches
            .into_iter()
            .map(|b| BatchResponse {
                id: b.id,
                batch_key: b.batch_key,
                merchant_id: b.merchant_id,
                status: b.status,
                total_amount: b.total_amount,
                total_count: b.total_count,
                processed_count: b.processed_count,
                failed_count: b.failed_count,
                asset: b.asset,
                created_at: b.created_at,
                updated_at: b.updated_at,
            })
            .collect(),
    ))
}
