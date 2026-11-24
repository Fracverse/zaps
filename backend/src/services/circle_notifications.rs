//! #1217 — Circle notification service.
//!
//! Producers use [`notify`] with one of the standardized [`CircleNotificationType`]
//! variants. A caller-supplied `dedupe_key` makes repeated events (for example a
//! retried webhook) idempotent. Every notification gets a per-channel delivery
//! row; failures are logged and retried with exponential backoff by
//! [`retry_due_deliveries`]. In-app notifications are always persisted, so a
//! push/webhook outage never loses the user-visible record.

use chrono::{DateTime, Duration, Utc};
use serde_json::Value;
use sqlx::{PgPool, Row};
use uuid::Uuid;

/// Standardized notification types for all Zaps Circle events.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CircleNotificationType {
    Invitation,
    Contribution,
    MembershipChange,
    Comment,
    Reaction,
    PayoutRequest,
    PayoutApproval,
    GoalCompleted,
    Announcement,
}

impl CircleNotificationType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Invitation => "invitation",
            Self::Contribution => "contribution",
            Self::MembershipChange => "membership_change",
            Self::Comment => "comment",
            Self::Reaction => "reaction",
            Self::PayoutRequest => "payout_request",
            Self::PayoutApproval => "payout_approval",
            Self::GoalCompleted => "goal_completed",
            Self::Announcement => "announcement",
        }
    }

    pub fn all() -> &'static [&'static str] {
        &[
            "invitation",
            "contribution",
            "membership_change",
            "comment",
            "reaction",
            "payout_request",
            "payout_approval",
            "goal_completed",
            "announcement",
        ]
    }
}

#[derive(Clone, Debug)]
pub struct NewCircleNotification<'a> {
    pub circle_id: Option<Uuid>,
    pub recipient_address: &'a str,
    pub kind: CircleNotificationType,
    pub title: &'a str,
    pub body: &'a str,
    pub metadata: Value,
    /// Stable idempotency key. Repeated events with the same key create only
    /// one notification (and therefore one set of delivery attempts).
    pub dedupe_key: Option<&'a str>,
    /// Push channel only; pass `None` for in-app-only events.
    pub push_token: Option<&'a str>,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct CircleNotificationEntry {
    pub id: Uuid,
    pub circle_id: Option<Uuid>,
    pub notification_type: String,
    pub title: String,
    pub body: String,
    pub metadata: Value,
    pub is_read: bool,
    pub created_at: DateTime<Utc>,
}

const MAX_DELIVERY_ATTEMPTS: i32 = 5;

/// Create a notification. Returns `None` when an identical event was already
/// recorded (duplicate suppressed).
pub async fn notify(
    pool: &PgPool,
    input: &NewCircleNotification<'_>,
) -> Result<Option<Uuid>, sqlx::Error> {
    let id = Uuid::new_v4();
    let mut tx = pool.begin().await?;

    let inserted: Option<(Uuid,)> = sqlx::query_as(
        r#"
        INSERT INTO circle_notifications
            (id, circle_id, recipient_address, notification_type, title, body,
             metadata, dedupe_key)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        ON CONFLICT (dedupe_key) WHERE dedupe_key IS NOT NULL DO NOTHING
        RETURNING id
        "#,
    )
    .bind(id)
    .bind(input.circle_id)
    .bind(input.recipient_address)
    .bind(input.kind.as_str())
    .bind(input.title)
    .bind(input.body)
    .bind(&input.metadata)
    .bind(input.dedupe_key)
    .fetch_optional(&mut *tx)
    .await?;

    let Some((notification_id,)) = inserted else {
        tx.rollback().await?;
        tracing::debug!(
            dedupe_key = ?input.dedupe_key,
            "Duplicate circle notification suppressed"
        );
        return Ok(None);
    };

    // The in-app record is the canonical delivery and needs no remote call.
    sqlx::query(
        r#"
        INSERT INTO circle_notification_deliveries
            (notification_id, channel, status, attempts, next_retry_at)
        VALUES ($1, 'in_app', 'delivered', 1, NOW())
        ON CONFLICT (notification_id, channel) DO NOTHING
        "#,
    )
    .bind(notification_id)
    .execute(&mut *tx)
    .await?;

    // #1218: resolve a registered device token for the recipient and respect
    // their circle push preference. An explicit token from the producer wins.
    let push_token = match input.push_token {
        Some(token) if !token.trim().is_empty() => Some(token.trim().to_string()),
        _ => sqlx::query_scalar::<_, String>(
            r#"
            SELECT t.expo_push_token
              FROM user_push_tokens t
              JOIN users u ON u.id = t.user_id
             WHERE u.address = $1 AND u.circle_push_enabled
               AND t.expo_push_token <> ''
             ORDER BY t.updated_at DESC
             LIMIT 1
            "#,
        )
        .bind(input.recipient_address)
        .fetch_optional(&mut *tx)
        .await?,
    };

    if push_token.is_some() {
        sqlx::query(
            r#"
            INSERT INTO circle_notification_deliveries
                (notification_id, channel, status, attempts, next_retry_at)
            VALUES ($1, 'push', 'pending', 0, NOW())
            ON CONFLICT (notification_id, channel) DO NOTHING
            "#,
        )
        .bind(notification_id)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(Some(notification_id))
}

/// List notifications for a recipient, optionally filtering unread only.
pub async fn list(
    pool: &PgPool,
    recipient: &str,
    unread_only: bool,
    limit: i64,
    offset: i64,
) -> Result<Vec<CircleNotificationEntry>, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT id, circle_id, notification_type, title, body, metadata,
               is_read, created_at
        FROM circle_notifications
        WHERE recipient_address = $1 AND (NOT $2 OR is_read = FALSE)
        ORDER BY created_at DESC
        LIMIT $3 OFFSET $4
        "#,
    )
    .bind(recipient)
    .bind(unread_only)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| CircleNotificationEntry {
            id: r.get("id"),
            circle_id: r.get("circle_id"),
            notification_type: r.get("notification_type"),
            title: r.get("title"),
            body: r.get("body"),
            metadata: r.get("metadata"),
            is_read: r.get("is_read"),
            created_at: r.get("created_at"),
        })
        .collect())
}

