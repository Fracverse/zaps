use crate::{api_error::ApiError, config::Config};
use chrono::{DateTime, Utc};
use deadpool_postgres::Pool;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::sync::Arc;
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;

/// Maximum delivery attempts before a delivery is marked `exhausted`.
const MAX_ATTEMPTS: i32 = 5;

/// Exponential backoff delays in seconds: 10s, 30s, 5m, 30m, 2h.
const BACKOFF_SECS: [i64; 5] = [10, 30, 300, 1800, 7200];

// ---------------------------------------------------------------------------
// Domain types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookEndpoint {
    pub id: String,
    pub merchant_id: String,
    pub url: String,
    pub events: Vec<String>,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookDelivery {
    pub id: String,
    pub endpoint_id: String,
    pub event_type: String,
    pub payload: serde_json::Value,
    pub status: String,
    pub attempt_count: i32,
    pub next_retry_at: Option<DateTime<Utc>>,
    pub response_status: Option<i32>,
    pub error_message: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct RegisterWebhookRequest {
    pub merchant_id: String,
    pub url: String,
    pub secret: String,
    pub events: Vec<String>,
}

// ---------------------------------------------------------------------------
// Service
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct WebhookService {
    db_pool: Arc<Pool>,
    #[allow(dead_code)]
    config: Config,
    http: reqwest::Client,
}

impl WebhookService {
    pub fn new(db_pool: Arc<Pool>, config: Config) -> Self {
        Self {
            db_pool,
            config,
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .expect("failed to build reqwest client"),
        }
    }

    // -----------------------------------------------------------------------
    // Registration
    // -----------------------------------------------------------------------

    pub async fn register_endpoint(
        &self,
        req: RegisterWebhookRequest,
    ) -> Result<WebhookEndpoint, ApiError> {
        // Validate URL scheme.
        if !req.url.starts_with("https://") && !req.url.starts_with("http://") {
            return Err(ApiError::Validation("Webhook URL must be http(s)".into()));
        }

        let client = self.db_pool.get().await?;
        let id = Uuid::new_v4();
        // Store a hashed secret so the plaintext is never persisted.
        let hashed_secret = Self::hash_secret(&req.secret);

        let row = client
            .query_one(
                r#"
                INSERT INTO webhook_endpoints (id, merchant_id, url, secret, events)
                VALUES ($1, $2, $3, $4, $5)
                RETURNING id, merchant_id, url, events, is_active, created_at
                "#,
                &[
                    &id,
                    &req.merchant_id,
                    &req.url,
                    &hashed_secret,
                    &req.events,
                ],
            )
            .await?;

        Ok(Self::row_to_endpoint(&row))
    }

    pub async fn list_endpoints(
        &self,
        merchant_id: &str,
    ) -> Result<Vec<WebhookEndpoint>, ApiError> {
        let client = self.db_pool.get().await?;
        let rows = client
            .query(
                r#"
                SELECT id, merchant_id, url, events, is_active, created_at
                FROM webhook_endpoints
                WHERE merchant_id = $1
                ORDER BY created_at DESC
                "#,
                &[&merchant_id],
            )
            .await?;

        Ok(rows.iter().map(Self::row_to_endpoint).collect())
    }

    pub async fn delete_endpoint(&self, id: Uuid, merchant_id: &str) -> Result<(), ApiError> {
        let client = self.db_pool.get().await?;
        let n = client
            .execute(
                "DELETE FROM webhook_endpoints WHERE id = $1 AND merchant_id = $2",
                &[&id, &merchant_id],
            )
            .await?;
        if n == 0 {
            return Err(ApiError::NotFound("Webhook endpoint not found".into()));
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Dispatch
    // -----------------------------------------------------------------------

    /// Enqueue a delivery for every active endpoint subscribed to `event_type`.
    pub async fn dispatch_event(
        &self,
        event_type: &str,
        payload: serde_json::Value,
    ) -> Result<(), ApiError> {
        let client = self.db_pool.get().await?;

        let endpoints = client
            .query(
                r#"
                SELECT id FROM webhook_endpoints
                WHERE is_active = true AND $1 = ANY(events)
                "#,
                &[&event_type],
            )
            .await?;

        for row in endpoints {
            let endpoint_id: Uuid = row.get(0);
            let delivery_id = Uuid::new_v4();
            client
                .execute(
                    r#"
                    INSERT INTO webhook_deliveries
                        (id, endpoint_id, event_type, payload, status, next_retry_at)
                    VALUES ($1, $2, $3, $4, 'pending', NOW())
                    "#,
                    &[&delivery_id, &endpoint_id, &event_type, &payload],
                )
                .await?;
        }

        Ok(())
    }

    /// Attempt delivery of a single queued delivery record.
    ///
    /// Called by the background job worker.  Updates status and schedules
    /// the next retry with exponential backoff on failure.
    pub async fn attempt_delivery(&self, delivery_id: Uuid) -> Result<(), ApiError> {
        let client = self.db_pool.get().await?;

        // Load delivery + endpoint in one query.
        let row = client
            .query_opt(
                r#"
                SELECT d.id, d.endpoint_id, d.event_type, d.payload,
                       d.attempt_count, e.url, e.secret
                FROM webhook_deliveries d
                JOIN webhook_endpoints e ON e.id = d.endpoint_id
                WHERE d.id = $1 AND d.status IN ('pending', 'delivering')
                "#,
                &[&delivery_id],
            )
            .await?;

        let row = match row {
            Some(r) => r,
            None => return Ok(()), // already delivered or not found
        };

        let attempt_count: i32 = row.get(4);
        let url: &str = row.get(5);
        let secret: &str = row.get(6);
        let payload: serde_json::Value = row.get(3);
        let event_type: &str = row.get(2);

        // Mark as delivering.
        client
            .execute(
                "UPDATE webhook_deliveries SET status = 'delivering', attempt_count = attempt_count + 1, updated_at = NOW() WHERE id = $1",
                &[&delivery_id],
            )
            .await?;

        let body = serde_json::to_string(&payload).unwrap_or_default();
        let signature = Self::sign_payload(&body, secret);

        let result = self
            .http
            .post(url)
            .header("Content-Type", "application/json")
            .header("X-Webhook-Signature", &signature)
            .header("X-Webhook-Event", event_type)
            .body(body)
            .send()
            .await;

        let new_attempt = attempt_count + 1;

        match result {
            Ok(resp) if resp.status().is_success() => {
                let status_code = resp.status().as_u16() as i32;
                client
                    .execute(
                        r#"
                        UPDATE webhook_deliveries
                        SET status = 'delivered', response_status = $2, updated_at = NOW()
                        WHERE id = $1
                        "#,
                        &[&delivery_id, &status_code],
                    )
                    .await?;
            }
            Ok(resp) => {
                let status_code = resp.status().as_u16() as i32;
                let err_msg = format!("HTTP {}", status_code);
                self.schedule_retry(&client, delivery_id, new_attempt, &err_msg, Some(status_code))
                    .await?;
            }
            Err(e) => {
                self.schedule_retry(&client, delivery_id, new_attempt, &e.to_string(), None)
                    .await?;
            }
        }

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Delivery status
    // -----------------------------------------------------------------------

    pub async fn get_deliveries(
        &self,
        endpoint_id: Uuid,
    ) -> Result<Vec<WebhookDelivery>, ApiError> {
        let client = self.db_pool.get().await?;
        let rows = client
            .query(
                r#"
                SELECT id, endpoint_id, event_type, payload, status,
                       attempt_count, next_retry_at, response_status, error_message, created_at
                FROM webhook_deliveries
                WHERE endpoint_id = $1
                ORDER BY created_at DESC
                LIMIT 100
                "#,
                &[&endpoint_id],
            )
            .await?;

        Ok(rows.iter().map(Self::row_to_delivery).collect())
    }

    // -----------------------------------------------------------------------
    // Signature helpers
    // -----------------------------------------------------------------------

    /// Compute `HMAC-SHA256(secret, body)` and return as hex.
    pub fn sign_payload(body: &str, secret: &str) -> String {
        let mut mac =
            HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC accepts any key length");
        mac.update(body.as_bytes());
        hex::encode(mac.finalize().into_bytes())
    }

    /// Verify an incoming webhook signature.
    pub fn verify_signature(body: &str, secret: &str, provided_sig: &str) -> bool {
        let expected = Self::sign_payload(body, secret);
        // Constant-time comparison via HMAC verify.
        let mut mac =
            HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC accepts any key length");
        mac.update(body.as_bytes());
        let provided_bytes = hex::decode(provided_sig).unwrap_or_default();
        mac.verify_slice(&provided_bytes).is_ok() && expected == provided_sig
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    async fn schedule_retry(
        &self,
        client: &deadpool_postgres::Object,
        delivery_id: Uuid,
        attempt: i32,
        error: &str,
        response_status: Option<i32>,
    ) -> Result<(), ApiError> {
        if attempt >= MAX_ATTEMPTS {
            client
                .execute(
                    r#"
                    UPDATE webhook_deliveries
                    SET status = 'exhausted', error_message = $2,
                        response_status = $3, updated_at = NOW()
                    WHERE id = $1
                    "#,
                    &[&delivery_id, &error, &response_status],
                )
                .await?;
        } else {
            let delay = BACKOFF_SECS[(attempt as usize).min(BACKOFF_SECS.len() - 1)];
            let next_retry = Utc::now() + chrono::Duration::seconds(delay);
            client
                .execute(
                    r#"
                    UPDATE webhook_deliveries
                    SET status = 'pending', error_message = $2,
                        response_status = $3, next_retry_at = $4, updated_at = NOW()
                    WHERE id = $1
                    "#,
                    &[&delivery_id, &error, &response_status, &next_retry],
                )
                .await?;
        }
        Ok(())
    }

    fn hash_secret(secret: &str) -> String {
        use sha2::Digest;
        let mut hasher = sha2::Sha256::new();
        hasher.update(secret.as_bytes());
        hex::encode(hasher.finalize())
    }

    fn row_to_endpoint(row: &tokio_postgres::Row) -> WebhookEndpoint {
        WebhookEndpoint {
            id: row.get::<_, Uuid>(0).to_string(),
            merchant_id: row.get(1),
            url: row.get(2),
            events: row.get(3),
            is_active: row.get(4),
            created_at: row.get(5),
        }
    }

    fn row_to_delivery(row: &tokio_postgres::Row) -> WebhookDelivery {
        WebhookDelivery {
            id: row.get::<_, Uuid>(0).to_string(),
            endpoint_id: row.get::<_, Uuid>(1).to_string(),
            event_type: row.get(2),
            payload: row.get(3),
            status: row.get(4),
            attempt_count: row.get(5),
            next_retry_at: row.get(6),
            response_status: row.get(7),
            error_message: row.get(8),
            created_at: row.get(9),
        }
    }
}
