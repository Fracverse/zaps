use crate::{
    api_error::ApiError,
    config::Config,
    models::{
        DeliveryChannel, DeliveryStatus, Notification, NotificationDeliveryLog,
        NotificationPreference, NotificationTemplate, NotificationType,
    },
};
use chrono::Utc;
use deadpool_postgres::Pool;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone)]
pub struct NotificationService {
    db_pool: Arc<Pool>,
    #[allow(dead_code)]
    config: Config,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateNotificationRequest {
    pub user_id: String,
    pub notification_type: NotificationType,
    pub title: String,
    pub message: String,
    pub metadata: Option<serde_json::Value>,
    pub template_name: Option<String>,
    pub template_vars: Option<HashMap<String, String>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdatePreferencesRequest {
    pub email_enabled: Option<bool>,
    pub sms_enabled: Option<bool>,
    pub push_enabled: Option<bool>,
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

        let (title, message) = if let Some(template_name) = &request.template_name {
            let template = self.get_template(template_name).await?;
            let vars = request.template_vars.clone().unwrap_or_default();
            let body = self.parse_template(&template.body_template, &vars);
            let subject = template
                .subject_template
                .as_ref()
                .map(|s| self.parse_template(s, &vars))
                .unwrap_or(request.title.clone());
            (subject, body)
        } else {
            (request.title, request.message)
        };

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
                    &request.notification_type.to_string(),
                    &title,
                    &message,
                    &request.metadata,
                    &false,
                ],
            )
            .await?;

        let notification = Notification {
            id: row.get::<_, Uuid>(0).to_string(),
            user_id: row.get(1),
            notification_type: NotificationType::from_str(row.get(2)).unwrap(),
            title: row.get(3),
            message: row.get(4),
            metadata: row.get(5),
            read: row.get(6),
            created_at: row.get::<_, chrono::DateTime<Utc>>(7),
            updated_at: row.get::<_, chrono::DateTime<Utc>>(8),
        };

        // Check preferences and dispatch
        let prefs = self.get_preferences(&notification.user_id).await?;
        
        // Always record IN_APP delivery as successful
        self.log_delivery(&notification_id, DeliveryChannel::IN_APP, DeliveryStatus::DELIVERED, None).await?;

        if prefs.email_enabled {
            match self.send_email(&notification).await {
                Ok(_) => self.log_delivery(&notification_id, DeliveryChannel::EMAIL, DeliveryStatus::SENT, None).await?,
                Err(e) => self.log_delivery(&notification_id, DeliveryChannel::EMAIL, DeliveryStatus::FAILED, Some(&e.to_string())).await?,
            }
        }

        if prefs.sms_enabled {
            match self.send_sms(&notification).await {
                Ok(_) => self.log_delivery(&notification_id, DeliveryChannel::SMS, DeliveryStatus::SENT, None).await?,
                Err(e) => self.log_delivery(&notification_id, DeliveryChannel::SMS, DeliveryStatus::FAILED, Some(&e.to_string())).await?,
            }
        }

