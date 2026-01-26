use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::{api_error::ApiError, auth, service::ServiceContainer};

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub user_id: String,
    pub pin: String, // In production, this would be hashed
}

#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub user_id: String,
    pub pin: String,
    #[serde(default)]
    pub role: Option<String>, // Optional role for registration (admin-only in production)
}

#[derive(Debug, Serialize)]
pub struct AuthResponse {
    pub token: String,
    pub user_id: String,
    pub role: String,
    pub expires_in: i64,
}

#[derive(Debug, Deserialize)]
pub struct RefreshTokenRequest {
    pub token: String,
}

pub async fn login(
    State(services): State<Arc<ServiceContainer>>,
    Json(request): Json<LoginRequest>,
) -> Result<Json<AuthResponse>, ApiError> {
    // In production, validate PIN against stored hash
    // Get user to retrieve their role
    let user = services
        .identity
        .get_user_by_id(&request.user_id)
        .await
        .map_err(|_| ApiError::Authentication("Invalid credentials".to_string()))?;

    // Generate JWT token with user's role
    let token = auth::generate_jwt(
        &request.user_id,
        user.role,
        &services.config.jwt.secret,
        services.config.jwt.expiration_hours,
    )?;

    Ok(Json(AuthResponse {
        token,
        user_id: request.user_id,
        role: user.role.to_string(),
        expires_in: services.config.jwt.expiration_hours * 3600,
    }))
}

pub async fn register(
    State(services): State<Arc<ServiceContainer>>,
    Json(request): Json<RegisterRequest>,
) -> Result<Json<AuthResponse>, ApiError> {
    // Check if user already exists
    if services.identity.user_exists(&request.user_id).await? {
        return Err(ApiError::Conflict("User already exists".to_string()));
    }

    // Create user with default role (User)
    // In production, role assignment should be restricted
    let user = services
        .identity
        .create_user(request.user_id.clone())
        .await?;

    // Generate JWT token
    let token = auth::generate_jwt(
        &request.user_id,
        user.role,
        &services.config.jwt.secret,
        services.config.jwt.expiration_hours,
    )?;

    Ok(Json(AuthResponse {
        token,
        user_id: request.user_id,
        role: user.role.to_string(),
        expires_in: services.config.jwt.expiration_hours * 3600,
    }))
}

pub async fn refresh_token(
    State(services): State<Arc<ServiceContainer>>,
    Json(request): Json<RefreshTokenRequest>,
) -> Result<Json<AuthResponse>, ApiError> {
    // Validate the current token
    let claims = auth::validate_jwt(&request.token, &services.config.jwt.secret)?;

    // Generate new token with same role
    let token = auth::generate_jwt(
        &claims.sub,
        claims.role,
        &services.config.jwt.secret,
        services.config.jwt.expiration_hours,
    )?;

    Ok(Json(AuthResponse {
        token,
        user_id: claims.sub,
        role: claims.role.to_string(),
        expires_in: services.config.jwt.expiration_hours * 3600,
    }))
}
