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
    models::BuildTransactionDto,
    service::ServiceContainer,
};

#[derive(Debug, Serialize)]
pub struct TransferResponse {
    pub id: Uuid,
    pub from_user_id: String,
    pub to_user_id: String,
    pub amount: i64,
    pub asset: String,
    pub status: String,
    pub memo: Option<String>,
    /// Unsigned transaction XDR for the user-to-user transfer
    pub unsigned_xdr: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateTransferRequest {
    pub to_user_id: String,
    pub amount: i64,
    pub asset: String,
    pub memo: Option<String>,
}

fn is_valid_stellar_address(address: &str) -> bool {
    // Lightweight validation suitable for current mock addresses
    !address.is_empty() && address.starts_with('G')
}

pub async fn create_transfer(
    State(services): State<Arc<ServiceContainer>>,
    auth_user: AuthenticatedUser,
    Json(request): Json<CreateTransferRequest>,
) -> Result<Json<TransferResponse>, ApiError> {
    if request.amount <= 0 {
        return Err(ApiError::Validation(
            "Amount must be greater than zero".to_string(),
        ));
    }

    // Resolve sender and recipient wallets and validate recipient
    let from_wallet = services
        .identity
        .get_user_wallet(&auth_user.user_id)
        .await?;

    let to_user = services
        .identity
        .get_user_by_id(&request.to_user_id)
        .await?;

    if !is_valid_stellar_address(&to_user.stellar_address) {
        return Err(ApiError::Validation(
            "Recipient has an invalid Stellar address".to_string(),
        ));
    }

    // Build an unsigned transaction XDR for the direct transfer
    let dto = BuildTransactionDto {
        contract_id: "user_to_user_transfer".to_string(),
        method: "transfer".to_string(),
        args: vec![
            serde_json::json!({ "from_user_id": auth_user.user_id }),
            serde_json::json!({ "from_address": from_wallet.address }),
            serde_json::json!({ "to_user_id": request.to_user_id }),
            serde_json::json!({ "to_address": to_user.stellar_address }),
            serde_json::json!({ "asset": request.asset }),
            serde_json::json!({ "amount": request.amount }),
            serde_json::json!({ "memo": request.memo }),
        ],
    };

    let unsigned_xdr = services.soroban.build_transaction(dto).await?;

    let transfer_id = Uuid::new_v4();

    Ok(Json(TransferResponse {
        id: transfer_id,
        from_user_id: auth_user.user_id,
        to_user_id: request.to_user_id,
        amount: request.amount,
        asset: request.asset,
        status: "pending".to_string(),
        memo: request.memo,
        unsigned_xdr,
    }))
}

pub async fn get_transfer(
    State(_services): State<Arc<ServiceContainer>>,
    Path(_transfer_id): Path<Uuid>,
) -> Result<Json<TransferResponse>, ApiError> {
    // Placeholder implementation
    Err(ApiError::NotFound("Not implemented".to_string()))
}

pub async fn get_transfer_status(
    State(_services): State<Arc<ServiceContainer>>,
    Path(_transfer_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // Placeholder implementation
    Err(ApiError::NotFound("Not implemented".to_string()))
}
