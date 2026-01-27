use crate::{
    api_error::ApiError,
    config::Config,
    models::{Notification, NotificationType},
};
use deadpool_postgres::Pool;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone)]
pub struct NotificationService {
    pub db_pool: Arc<Pool>,
    pub config: Config,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateNotificationRequest {
    pub user_id: String,
    pub notification_type: NotificationType,
    pub title: String,
    pub message: String,
    pub metadata: Option<serde_json::Value>,
}

impl NotificationService {
    pub fn new(db_pool: Arc<Pool>, config: Config) -> Self {
        Self { db_pool, config }
    }

    pub async fn create_notification(
        &self,
        request: CreateNotificationRequest,
    ) -> Result<Notification, ApiError> {
        let client = self.db_pool.get().await?;

        let notification_id = Uuid::new_v4();

        let row = client
            .query_one(
                r#"
                INSERT INTO notifications (
                    id, user_id, type, title, message, metadata, read
                )
                VALUES ($1, $2, $3::notification_type, $4, $5, $6, $7)
                RETURNING id, user_id, type, title, message, metadata, read, created_at, updated_at
                "#,
                &[
                    &notification_id,
                    &request.user_id,
                    &request.notification_type.to_string_lose(),
                    &request.title,
                    &request.message,
                    &request.metadata,
                    &false,
                ],
            )
            .await?;

        let notification = Notification {
            id: row.get::<_, Uuid>(0).to_string(),
            user_id: row.get(1),
            notification_type: NotificationType::from_string(row.get(2)),
            title: row.get(3),
            message: row.get(4),
            metadata: row.get(5),
            read: row.get(6),
            created_at: row.get::<_, chrono::DateTime<chrono::Utc>>(7),
            updated_at: row.get::<_, chrono::DateTime<chrono::Utc>>(8),
        };

        // Mock email notification
        self.send_email_notification(&notification).await?;

        Ok(notification)
    }

    pub async fn get_user_notifications(
        &self,
        user_id: &str,
    ) -> Result<Vec<Notification>, ApiError> {
        let client = self.db_pool.get().await?;

        let rows = client
            .query(
                r#"
                SELECT id, user_id, type, title, message, metadata, read, created_at, updated_at
                FROM notifications
                WHERE user_id = $1
                ORDER BY created_at DESC
                "#,
                &[&user_id],
            )
            .await?;

        let notifications = rows
            .into_iter()
            .map(|row| Notification {
                id: row.get::<_, Uuid>(0).to_string(),
                user_id: row.get(1),
                notification_type: NotificationType::from_string(row.get(2)),
                title: row.get(3),
                message: row.get(4),
                metadata: row.get(5),
                read: row.get(6),
                created_at: row.get::<_, chrono::DateTime<chrono::Utc>>(7),
                updated_at: row.get::<_, chrono::DateTime<chrono::Utc>>(8),
            })
            .collect();

        Ok(notifications)
    }

    pub async fn mark_as_read(&self, notification_id: Uuid) -> Result<(), ApiError> {
        let client = self.db_pool.get().await?;

        let rows_affected = client
            .execute(
                "UPDATE notifications SET read = true, updated_at = NOW() WHERE id = $1",
                &[&notification_id],
            )
            .await?;

        if rows_affected == 0 {
            return Err(ApiError::NotFound("Notification not found".to_string()));
        }

        Ok(())
    }

    async fn send_email_notification(&self, notification: &Notification) -> Result<(), ApiError> {
        // MOCK EMAIL PROVIDER
        println!(
            "[MOCK EMAIL] Sending {} email to {}: {}",
            notification.notification_type.to_string_lose(),
            notification.user_id,
            notification.title
        );
        // In a real implementation, this would call an external API like SendGrid or AWS SES
        Ok(())
    }
}
