use axum::{
    extract::{Path, State},
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Instant;
use uuid::Uuid;

use crate::{
    api_error::ApiError,
    models::RiskLevel,
    service::MetricsService,
    service::{payment_service::CreatePaymentRequest, ServiceContainer},
};
use crate::service::schedule_service::ScheduleService;

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
    // base64 XDR pre-sponsored by server as fee-payer (if available)
    pub sponsored_xdr: Option<String>,
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
    // Base64 XDR for QR code payload (pre-sponsored if fee payer available)
    pub xdr_payload: Option<String>,
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
    // Base64 XDR for NFC payload (pre-sponsored if fee payer available)
    pub xdr_payload: Option<String>,
}

pub async fn create_payment(
    State(services): State<Arc<ServiceContainer>>,
    Json(request): Json<CreatePaymentRequest>,
) -> Result<Json<PaymentResponse>, ApiError> {
    let start = Instant::now();
    // Get user from auth context (would need to implement proper auth extraction)
    // For now, using a placeholder address
    let from_address = "GEXAMPLE_ADDRESS".to_string();

    // Validate asset format early (XLM or CODE:ISSUER)
    services.soroban.validate_asset(&request.send_asset)?;

    // Ensure merchant exists and fetch vault address
    let merchant = services.payment.get_merchant(&request.merchant_id).await?;

    let risk_assessment = services
        .compliance
        .assess_transaction_risk("anonymous", &merchant.vault_address, request.send_amount)
        .await?;
    if risk_assessment.risk_level == RiskLevel::Blocked {
        MetricsService::record_business_event("payment", "blocked");
        return Err(ApiError::Compliance(
            "Payment blocked by sanctions screening".to_string(),
        ));
    }

    // Build payment XDR (base64) for client signing; this is pre-sponsorship build
    let tx_xdr = services
        .soroban
        .build_payment_xdr(
            &from_address,
            &merchant.vault_address,
            &request.send_asset,
            request.send_amount,
            request.memo.as_deref(),
        )
        .await?;

    // Optionally simulate to get accurate fees/footprint (not currently returned)
    let _sim = services.soroban.simulate_transaction(&tx_xdr).await?;

    // Sign as fee payer (server-side) to produce a pre-sponsored XDR
    let sponsored_xdr = services
        .soroban
        .sign_transaction_as_fee_payer(&tx_xdr)
        .await
        .map(Some)?;

    // Persist payment (status pending)
    let payment = services
        .payment
        .create_payment(from_address, request)
        .await?;
    let _ = services
        .cache
        .set_json(&format!("payment:{}", payment.id), &payment, None)
        .await;

    MetricsService::record_business_event("payment", "created");
    MetricsService::record_payment_transaction(
        &payment.merchant_id,
        "created",
        "api",
        &payment.send_asset,
        payment.send_amount,
    );
    MetricsService::record_payment_processing_duration("api", "created", start.elapsed().as_secs_f64());

    Ok(Json(PaymentResponse {
        id: Uuid::parse_str(&payment.id).unwrap_or_default(),
        tx_hash: payment.tx_hash,
        from_address: payment.from_address,
        merchant_id: payment.merchant_id,
        send_asset: payment.send_asset,
        send_amount: payment.send_amount,
        receive_amount: payment.receive_amount,
        status: payment.status.to_string(),
        memo: payment.memo,
        created_at: payment.created_at,
        sponsored_xdr,
    }))
}

pub async fn get_payment(
    State(services): State<Arc<ServiceContainer>>,
    Path(payment_id): Path<String>,
) -> Result<Json<PaymentResponse>, ApiError> {
    let payment_uuid = Uuid::parse_str(&payment_id)
        .map_err(|_| ApiError::Validation("Invalid Payment ID".to_string()))?;

    let cache_key = format!("payment:{}", payment_uuid);
    let payment = match services.cache.get_json(&cache_key).await? {
        Some(payment) => payment,
        None => {
            let payment = services.payment.get_payment(payment_uuid).await?;
            let _ = services.cache.set_json(&cache_key, &payment, None).await;
            payment
        }
    };

    Ok(Json(PaymentResponse {
        id: Uuid::parse_str(&payment.id).unwrap_or_default(),
        tx_hash: payment.tx_hash,
        from_address: payment.from_address,
        merchant_id: payment.merchant_id,
        send_asset: payment.send_asset,
        send_amount: payment.send_amount,
        receive_amount: payment.receive_amount,
        status: payment.status.to_string(),
        memo: payment.memo,
        created_at: payment.created_at,
        sponsored_xdr: None,
    }))
}

