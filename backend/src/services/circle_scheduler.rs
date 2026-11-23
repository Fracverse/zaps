//! Circle Contribution Reminder Scheduler Service (#1219).
//!
//! Evaluates contribution rules across all active circles, identifies upcoming
//! and overdue contribution obligations, and dispatches notifications while
//! preventing duplicates via persistent cycle logs.

use chrono::{DateTime, Datelike, Duration, TimeZone, Timelike, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

// ── Models ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReminderPreferences {
    pub circle_id: Uuid,
    pub member_address: String,
    pub enabled: bool,
    pub lead_time_hours: i32,
    pub channels: Vec<String>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateReminderPreferencesRequest {
    pub enabled: Option<bool>,
    pub lead_time_hours: Option<i32>,
    pub channels: Option<Vec<String>>,
}

#[derive(Debug, Default, Serialize)]
pub struct SchedulerMetrics {
    pub circles_scanned: i64,
    pub upcoming_reminders_sent: i64,
    pub overdue_reminders_sent: i64,
    pub duplicate_reminders_skipped: i64,
}

// ── Preferences Management ────────────────────────────────────────────────────

/// Get member reminder preferences (defaults to enabled, 24h lead time).
pub async fn get_reminder_preferences(
    pool: &PgPool,
    circle_id: Uuid,
    member_address: &str,
) -> Result<ReminderPreferences, sqlx::Error> {
    let row = sqlx::query!(
        r#"
        SELECT circle_id, member_address, enabled, lead_time_hours, channels, updated_at
        FROM circle_reminder_preferences
        WHERE circle_id = $1 AND member_address = $2
        "#,
        circle_id,
        member_address
    )
    .fetch_optional(pool)
    .await?;

    match row {
        Some(r) => Ok(ReminderPreferences {
            circle_id: r.circle_id,
            member_address: r.member_address,
            enabled: r.enabled,
            lead_time_hours: r.lead_time_hours,
            channels: r.channels,
            updated_at: r.updated_at,
        }),
        None => Ok(ReminderPreferences {
            circle_id,
            member_address: member_address.to_string(),
            enabled: true,
            lead_time_hours: 24,
            channels: vec!["in_app".to_string()],
            updated_at: Utc::now(),
        }),
    }
}

/// Update member reminder preferences.
pub async fn update_reminder_preferences(
    pool: &PgPool,
    circle_id: Uuid,
    member_address: &str,
    req: UpdateReminderPreferencesRequest,
) -> Result<ReminderPreferences, &'static str> {
    let current = get_reminder_preferences(pool, circle_id, member_address)
        .await
        .map_err(|_| "internal_error")?;

    let enabled = req.enabled.unwrap_or(current.enabled);
    let lead_time_hours = req
        .lead_time_hours
        .unwrap_or(current.lead_time_hours)
        .clamp(1, 168); // 1 hour to 7 days
    let channels = req.channels.unwrap_or(current.channels);

    let row = sqlx::query!(
        r#"
        INSERT INTO circle_reminder_preferences
            (circle_id, member_address, enabled, lead_time_hours, channels, updated_at)
        VALUES ($1, $2, $3, $4, $5, NOW())
        ON CONFLICT (circle_id, member_address)
        DO UPDATE SET
            enabled = EXCLUDED.enabled,
            lead_time_hours = EXCLUDED.lead_time_hours,
            channels = EXCLUDED.channels,
            updated_at = NOW()
        RETURNING circle_id, member_address, enabled, lead_time_hours, channels, updated_at
        "#,
        circle_id,
        member_address,
        enabled,
        lead_time_hours,
        &channels
    )
    .fetch_one(pool)
    .await
    .map_err(|_| "internal_error")?;

    Ok(ReminderPreferences {
        circle_id: row.circle_id,
        member_address: row.member_address,
        enabled: row.enabled,
        lead_time_hours: row.lead_time_hours,
        channels: row.channels,
        updated_at: row.updated_at,
    })
}

// ── Cycle Calculation ─────────────────────────────────────────────────────────

