use axum::{extract::{Path, State}, Json};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::{api_error::ApiError, service::{ServiceContainer, payment_service::CreatePaymentRequest}};

#[derive(Debug, Serialize)]
pub struct PaymentResponse {
    pub id: Uuid,
    pub tx_hash: Option<String>,
    pub from_address: String,
    pub merchant_id: String,
    pub send_asset: String,
    pub send_amount: i64,
    pub receive_amount: Option<i64>,
    pub status: String,
    pub memo: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize)]
pub struct PaymentStatusResponse {
    pub id: Uuid,
    pub status: String,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct QrPaymentRequest {
    pub merchant_id: String,
    pub amount: i64,
    pub asset: String,
    pub memo: Option<String>,
    pub expiry: i64,
}

#[derive(Debug, Serialize)]
pub struct QrPaymentResponse {
    pub qr_data: String,
    pub merchant_id: String,
    pub amount: i64,
    pub asset: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NfcPaymentRequest {
    pub merchant_id: String,
    pub amount: i64,
    pub asset: String,
    pub memo: Option<String>,
    pub timestamp: i64,
}

#[derive(Debug, Serialize)]
pub struct NfcValidationResponse {
    pub valid: bool,
    pub merchant_id: String,
    pub amount: i64,
}

pub async fn create_payment(
    State(services): State<Arc<ServiceContainer>>,
    Json(request): Json<CreatePaymentRequest>,
) -> Result<Json<PaymentResponse>, ApiError> {
    // Get user from auth context (would need to implement proper auth extraction)
    // For now, using a placeholder address
    let from_address = "GEXAMPLE_ADDRESS".to_string();

    let payment = services.payment.create_payment(from_address, request).await?;

    Ok(Json(PaymentResponse {
        id: payment.id,
        tx_hash: payment.tx_hash,
        from_address: payment.from_address,
        merchant_id: payment.merchant_id,
        send_asset: payment.send_asset,
        send_amount: payment.send_amount,
        receive_amount: payment.receive_amount,
        status: payment.status.to_string(),
        memo: payment.memo,
        created_at: payment.created_at,
    }))
}

pub async fn get_payment(
    State(services): State<Arc<ServiceContainer>>,
    Path(payment_id): Path<Uuid>,
) -> Result<Json<PaymentResponse>, ApiError> {
    let payment = services.payment.get_payment(payment_id).await?;

    Ok(Json(PaymentResponse {
        id: payment.id,
        tx_hash: payment.tx_hash,
        from_address: payment.from_address,
        merchant_id: payment.merchant_id,
        send_asset: payment.send_asset,
        send_amount: payment.send_amount,
        receive_amount: payment.receive_amount,
        status: payment.status.to_string(),
        memo: payment.memo,
        created_at: payment.created_at,
    }))
}

pub async fn get_payment_status(
    State(services): State<Arc<ServiceContainer>>,
    Path(payment_id): Path<Uuid>,
) -> Result<Json<PaymentStatusResponse>, ApiError> {
    let payment = services.payment.get_payment(payment_id).await?;

    Ok(Json(PaymentStatusResponse {
        id: payment.id,
        status: payment.status.to_string(),
        updated_at: payment.updated_at,
    }))
}

pub async fn generate_qr(
    State(services): State<Arc<ServiceContainer>>,
    Json(request): Json<QrPaymentRequest>,
) -> Result<Json<QrPaymentResponse>, ApiError> {
    let qr_data = services.payment.generate_qr_payment(request.clone()).await?;

    Ok(Json(QrPaymentResponse {
        qr_data,
        merchant_id: request.merchant_id,
        amount: request.amount,
        asset: request.asset,
    }))
}

pub async fn validate_nfc(
    State(services): State<Arc<ServiceContainer>>,
    Json(request): Json<NfcPaymentRequest>,
) -> Result<Json<NfcValidationResponse>, ApiError> {
    let valid = services.payment.validate_nfc_payment(request.clone()).await?;

    Ok(Json(NfcValidationResponse {
        valid,
        merchant_id: request.merchant_id,
        amount: request.amount,
    }))
}