pub async fn unread_count(pool: &PgPool, recipient: &str) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM circle_notifications WHERE recipient_address = $1 AND is_read = FALSE",
    )
    .bind(recipient)
    .fetch_one(pool)
    .await
}

/// Mark one notification as read. Only the recipient may do so.
pub async fn mark_read(
    pool: &PgPool,
    recipient: &str,
    notification_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        r#"
        UPDATE circle_notifications
           SET is_read = TRUE, updated_at = NOW()
         WHERE id = $1 AND recipient_address = $2
        "#,
    )
    .bind(notification_id)
    .bind(recipient)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn mark_all_read(pool: &PgPool, recipient: &str) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        r#"
        UPDATE circle_notifications
           SET is_read = TRUE, updated_at = NOW()
         WHERE recipient_address = $1 AND is_read = FALSE
        "#,
    )
    .bind(recipient)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

/// Retry due push deliveries. Transport failures are persisted with the error
/// and rescheduled using exponential backoff; after MAX_DELIVERY_ATTEMPTS the
/// delivery is left in `failed` state for operators to inspect.
pub async fn retry_due_deliveries(pool: &PgPool) {
    let rows = match sqlx::query(
        r#"
        SELECT d.id, d.notification_id, d.attempts, n.title, n.body,
               n.notification_type, n.metadata, u.id AS user_id,
               u.push_financial_details
        FROM circle_notification_deliveries d
        JOIN circle_notifications n ON n.id = d.notification_id
        JOIN users u ON u.address = n.recipient_address
        WHERE d.channel = 'push' AND d.status = 'pending' AND d.next_retry_at <= NOW()
        LIMIT 100
        FOR UPDATE SKIP LOCKED
        "#,
    )
    .fetch_all(pool)
    .await
    {
        Ok(rows) => rows,
        Err(e) => {
            tracing::error!(error = %e, "Failed to load due circle notification deliveries");
            return;
        }
    };

    for row in rows {
        let id: Uuid = row.get("id");
        let notification_id: Uuid = row.get("notification_id");
        let attempts: i32 = row.get("attempts");
        let title: String = row.get("title");
        let body: String = row.get("body");
        let kind: String = row.get("notification_type");
        let metadata: Value = row.get("metadata");
        let allow_financial: bool = row.get("push_financial_details");

        // In-app delivery is canonical. Push failures are logged and retried,
        // never dropped; the notification itself remains visible to the user.
        let push_body = lock_screen_body(&kind, &body, &metadata, allow_financial);
        let result = push_to_user(pool, row.get("user_id"), &title, &push_body).await;
        let failed = result.is_err();
        let next_attempts = attempts + 1;
        let exhausted = failed && next_attempts >= MAX_DELIVERY_ATTEMPTS;
        let delay = Duration::seconds(2_i64.pow(next_attempts.max(1) as u32));
        let error = result.err().map(|e| e.to_string()).unwrap_or_default();

        let update = if failed && !exhausted {
            sqlx::query(
                r#"
                UPDATE circle_notification_deliveries
                   SET attempts = $1, last_error = $2, next_retry_at = $3, updated_at = NOW()
                 WHERE id = $4
                "#,
            )
            .bind(next_attempts)
            .bind(&error)
            .bind(Utc::now() + delay)
            .bind(id)
        } else {
            sqlx::query(
                r#"
                UPDATE circle_notification_deliveries
                   SET attempts = $1, status = $2, last_error = $3,
                       next_retry_at = NOW(), updated_at = NOW()
                 WHERE id = $4
                "#,
            )
            .bind(next_attempts)
            .bind(if failed { "failed" } else { "delivered" })
            .bind(&error)
            .bind(id)
        };

        if let Err(e) = update.execute(pool).await {
            tracing::error!(
                delivery_id = %id,
                error = %e,
                "Failed to update circle notification delivery state"
            );
        }
        if failed {
            tracing::warn!(
                notification_id = %notification_id,
                attempts = next_attempts,
                error = %error,
                "Circle notification push delivery failed"
            );
        }
    }
}

