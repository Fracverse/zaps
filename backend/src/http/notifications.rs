use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::{
    api_error::ApiError,
    service::{notification_service::CreateNotificationRequest, ServiceContainer},
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateNotificationDto {
    pub user_id: String,
    pub notification_type: String,
    pub title: String,
    pub message: String,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationResponseDto {
    pub id: String,
    pub user_id: String,
    pub notification_type: String,
    pub title: String,
    pub message: String,
    pub read: bool,
    pub metadata: Option<serde_json::Value>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize)]
pub struct NotificationQuery {
    #[serde(rename = "userId")]
    pub user_id: String,
}

pub async fn create_notification(
    State(services): State<Arc<ServiceContainer>>,
    Json(request): Json<CreateNotificationDto>,
) -> Result<Json<NotificationResponseDto>, ApiError> {
    let notification_type = crate::models::NotificationType::from_str(&request.notification_type);

    let notification = services
        .notification
        .create_notification(CreateNotificationRequest {
            user_id: request.user_id,
            notification_type,
            title: request.title,
            message: request.message,
            metadata: request.metadata,
        })
        .await?;

    Ok(Json(NotificationResponseDto {
        id: notification.id,
        user_id: notification.user_id,
        notification_type: notification.notification_type.to_string_lose(),
        title: notification.title,
        message: notification.message,
        read: notification.read,
        metadata: notification.metadata,
        created_at: notification.created_at,
    }))
}

pub async fn get_notifications(
    State(services): State<Arc<ServiceContainer>>,
    Query(query): Query<NotificationQuery>,
) -> Result<Json<Vec<NotificationResponseDto>>, ApiError> {
    let notifications = services
        .notification
        .get_user_notifications(&query.user_id)
        .await?;

    let response = notifications
        .into_iter()
        .map(|n| NotificationResponseDto {
            id: n.id,
            user_id: n.user_id,
            notification_type: n.notification_type.to_string_lose(),
            title: n.title,
            message: n.message,
            read: n.read,
            metadata: n.metadata,
            created_at: n.created_at,
        })
        .collect();

    Ok(Json(response))
}

pub async fn mark_notification_read(
    State(services): State<Arc<ServiceContainer>>,
    Path(id): Path<String>,
) -> Result<(), ApiError> {
    let notification_uuid = Uuid::parse_str(&id)
        .map_err(|_| ApiError::Validation("Invalid Notification ID".to_string()))?;

    services
        .notification
        .mark_as_read(notification_uuid)
        .await?;

    Ok(())
}