/// Compute the cycle start, deadline, and unique cycle reference for a frequency.
pub fn calculate_cycle_window(
    frequency: &str,
    now: DateTime<Utc>,
) -> Option<(DateTime<Utc>, DateTime<Utc>, String)> {
    match frequency.to_lowercase().as_str() {
        "daily" => {
            let start = Utc
                .with_ymd_and_hms(now.year(), now.month(), now.day(), 0, 0, 0)
                .single()?;
            let deadline = Utc
                .with_ymd_and_hms(now.year(), now.month(), now.day(), 23, 59, 59)
                .single()?;
            let reference = format!("daily_{}", start.format("%Y%m%d"));
            Some((start, deadline, reference))
        }
        "weekly" => {
            // ISO week Monday 00:00:00 to Sunday 23:59:59
            let days_from_monday = now.weekday().num_days_from_monday() as i64;
            let monday = (now - Duration::days(days_from_monday))
                .with_hour(0)?
                .with_minute(0)?
                .with_second(0)?;
            let sunday = (monday + Duration::days(6))
                .with_hour(23)?
                .with_minute(59)?
                .with_second(59)?;
            let reference = format!("weekly_{}_w{}", now.format("%G"), now.format("%V"));
            Some((monday, sunday, reference))
        }
        "monthly" => {
            let start = Utc
                .with_ymd_and_hms(now.year(), now.month(), 1, 0, 0, 0)
                .single()?;
            // Next month day 1 minus 1 second
            let (next_year, next_month) = if now.month() == 12 {
                (now.year() + 1, 1)
            } else {
                (now.year(), now.month() + 1)
            };
            let next_start = Utc
                .with_ymd_and_hms(next_year, next_month, 1, 0, 0, 0)
                .single()?;
            let deadline = next_start - Duration::seconds(1);
            let reference = format!("monthly_{}", start.format("%Y%m"));
            Some((start, deadline, reference))
        }
        _ => None,
    }
}

// ── Scheduler Engine ──────────────────────────────────────────────────────────

/// Process contribution deadlines and generate reminders.
///
/// Idempotent: uses `circle_reminder_logs` unique constraint to prevent duplicate alerts.
pub async fn process_contribution_reminders(
    pool: &PgPool,
    now: DateTime<Utc>,
) -> Result<SchedulerMetrics, sqlx::Error> {
    let mut metrics = SchedulerMetrics::default();

    // Query active recurring contribution rules
    let rules = sqlx::query!(
        r#"
        SELECT
            cr.circle_id, cr.min_amount, cr.frequency,
            c.name AS circle_name
        FROM circle_contribution_rules cr
        JOIN circles c ON c.id = cr.circle_id
        WHERE cr.frequency IN ('daily', 'weekly', 'monthly')
          AND cr.min_amount > 0
        "#
    )
    .fetch_all(pool)
    .await?;

    for rule in rules {
        metrics.circles_scanned += 1;

        let Some((cycle_start, deadline, cycle_ref)) =
            calculate_cycle_window(&rule.frequency, now)
        else {
            continue;
        };

        // Query all members and their reminder preferences
        let members = sqlx::query!(
            r#"
            SELECT
                cm.address,
                COALESCE(rp.enabled, TRUE) AS "enabled!",
                COALESCE(rp.lead_time_hours, 24) AS "lead_time_hours!"
            FROM circle_members cm
            LEFT JOIN circle_reminder_preferences rp
                ON rp.circle_id = cm.circle_id AND rp.member_address = cm.address
            WHERE cm.circle_id = $1
            "#,
            rule.circle_id
        )
        .fetch_all(pool)
        .await?;

        for member in members {
            if !member.enabled {
                continue;
            }

            // Sum member contributions in the current cycle
            let contributed: i64 = sqlx::query_scalar!(
                r#"
                SELECT COALESCE(SUM(amount), 0)
                FROM circle_contributions
                WHERE circle_id = $1
                  AND member_address = $2
                  AND status = 'confirmed'
                  AND created_at >= $3
                  AND created_at <= $4
                "#,
                rule.circle_id,
                member.address,
                cycle_start,
                deadline
            )
            .fetch_one(pool)
            .await?
            .unwrap_or(0);

            if contributed >= rule.min_amount {
                // Member fulfilled their contribution for this cycle
                continue;
            }

            let remaining = rule.min_amount - contributed;
            let lead_time = Duration::hours(member.lead_time_hours as i64);

            // Determine if overdue or upcoming
            let (reminder_type, title, body) = if now > deadline {
                let title = format!("Payment Overdue: {}", rule.circle_name);
                let body = format!(
                    "Your contribution of {} is overdue for the cycle {}. Please contribute to maintain good standing.",
                    remaining, cycle_ref
                );
                ("overdue", title, body)
            } else if now >= (deadline - lead_time) {
                let title = format!("Upcoming Contribution: {}", rule.circle_name);
                let body = format!(
                    "Reminder: A contribution of {} is due by {}.",
                    remaining,
                    deadline.format("%Y-%m-%d %H:%M UTC")
                );
                ("upcoming", title, body)
            } else {
                continue;
            };

            // Attempt to log reminder (idempotent duplicate prevention)
            let log_res = sqlx::query!(
                r#"
                INSERT INTO circle_reminder_logs
                    (circle_id, member_address, reminder_type, cycle_reference)
                VALUES ($1, $2, $3, $4)
                ON CONFLICT (circle_id, member_address, reminder_type, cycle_reference)
                DO NOTHING
                RETURNING id
                "#,
                rule.circle_id,
                member.address,
                reminder_type,
                cycle_ref
            )
            .fetch_optional(pool)
            .await?;

            if log_res.is_none() {
                // Duplicate already dispatched in this cycle
                metrics.duplicate_reminders_skipped += 1;
                continue;
            }

            // Dispatch notification to user's circle notifications inbox
            let metadata = serde_json::json!({
                "cycle_reference": cycle_ref,
                "deadline": deadline,
                "remaining_amount": remaining.to_string(),
                "frequency": rule.frequency,
                "circle_id": rule.circle_id,
            });

            let notif_type = format!("{reminder_type}_contribution");

            let _ = sqlx::query!(
                r#"
                INSERT INTO circle_notifications
                    (circle_id, recipient_address, notification_type, title, body, metadata)
                VALUES ($1, $2, $3, $4, $5, $6)
                "#,
                rule.circle_id,
                member.address,
                notif_type,
                title,
                body,
                metadata
            )
            .execute(pool)
            .await;

            if reminder_type == "overdue" {
                metrics.overdue_reminders_sent += 1;
            } else {
                metrics.upcoming_reminders_sent += 1;
            }
        }
    }

    Ok(metrics)
}

