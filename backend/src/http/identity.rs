use axum::{
    extract::{Path, State},
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::{api_error::ApiError, auth, middleware::AuthenticatedUser, service::ServiceContainer};

#[derive(Debug, Deserialize)]
pub struct CreateUserRequest {
    pub user_id: String,
    pub pin: String,
}

#[derive(Debug, Serialize)]
pub struct UserResponse {
    pub id: uuid::Uuid,
    pub user_id: String,
    pub stellar_address: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize)]
pub struct WalletResponse {
    pub user_id: String,
    pub address: String,
}

pub async fn create_user(
    State(services): State<Arc<ServiceContainer>>,
    Json(request): Json<CreateUserRequest>,
) -> Result<Json<UserResponse>, ApiError> {
    let pin_hash = auth::hash_pin(&request.pin)?;
    let user = services
        .identity
        .create_user(request.user_id, pin_hash)
        .await?;

    Ok(Json(UserResponse {
        id: uuid::Uuid::parse_str(&user.id).unwrap_or_default(),
        user_id: user.user_id,
        stellar_address: user.stellar_address,
        created_at: user.created_at,
    }))
}

pub async fn get_user(
    State(services): State<Arc<ServiceContainer>>,
    user: AuthenticatedUser,
) -> Result<Json<UserResponse>, ApiError> {
    let user = services.identity.get_user_by_id(&user.user_id).await?;

    Ok(Json(UserResponse {
        id: uuid::Uuid::parse_str(&user.id).unwrap_or_default(),
        user_id: user.user_id,
        stellar_address: user.stellar_address,
        created_at: user.created_at,
    }))
}

pub async fn get_wallet(
    State(services): State<Arc<ServiceContainer>>,
    user: AuthenticatedUser,
) -> Result<Json<WalletResponse>, ApiError> {
    let wallet = services.identity.get_user_wallet(&user.user_id).await?;

    Ok(Json(WalletResponse {
        user_id: wallet.user_id,
        address: wallet.address,
    }))
}

pub async fn resolve_user_id(
    State(services): State<Arc<ServiceContainer>>,
    Path(user_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let address = services.identity.resolve_user_id(&user_id).await?;

    Ok(Json(serde_json::json!({
        "user_id": user_id,
        "stellar_address": address
    })))
}
