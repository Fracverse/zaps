use axum::{
    extract::{Path, State},
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::{api_error::ApiError, service::ServiceContainer};

#[derive(Debug, Serialize)]
pub struct WithdrawalResponse {
    pub id: Uuid,
    pub user_id: String,
    pub destination_address: String,
    pub amount: i64,
    pub asset: String,
    pub status: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateWithdrawalRequest {
    pub destination_address: String,
    pub amount: i64,
    pub asset: String,
}

pub async fn create_withdrawal(
    State(_services): State<Arc<ServiceContainer>>,
    Json(_request): Json<CreateWithdrawalRequest>,
) -> Result<Json<WithdrawalResponse>, ApiError> {
    // Placeholder implementation
    Err(ApiError::NotFound("Not implemented".to_string()))
}

pub async fn get_withdrawal(
    State(_services): State<Arc<ServiceContainer>>,
    Path(_withdrawal_id): Path<Uuid>,
) -> Result<Json<WithdrawalResponse>, ApiError> {
    // Placeholder implementation
    Err(ApiError::NotFound("Not implemented".to_string()))
}

pub async fn get_withdrawal_status(
    State(_services): State<Arc<ServiceContainer>>,
    Path(_withdrawal_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // Placeholder implementation
    Err(ApiError::NotFound("Not implemented".to_string()))
}