pub async fn get_payment_status(
    State(services): State<Arc<ServiceContainer>>,
    Path(payment_id): Path<String>,
) -> Result<Json<PaymentStatusResponse>, ApiError> {
    let payment_uuid = Uuid::parse_str(&payment_id)
        .map_err(|_| ApiError::Validation("Invalid Payment ID".to_string()))?;

    let payment = services.payment.get_payment(payment_uuid).await?;

    Ok(Json(PaymentStatusResponse {
        id: Uuid::parse_str(&payment.id).unwrap_or_default(),
        status: payment.status.to_string(),
        updated_at: payment.updated_at,
    }))
}

pub async fn generate_qr(
    State(services): State<Arc<ServiceContainer>>,
    Json(request): Json<QrPaymentRequest>,
) -> Result<Json<QrPaymentResponse>, ApiError> {
    // Validate asset format early
    services.soroban.validate_asset(&request.asset)?;

    // Get merchant vault address
    let merchant = services.payment.get_merchant(&request.merchant_id).await?;

    // Build XDR for QR payload
    let tx_xdr = services
        .soroban
        .build_payment_xdr(
            "GQRCODE_PLACEHOLDER", // Will be replaced by client with actual sender
            &merchant.vault_address,
            &request.asset,
            request.amount,
            request.memo.as_deref(),
        )
        .await?;

    // Sign as fee payer if available
    let xdr_payload = services
        .soroban
        .sign_transaction_as_fee_payer(&tx_xdr)
        .await
        .ok();

    let qr_data = services
        .payment
        .generate_qr_payment(request.clone())
        .await?;

    MetricsService::record_business_event("payment_qr", "generated");

    Ok(Json(QrPaymentResponse {
        qr_data,
        merchant_id: request.merchant_id,
        amount: request.amount,
        asset: request.asset,
        xdr_payload,
    }))
}

