use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::{
    api_error::ApiError, middleware::auth::AuthenticatedUser, role::Role, service::ServiceContainer,
};

/// Helper function to check if a user can access a resource (own resource or admin)
fn can_access_resource(user: &AuthenticatedUser, resource_user_id: &str) -> bool {
    user.user_id == resource_user_id || user.role == Role::Admin
}

/// Validate profile input fields
fn validate_profile_input(
    display_name: Option<&String>,
    avatar_url: Option<&String>,
    bio: Option<&String>,
) -> Result<(), ApiError> {
    // Validate display_name if provided
    if let Some(name) = display_name {
        if name.trim().is_empty() {
            return Err(ApiError::Validation("Display name cannot be empty".into()));
        }
        if name.len() > 100 {
            return Err(ApiError::Validation(
                "Display name must be 100 characters or less".into(),
            ));
        }
    }

    // Validate avatar_url if provided
    if let Some(url) = avatar_url {
        if url.len() > 2048 {
            return Err(ApiError::Validation(
                "Avatar URL must be 2048 characters or less".into(),
            ));
        }
        // Basic URL format validation
        if !url.starts_with("http://") && !url.starts_with("https://") && !url.starts_with("/") {
            return Err(ApiError::Validation(
                "Avatar URL must be a valid HTTP/HTTPS URL or relative path".into(),
            ));
        }
    }

    // Validate bio if provided
    if let Some(bio_text) = bio {
        if bio_text.len() > 500 {
            return Err(ApiError::Validation(
                "Bio must be 500 characters or less".into(),
            ));
        }
    }

    Ok(())
}

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
    // Validate input
    validate_profile_input(
        Some(&request.display_name),
        request.avatar_url.as_ref(),
        request.bio.as_ref(),
    )?;

    // Resolve username to internal UUID
    let user_model = services.identity.get_user_by_id(&user.user_id).await?;
    let user_uuid = Uuid::parse_str(&user_model.id)
        .map_err(|_| ApiError::Validation("Invalid user internal ID".into()))?;

    // Check if profile already exists
    if services.profile.get_profile(user_uuid).await?.is_some() {
        return Err(ApiError::Conflict(
            "Profile already exists for this user".into(),
        ));
    }

    let profile = services
        .profile
        .create_profile(
            user_uuid,
            request.display_name,
            request.avatar_url,
            request.bio,
            request.country,
            request.metadata,
        )
        .await?;

    Ok(Json(UserProfileResponseDto {
        id: profile.id,
        user_id: user.user_id, // Return the username string
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
    // Resolve username to internal UUID
    let user_model = services.identity.get_user_by_id(&user_id).await?;
    let user_uuid = Uuid::parse_str(&user_model.id)
        .map_err(|_| ApiError::Validation("Invalid user internal ID".into()))?;

    let profile = services
        .profile
        .get_profile(user_uuid)
        .await?
        .ok_or(ApiError::NotFound("Profile not found".into()))?;

    Ok(Json(UserProfileResponseDto {
        id: profile.id,
        user_id, // Return the username string
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
    // Authorization check: User can only update their own profile, unless Admin
    if !can_access_resource(&user, &user_id) {
        return Err(ApiError::Authorization(
            "You can only update your own profile".into(),
        ));
    }

    // Validate input
    validate_profile_input(
        request.display_name.as_ref(),
        request.avatar_url.as_ref(),
        request.bio.as_ref(),
    )?;

    // Resolve username to internal UUID
    let user_model = services.identity.get_user_by_id(&user_id).await?;
    let target_uuid = Uuid::parse_str(&user_model.id)
        .map_err(|_| ApiError::Validation("Invalid user internal ID".into()))?;

    let profile = services
        .profile
        .update_profile(
            target_uuid,
            request.display_name,
            request.avatar_url,
            request.bio,
            request.country,
            request.metadata,
        )
        .await?;

    Ok(Json(UserProfileResponseDto {
        id: profile.id,
        user_id,
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
    // Authorization check: User can only delete their own profile, unless Admin
    if !can_access_resource(&user, &user_id) {
        return Err(ApiError::Authorization(
            "You can only delete your own profile".into(),
        ));
    }

    // Resolve username to internal UUID
    let user_model = services.identity.get_user_by_id(&user_id).await?;
    let target_uuid = Uuid::parse_str(&user_model.id)
        .map_err(|_| ApiError::Validation("Invalid user internal ID".into()))?;

    services.profile.delete_profile(target_uuid).await?;

    Ok(StatusCode::NO_CONTENT)
}

/// Get the authenticated user's own profile
pub async fn get_my_profile(
    State(services): State<Arc<ServiceContainer>>,
    Extension(user): Extension<AuthenticatedUser>,
) -> Result<Json<UserProfileResponseDto>, ApiError> {
    // Resolve username to internal UUID
    let user_model = services.identity.get_user_by_id(&user.user_id).await?;
    let user_uuid = Uuid::parse_str(&user_model.id)
        .map_err(|_| ApiError::Validation("Invalid user internal ID".into()))?;

    let profile = services
        .profile
        .get_profile(user_uuid)
        .await?
        .ok_or(ApiError::NotFound("Profile not found".into()))?;

    Ok(Json(UserProfileResponseDto {
        id: profile.id,
        user_id: user.user_id, // Return the username string
        display_name: profile.display_name,
        avatar_url: profile.avatar_url,
        bio: profile.bio,
        country: profile.country,
        created_at: profile.created_at,
        updated_at: profile.updated_at,
    }))
}
