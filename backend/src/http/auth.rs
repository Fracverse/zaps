use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::{api_error::ApiError, auth, service::ServiceContainer};

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub user_id: String,
    pub pin: String,
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
    pub refresh_token: String,
    pub user_id: String,
    pub role: String,
    pub expires_in: i64,
    pub refresh_expires_in: i64,
}

#[derive(Debug, Deserialize)]
pub struct RefreshTokenRequest {
    pub token: String,
}

pub async fn login(
    State(services): State<Arc<ServiceContainer>>,
    Json(request): Json<LoginRequest>,
) -> Result<Json<AuthResponse>, ApiError> {
    // Get user with pin hash
    let (user, pin_hash) = services
        .identity
        .get_user_with_pin_hash(&request.user_id)
        .await?;

    // Verify PIN
    if !auth::verify_pin(&request.pin, &pin_hash)? {
        return Err(ApiError::Authentication("Invalid credentials".to_string()));
    }

    // Generate token pair
    let token = auth::generate_access_token(
        &user.user_id,
        user.role,
        &services.config.jwt.secret,
        services.config.jwt.expiration_hours,
    )?;

    let refresh_token = auth::generate_refresh_token(
        &user.user_id,
        user.role,
        &services.config.jwt.secret,
        services.config.jwt.refresh_expiration_hours,
    )?;

    Ok(Json(AuthResponse {
        token,
        refresh_token,
        user_id: user.user_id,
        role: user.role.to_string(),
        expires_in: services.config.jwt.expiration_hours * 3600,
        refresh_expires_in: services.config.jwt.refresh_expiration_hours * 3600,
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

    // Validate and hash the PIN (enforces 4–6 digit format, bcrypt 10 rounds)
    let pin_hash = auth::hash_pin(&request.pin)?;

    // Create user with default role (User)
    // In production, role assignment should be restricted
    let user = services
        .identity
        .create_user(request.user_id.clone(), pin_hash)
        .await?;

    // Generate token pair
    let token = auth::generate_access_token(
        &user.user_id,
        user.role,
        &services.config.jwt.secret,
        services.config.jwt.expiration_hours,
    )?;

    let refresh_token = auth::generate_refresh_token(
        &user.user_id,
        user.role,
        &services.config.jwt.secret,
        services.config.jwt.refresh_expiration_hours,
    )?;

    Ok(Json(AuthResponse {
        token,
        refresh_token,
        user_id: user.user_id,
        role: user.role.to_string(),
        expires_in: services.config.jwt.expiration_hours * 3600,
        refresh_expires_in: services.config.jwt.refresh_expiration_hours * 3600,
    }))
}

pub async fn refresh_token(
    State(services): State<Arc<ServiceContainer>>,
    Json(request): Json<RefreshTokenRequest>,
) -> Result<Json<AuthResponse>, ApiError> {
    let claims = auth::validate_refresh_token(&request.token, &services.config.jwt.secret)?;

    if !services.identity.user_exists(&claims.sub).await? {
        return Err(ApiError::Authentication("User not found".to_string()));
    }

    let token = auth::generate_access_token(
        &claims.sub,
        claims.role,
        &services.config.jwt.secret,
        services.config.jwt.expiration_hours,
    )?;

    let refresh_token = auth::generate_refresh_token(
        &claims.sub,
        claims.role,
        &services.config.jwt.secret,
        services.config.jwt.refresh_expiration_hours,
    )?;

    Ok(Json(AuthResponse {
        token,
        refresh_token,
        user_id: claims.sub,
        role: claims.role.to_string(),
        expires_in: services.config.jwt.expiration_hours * 3600,
        refresh_expires_in: services.config.jwt.refresh_expiration_hours * 3600,
    }))
}

pub async fn user_register(
    State(services): State<Arc<ServiceContainer>>,
    Json(request): Json<RegisterRequest>,
) -> Result<Json<AuthResponse>, ApiError> {
    if services.identity.user_exists(&request.user_id).await? {
        return Err(ApiError::Conflict("User already exists".to_string()));
    }

    let pin_hash = auth::hash_pin(&request.pin)?;

    let user = services
        .identity
        .create_user(request.user_id.clone(), pin_hash)
        .await?;

    let token = auth::generate_access_token(
        &user.user_id,
        user.role,
        &services.config.jwt.secret,
        services.config.jwt.expiration_hours,
    )?;

    let refresh_token = auth::generate_refresh_token(
        &user.user_id,
        user.role,
        &services.config.jwt.secret,
        services.config.jwt.refresh_expiration_hours,
    )?;

    Ok(Json(AuthResponse {
        token,
        refresh_token,
        user_id: user.user_id,
        role: user.role.to_string(),
        expires_in: services.config.jwt.expiration_hours * 3600,
        refresh_expires_in: services.config.jwt.refresh_expiration_hours * 3600,
    }))
}