pub async fn validate_nfc(
    State(services): State<Arc<ServiceContainer>>,
    Json(request): Json<NfcPaymentRequest>,
) -> Result<Json<NfcValidationResponse>, ApiError> {
    // Validate asset format early
    services.soroban.validate_asset(&request.asset)?;

    // Get merchant vault address
    let merchant = services.payment.get_merchant(&request.merchant_id).await?;

    // Build XDR for NFC payload
    let tx_xdr = services
        .soroban
        .build_payment_xdr(
            "GNFC_PLACEHOLDER", // Will be replaced by client with actual sender
            &merchant.vault_address,
            &request.asset,
            request.amount,
            request.memo.as_deref(),
        )
        .await?;

    // Sign as fee payer if available
    let xdr_payload = services
        .soroban
        .sign_transaction_as_fee_payer(&tx_xdr)
        .await
        .ok();

    let valid = services
        .payment
        .validate_nfc_payment(request.clone())
        .await?;

    MetricsService::record_business_event("payment_nfc", if valid { "valid" } else { "invalid" });

    Ok(Json(NfcValidationResponse {
        valid,
        merchant_id: request.merchant_id,
        amount: request.amount,
        xdr_payload,
    }))
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateScheduleRequest {
    pub merchant_id: String,
    pub to_address: String,
    pub send_asset: String,
    pub send_amount: i64,
    pub memo: Option<String>,
    pub schedule_type: String, // ONE_TIME or RECURRING
    pub interval_seconds: Option<i64>,
    pub first_run: Option<chrono::DateTime<chrono::Utc>>,
}

pub async fn create_schedule(
    State(services): State<Arc<ServiceContainer>>,
    Json(request): Json<CreateScheduleRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let db_pool = services.db_pool.clone();
    let schedule_svc = ScheduleService::new(db_pool);

    let first_run = request
        .first_run
        .unwrap_or_else(|| chrono::Utc::now() + chrono::Duration::seconds(10));

    let schedule = if request.schedule_type == "ONE_TIME" {
        schedule_svc
            .create_one_time_schedule(
                &request.merchant_id,
                "GEXAMPLE_ADDRESS",
                &request.to_address,
                &request.send_asset,
                request.send_amount,
                request.memo.as_deref(),
                first_run,
            )
            .await?
    } else {
        let interval = request.interval_seconds.unwrap_or(86400);
        schedule_svc
            .create_recurring_schedule(
                &request.merchant_id,
                "GEXAMPLE_ADDRESS",
                &request.to_address,
                &request.send_asset,
                request.send_amount,
                request.memo.as_deref(),
                interval,
                first_run,
            )
            .await?
    };

    Ok(Json(serde_json::json!({"schedule_id": schedule.id})))
}

#[derive(Debug, Clone, Deserialize)]
pub struct ModifyScheduleRequest {
    pub interval_seconds: Option<i64>,
    pub next_run: Option<chrono::DateTime<chrono::Utc>>,
}

pub async fn modify_schedule(
    State(services): State<Arc<ServiceContainer>>,
    Path(schedule_id): Path<String>,
    Json(request): Json<ModifyScheduleRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let schedule_uuid = Uuid::parse_str(&schedule_id)
        .map_err(|_| ApiError::Validation("Invalid Schedule ID".to_string()))?;

    let db_pool = services.db_pool.clone();
    let schedule_svc = ScheduleService::new(db_pool);

    let schedule = schedule_svc
        .modify_schedule(schedule_uuid, request.interval_seconds, request.next_run)
        .await?;

    Ok(Json(serde_json::json!({
        "schedule_id": schedule.id,
        "next_run": schedule.next_run,
        "interval_seconds": schedule.interval_seconds,
    })))
}

pub async fn get_schedule(
    State(services): State<Arc<ServiceContainer>>,
    Path(schedule_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let schedule_uuid = Uuid::parse_str(&schedule_id)
        .map_err(|_| ApiError::Validation("Invalid Schedule ID".to_string()))?;

    let db_pool = services.db_pool.clone();
    let schedule_svc = ScheduleService::new(db_pool);
    let schedule = schedule_svc.get_schedule(schedule_uuid).await?;

    Ok(Json(serde_json::json!({
        "schedule_id": schedule.id,
        "merchant_id": schedule.merchant_id,
        "from_address": schedule.from_address,
        "to_address": schedule.to_address,
        "send_asset": schedule.send_asset,
        "send_amount": schedule.send_amount,
        "memo": schedule.memo,
        "schedule_type": schedule.schedule_type,
        "interval_seconds": schedule.interval_seconds,
        "next_run": schedule.next_run,
        "status": schedule.status,
        "retries": schedule.retries,
        "created_at": schedule.created_at,
        "updated_at": schedule.updated_at,
    })))
}

pub async fn get_schedule_runs(
    State(services): State<Arc<ServiceContainer>>,
    Path(schedule_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let schedule_uuid = Uuid::parse_str(&schedule_id)
        .map_err(|_| ApiError::Validation("Invalid Schedule ID".to_string()))?;

    let db_pool = services.db_pool.clone();
    let schedule_svc = ScheduleService::new(db_pool);
    let runs = schedule_svc.list_schedule_runs(schedule_uuid).await?;

    Ok(Json(serde_json::json!({
        "schedule_id": schedule_uuid,
        "runs": runs.iter().map(|run| serde_json::json!({
            "id": run.id,
            "attempted_at": run.attempted_at,
            "success": run.success,
            "error": run.error,
            "external_payment_id": run.external_payment_id,
        })).collect::<Vec<_>>()
    })))
}

pub async fn cancel_schedule(
    State(services): State<Arc<ServiceContainer>>,
    Path(schedule_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let schedule_uuid = Uuid::parse_str(&schedule_id)
        .map_err(|_| ApiError::Validation("Invalid Schedule ID".to_string()))?;

    let db_pool = services.db_pool.clone();
    let schedule_svc = ScheduleService::new(db_pool);

    schedule_svc.cancel_schedule(schedule_uuid).await?;

    Ok(Json(serde_json::json!({"cancelled": true})))
}