/// Continuous background loop for the reminder scheduler.
pub async fn run(pool: PgPool, poll_interval: std::time::Duration) {
    tracing::info!(
        "Starting Circle Contribution Reminder Scheduler with interval {:?}",
        poll_interval
    );
    let mut interval = tokio::time::interval(poll_interval);

    loop {
        interval.tick().await;
        let now = Utc::now();
        match process_contribution_reminders(&pool, now).await {
            Ok(metrics) => {
                if metrics.upcoming_reminders_sent > 0 || metrics.overdue_reminders_sent > 0 {
                    tracing::info!(
                        "Reminder Scheduler cycle: {} circles, {} upcoming sent, {} overdue sent, {} duplicates suppressed",
                        metrics.circles_scanned,
                        metrics.upcoming_reminders_sent,
                        metrics.overdue_reminders_sent,
                        metrics.duplicate_reminders_skipped
                    );
                }
            }
            Err(e) => {
                tracing::error!("Error executing circle reminder scheduler cycle: {e}");
            }
        }
    }
}

// ── Unit Tests (#1219) ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_cycle_window_daily() {
        let now = Utc.with_ymd_and_hms(2026, 11, 22, 14, 30, 0).unwrap();
        let (start, deadline, reference) = calculate_cycle_window("daily", now).unwrap();

        assert_eq!(start, Utc.with_ymd_and_hms(2026, 11, 22, 0, 0, 0).unwrap());
        assert_eq!(
            deadline,
            Utc.with_ymd_and_hms(2026, 11, 22, 23, 59, 59).unwrap()
        );
        assert_eq!(reference, "daily_20261122");
    }

    #[test]
    fn test_calculate_cycle_window_weekly() {
        // 2026-11-22 is Sunday
        let now = Utc.with_ymd_and_hms(2026, 11, 22, 12, 0, 0).unwrap();
        let (monday, sunday, reference) = calculate_cycle_window("weekly", now).unwrap();

        assert_eq!(
            monday,
            Utc.with_ymd_and_hms(2026, 11, 16, 0, 0, 0).unwrap()
        );
        assert_eq!(
            sunday,
            Utc.with_ymd_and_hms(2026, 11, 22, 23, 59, 59).unwrap()
        );
        assert!(reference.starts_with("weekly_2026_w47"));
    }

    #[test]
    fn test_calculate_cycle_window_monthly() {
        let now = Utc.with_ymd_and_hms(2026, 11, 15, 10, 0, 0).unwrap();
        let (start, deadline, reference) = calculate_cycle_window("monthly", now).unwrap();

        assert_eq!(start, Utc.with_ymd_and_hms(2026, 11, 1, 0, 0, 0).unwrap());
        assert_eq!(
            deadline,
            Utc.with_ymd_and_hms(2026, 11, 30, 23, 59, 59).unwrap()
        );
        assert_eq!(reference, "monthly_202611");
    }

    #[test]
    fn test_invalid_frequency_returns_none() {
        let now = Utc::now();
        assert!(calculate_cycle_window("none", now).is_none());
        assert!(calculate_cycle_window("hourly", now).is_none());
    }
}