/// Keep amounts, addresses and transaction hashes off lock screens unless the
/// user explicitly opted in. The full message remains in the in-app inbox.
fn lock_screen_body(kind: &str, body: &str, metadata: &Value, allow_financial: bool) -> String {
    let financial_kind = matches!(kind, "payout_request" | "payout_approval");
    let has_financial_metadata = ["amount", "tx_hash", "to_address", "asset", "balance"]
        .iter()
        .any(|key| metadata.get(*key).is_some());
    if !allow_financial && (financial_kind || has_financial_metadata) {
        "You have a new circle notification. Open Zaps to view the details.".to_string()
    } else {
        body.to_string()
    }
}

async fn push_to_user(pool: &PgPool, user_id: Uuid, title: &str, body: &str) -> Result<(), String> {
    // Re-check the preference at dispatch time: users may opt out between
    // notification creation and retry.
    let token: Option<String> = sqlx::query_scalar(
        r#"
        SELECT t.expo_push_token
          FROM user_push_tokens t
          JOIN users u ON u.id = t.user_id
         WHERE t.user_id = $1 AND u.circle_push_enabled AND t.expo_push_token <> ''
         ORDER BY t.updated_at DESC
         LIMIT 1
        "#,
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?;

    let Some(token) = token else {
        return Ok(()); // No device registered: in-app notification is enough.
    };

    let client = reqwest::Client::new();
    let response = client
        .post("https://exp.host/--/api/v2/push/send")
        .json(&serde_json::json!({
            "to": token,
            "title": title,
            "body": body,
            "data": { "source": "circle" },
            "sound": "default",
        }))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !response.status().is_success() {
        return Err(format!("Expo push returned {}", response.status()));
    }

    // Expo reports per-token problems inside a successful HTTP response.
    // DeviceNotRegistered is permanent: remove the stale token so later
    // notifications are not blocked by it.
    if let Ok(payload) = response.json::<serde_json::Value>().await {
        let invalid = payload
            .get("data")
            .and_then(|d| d.as_array())
            .map(|tickets| {
                tickets.iter().any(|ticket| {
                    ticket.get("status").and_then(|s| s.as_str()) == Some("error")
                        && ticket
                            .get("details")
                            .and_then(|d| d.get("error"))
                            .and_then(|e| e.as_str())
                            == Some("DeviceNotRegistered")
                })
            })
            .unwrap_or(false);
        if invalid {
            let _ = sqlx::query("DELETE FROM user_push_tokens WHERE expo_push_token = $1")
                .bind(&token)
                .execute(pool)
                .await;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_notification_types_are_unique_and_standardized() {
        let all = CircleNotificationType::all();
        assert_eq!(all.len(), 9);
        assert_eq!(
            all.iter().collect::<std::collections::HashSet<_>>().len(),
            9
        );
        for (i, name) in all.iter().enumerate() {
            assert_eq!(
                CircleNotificationType::all()[i],
                *name,
                "type list must be stable"
            );
        }
    }

    #[test]
    fn lock_screen_hides_financial_details_by_default() {
        let metadata = serde_json::json!({ "amount": "1000" });
        let safe = lock_screen_body("payout_request", "Paid 1000 stroops", &metadata, false);
        assert!(!safe.contains("1000"));
        let full = lock_screen_body("payout_request", "Paid 1000 stroops", &metadata, true);
        assert!(full.contains("1000"));
    }

    #[test]
    fn notification_type_serde_uses_snake_case() {
        let json = serde_json::to_string(&CircleNotificationType::MembershipChange).unwrap();
        assert_eq!(json, "\"membership_change\"");
    }
}