        if prefs.push_enabled {
            match self.send_push(&notification).await {
                Ok(_) => self.log_delivery(&notification_id, DeliveryChannel::PUSH, DeliveryStatus::SENT, None).await?,
                Err(e) => self.log_delivery(&notification_id, DeliveryChannel::PUSH, DeliveryStatus::FAILED, Some(&e.to_string())).await?,
            }
        }

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
                notification_type: NotificationType::from_str(row.get(2)).unwrap(),
                title: row.get(3),
                message: row.get(4),
                metadata: row.get(5),
                read: row.get(6),
                created_at: row.get::<_, chrono::DateTime<Utc>>(7),
                updated_at: row.get::<_, chrono::DateTime<Utc>>(8),
            })
            .collect();

        Ok(notifications)
    }

    pub async fn get_delivery_logs(&self, notification_id: &Uuid) -> Result<Vec<NotificationDeliveryLog>, ApiError> {
        let client = self.db_pool.get().await?;
        let rows = client.query(
            "SELECT id, notification_id, channel::text, status::text, error_message, delivered_at, created_at FROM notification_delivery_logs WHERE notification_id = $1",
            &[notification_id]
        ).await?;

        Ok(rows.into_iter().map(|row| NotificationDeliveryLog {
            id: row.get::<_, Uuid>(0).to_string(),
            notification_id: row.get::<_, Uuid>(1).to_string(),
            channel: DeliveryChannel::from_str(row.get(2)).unwrap(),
            status: DeliveryStatus::from_str(row.get(3)).unwrap(),
            error_message: row.get(4),
            delivered_at: row.get(5),
            created_at: row.get(6),
        }).collect())
    }

    pub async fn get_preferences(&self, user_id: &str) -> Result<NotificationPreference, ApiError> {
        let client = self.db_pool.get().await?;
        let row = client.query_opt(
            "SELECT id, user_id, email_enabled, sms_enabled, push_enabled, created_at, updated_at FROM notification_preferences WHERE user_id = $1",
            &[&user_id]
        ).await?;

        match row {
            Some(row) => Ok(NotificationPreference {
                id: row.get::<_, Uuid>(0).to_string(),
                user_id: row.get(1),
                email_enabled: row.get(2),
                sms_enabled: row.get(3),
                push_enabled: row.get(4),
                created_at: row.get(5),
                updated_at: row.get(6),
            }),
            None => {
                // Return defaults if no prefs found
                Ok(NotificationPreference {
                    id: Uuid::nil().to_string(),
                    user_id: user_id.to_string(),
                    email_enabled: true,
                    sms_enabled: false,
                    push_enabled: true,
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                })
            }
        }
    }

    pub async fn update_preferences(&self, user_id: &str, req: UpdatePreferencesRequest) -> Result<NotificationPreference, ApiError> {
        let client = self.db_pool.get().await?;
        
        client.execute(
            r#"
            INSERT INTO notification_preferences (user_id, email_enabled, sms_enabled, push_enabled)
            VALUES ($1, COALESCE($2, TRUE), COALESCE($3, FALSE), COALESCE($4, TRUE))
            ON CONFLICT (user_id) DO UPDATE SET
                email_enabled = COALESCE($2, notification_preferences.email_enabled),
                sms_enabled = COALESCE($3, notification_preferences.sms_enabled),
                push_enabled = COALESCE($4, notification_preferences.push_enabled),
                updated_at = NOW()
            "#,
            &[&user_id, &req.email_enabled, &req.sms_enabled, &req.push_enabled]
        ).await?;

        self.get_preferences(user_id).await
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

    async fn get_template(&self, name: &str) -> Result<NotificationTemplate, ApiError> {
        let client = self.db_pool.get().await?;
        let row = client.query_one(
            "SELECT id, name, subject_template, body_template, created_at, updated_at FROM notification_templates WHERE name = $1",
            &[&name]
        ).await.map_err(|_| ApiError::NotFound(format!("Template '{}' not found", name)))?;

        Ok(NotificationTemplate {
            id: row.get::<_, Uuid>(0).to_string(),
            name: row.get(1),
            subject_template: row.get(2),
            body_template: row.get(3),
            created_at: row.get(4),
            updated_at: row.get(5),
        })
    }

    fn parse_template(&self, template: &str, vars: &HashMap<String, String>) -> String {
        let mut result = template.to_string();
        for (key, value) in vars {
            let placeholder = format!("{{{{{}}}}}", key);
            result = result.replace(&placeholder, value);
        }
        result
    }

    async fn log_delivery(&self, notification_id: &Uuid, channel: DeliveryChannel, status: DeliveryStatus, error: Option<&str>) -> Result<(), ApiError> {
        let client = self.db_pool.get().await?;
        client.execute(
            "INSERT INTO notification_delivery_logs (notification_id, channel, status, error_message, delivered_at) VALUES ($1, $2::delivery_channel, $3::delivery_status, $4, $5)",
            &[notification_id, &channel.to_string(), &status.to_string(), &error, &if status == DeliveryStatus::DELIVERED { Some(Utc::now()) } else { None }]
        ).await?;
        Ok(())
    }

    async fn send_email(&self, notification: &Notification) -> Result<(), ApiError> {
        println!("[MOCK EMAIL] To {}: {} - {}", notification.user_id, notification.title, notification.message);
        Ok(())
    }

    async fn send_sms(&self, notification: &Notification) -> Result<(), ApiError> {
        println!("[MOCK SMS] To {}: {}", notification.user_id, notification.message);
        Ok(())
    }

    async fn send_push(&self, notification: &Notification) -> Result<(), ApiError> {
        println!("[MOCK PUSH] To {}: {}", notification.user_id, notification.title);
        Ok(())
    }
}
