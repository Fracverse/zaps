use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use std::sync::Arc;
use uuid::Uuid;

use crate::{
    api_error::ApiError,
    service::{
        notification_service::{CreateNotificationRequest, UpdatePreferencesRequest},
        ServiceContainer,
    },
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateNotificationDto {
    pub user_id: String,
    pub notification_type: String,
    pub title: String,
    pub message: String,
    pub metadata: Option<serde_json::Value>,
    pub template_name: Option<String>,
    pub template_vars: Option<std::collections::HashMap<String, String>>,
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

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreferenceResponseDto {
    pub user_id: String,
    pub email_enabled: bool,
    pub sms_enabled: bool,
    pub push_enabled: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeliveryLogResponseDto {
    pub channel: String,
    pub status: String,
    pub error_message: Option<String>,
    pub delivered_at: Option<chrono::DateTime<chrono::Utc>>,
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
    let notification_type =
        crate::models::NotificationType::from_str(&request.notification_type).unwrap();

    let notification = services
        .notification
        .create_notification(CreateNotificationRequest {
            user_id: request.user_id,
            notification_type,
            title: request.title,
            message: request.message,
            metadata: request.metadata,
            template_name: request.template_name,
            template_vars: request.template_vars,
        })
        .await?;

    Ok(Json(NotificationResponseDto {
        id: notification.id,
        user_id: notification.user_id,
        notification_type: notification.notification_type.to_string(),
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
            notification_type: n.notification_type.to_string(),
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
    Path(id): Path<Uuid>,
) -> Result<(), ApiError> {
    services.notification.mark_as_read(id).await?;
    Ok(())
}

pub async fn get_preferences(
    State(services): State<Arc<ServiceContainer>>,
    Path(user_id): Path<String>,
) -> Result<Json<PreferenceResponseDto>, ApiError> {
    let prefs = services.notification.get_preferences(&user_id).await?;
    Ok(Json(PreferenceResponseDto {
        user_id: prefs.user_id,
        email_enabled: prefs.email_enabled,
        sms_enabled: prefs.sms_enabled,
        push_enabled: prefs.push_enabled,
    }))
}

pub async fn update_preferences(
    State(services): State<Arc<ServiceContainer>>,
    Path(user_id): Path<String>,
    Json(request): Json<UpdatePreferencesRequest>,
) -> Result<Json<PreferenceResponseDto>, ApiError> {
    let prefs = services.notification.update_preferences(&user_id, request).await?;
    Ok(Json(PreferenceResponseDto {
        user_id: prefs.user_id,
        email_enabled: prefs.email_enabled,
        sms_enabled: prefs.sms_enabled,
        push_enabled: prefs.push_enabled,
    }))
}

pub async fn get_delivery_logs(
    State(services): State<Arc<ServiceContainer>>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<DeliveryLogResponseDto>>, ApiError> {
    let logs = services.notification.get_delivery_logs(&id).await?;
    let response = logs.into_iter().map(|l| DeliveryLogResponseDto {
        channel: l.channel.to_string(),
        status: l.status.to_string(),
        error_message: l.error_message,
        delivered_at: l.delivered_at,
    }).collect();
    Ok(Json(response))
}
