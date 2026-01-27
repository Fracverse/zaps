use axum::{extract::State, http::StatusCode, response::Json, routing::get, Router};
use serde_json::{json, Value};
use std::sync::Arc;

use crate::{
    job_worker::JobWorker,
    middleware::auth::AuthenticatedUser,
    ApiError,
};

pub fn create_job_routes() -> Router<Arc<JobWorker>> {
    Router::new()
        .route("/jobs/stats", get(get_queue_stats))
        .route("/jobs/email", post(enqueue_email))
        .route("/jobs/notification", post(enqueue_notification))
        .route("/jobs/sync", post(enqueue_sync))
        .route("/jobs/blockchain", post(enqueue_blockchain_tx))
}

async fn get_queue_stats(
    State(worker): State<Arc<JobWorker>>,
    _user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let stats = worker.get_queue_stats().await
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?;

    Ok(Json(json!({
        "main_queue_size": stats.main_queue_size,
        "processing_size": stats.processing_size,
        "retry_size": stats.retry_size,
        "dead_letter_size": stats.dead_letter_size,
        "timestamp": chrono::Utc::now().to_rfc3339()
    })))
}

async fn enqueue_email(
    State(worker): State<Arc<JobWorker>>,
    _user: AuthenticatedUser,
    Json(payload): Json<Value>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let to = payload.get("to")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::BadRequest("Missing 'to' field".to_string()))?
        .to_string();

    let subject = payload.get("subject")
        .and_then(|v| v.as_str())
        .unwrap_or("No Subject")
        .to_string();

    let body = payload.get("body")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    crate::job_worker::enqueue_email_job(worker, to, subject, body).await
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?;

    Ok((StatusCode::ACCEPTED, Json(json!({
        "message": "Email job enqueued successfully",
        "timestamp": chrono::Utc::now().to_rfc3339()
    }))))
}

async fn enqueue_notification(
    State(worker): State<Arc<JobWorker>>,
    _user: AuthenticatedUser,
    Json(payload): Json<Value>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let user_id = payload.get("user_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::BadRequest("Missing 'user_id' field".to_string()))?
        .to_string();

    let message = payload.get("message")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let notification_type = payload.get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("info")
        .to_string();

    crate::job_worker::enqueue_notification_job(worker, user_id, message, notification_type).await
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?;

    Ok((StatusCode::ACCEPTED, Json(json!({
        "message": "Notification job enqueued successfully",
        "timestamp": chrono::Utc::now().to_rfc3339()
    }))))
}

async fn enqueue_sync(
    State(worker): State<Arc<JobWorker>>,
    _user: AuthenticatedUser,
    Json(payload): Json<Value>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let sync_type = payload.get("sync_type")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::BadRequest("Missing 'sync_type' field".to_string()))?
        .to_string();

    let mut data = std::collections::HashMap::new();
    for (key, value) in payload.as_object().unwrap_or(&serde_json::Map::new()) {
        if key != "sync_type" {
            data.insert(key.clone(), value.clone());
        }
    }

    crate::job_worker::enqueue_sync_job(worker, sync_type, data).await
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?;

    Ok((StatusCode::ACCEPTED, Json(json!({
        "message": "Sync job enqueued successfully",
        "timestamp": chrono::Utc::now().to_rfc3339()
    }))))
}

async fn enqueue_blockchain_tx(
    State(worker): State<Arc<JobWorker>>,
    _user: AuthenticatedUser,
    Json(payload): Json<Value>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let from_address = payload.get("from_address")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::BadRequest("Missing 'from_address' field".to_string()))?
        .to_string();

    let to_address = payload.get("to_address")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::BadRequest("Missing 'to_address' field".to_string()))?
        .to_string();

    let amount = payload.get("amount")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::BadRequest("Missing 'amount' field".to_string()))?
        .to_string();

    let network = payload.get("network")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    crate::job_worker::enqueue_blockchain_tx_job(worker, from_address, to_address, amount, network).await
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?;

    Ok((StatusCode::ACCEPTED, Json(json!({
        "message": "Blockchain transaction job enqueued successfully",
        "timestamp": chrono::Utc::now().to_rfc3339()
    }))))
}
