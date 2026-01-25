use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::{
    api_error::ApiError,
    middleware::auth::AuthenticatedUser,
    role::Role,
    service::ServiceContainer,
};

#[derive(Debug, Deserialize)]
pub struct CreateUserProfileDto {
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub bio: Option<String>,
    pub country: Option<String>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateUserProfileDto {
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub bio: Option<String>,
    pub country: Option<String>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
pub struct UserProfileResponseDto {
    pub id: String,
    pub user_id: String,
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub bio: Option<String>,
    pub country: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

pub async fn create_profile(
    State(services): State<Arc<ServiceContainer>>,
    Extension(user): Extension<AuthenticatedUser>,
    Json(request): Json<CreateUserProfileDto>,
) -> Result<Json<UserProfileResponseDto>, ApiError> {
    let user_uuid = Uuid::parse_str(&user.user_id).map_err(|_| ApiError::BadRequest("Invalid user ID format".into()))?;

    // Check if profile already exists
    if services.profile.get_profile(user_uuid).await?.is_some() {
        return Err(ApiError::Conflict("Profile already exists for this user".into()));
    }

    let profile = services.profile.create_profile(
        user_uuid,
        request.display_name,
        request.avatar_url,
        request.bio,
        request.country,
        request.metadata,
    ).await?;

    Ok(Json(UserProfileResponseDto {
        id: profile.id,
        user_id: profile.user_id,
        display_name: profile.display_name,
        avatar_url: profile.avatar_url,
        bio: profile.bio,
        country: profile.country,
        created_at: profile.created_at,
        updated_at: profile.updated_at,
    }))
}

pub async fn get_profile(
    State(services): State<Arc<ServiceContainer>>,
    Path(user_id): Path<String>,
) -> Result<Json<UserProfileResponseDto>, ApiError> {
    let user_uuid = Uuid::parse_str(&user_id).map_err(|_| ApiError::BadRequest("Invalid user ID format".into()))?;
    
    let profile = services.profile.get_profile(user_uuid).await?
        .ok_or(ApiError::NotFound("Profile not found".into()))?;

    Ok(Json(UserProfileResponseDto {
        id: profile.id,
        user_id: profile.user_id,
        display_name: profile.display_name,
        avatar_url: profile.avatar_url,
        bio: profile.bio,
        country: profile.country,
        created_at: profile.created_at,
        updated_at: profile.updated_at,
    }))
}

pub async fn update_profile(
    State(services): State<Arc<ServiceContainer>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(user_id): Path<String>,
    Json(request): Json<UpdateUserProfileDto>,
) -> Result<Json<UserProfileResponseDto>, ApiError> {
    let target_uuid = Uuid::parse_str(&user_id).map_err(|_| ApiError::BadRequest("Invalid user ID format".into()))?;
    
    // Authorization check: User can only update their own profile, unless Admin
    // Note: Comparing strings for simplicity, assuming user.user_id is valid UUID string
    if user.user_id != user_id && user.role != Role::Admin {
        return Err(ApiError::Forbidden("You can only update your own profile".into()));
    }

    let profile = services.profile.update_profile(
        target_uuid,
        request.display_name,
        request.avatar_url,
        request.bio,
        request.country,
        request.metadata,
    ).await?;

    Ok(Json(UserProfileResponseDto {
        id: profile.id,
        user_id: profile.user_id,
        display_name: profile.display_name,
        avatar_url: profile.avatar_url,
        bio: profile.bio,
        country: profile.country,
        created_at: profile.created_at,
        updated_at: profile.updated_at,
    }))
}

pub async fn delete_profile(
    State(services): State<Arc<ServiceContainer>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(user_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let target_uuid = Uuid::parse_str(&user_id).map_err(|_| ApiError::BadRequest("Invalid user ID format".into()))?;

    // Authorization check: User can only delete their own profile, unless Admin
    if user.user_id != user_id && user.role != Role::Admin {
        return Err(ApiError::Forbidden("You can only delete your own profile".into()));
    }

    services.profile.delete_profile(target_uuid).await?;

    Ok(StatusCode::NO_CONTENT)
}
