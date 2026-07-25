use chrono::{DateTime, NaiveDateTime, Utc};
use serde::Serialize;
use serde_json::json;
use sqlx::{PgPool, Row};
use tokio_cron_scheduler::{Job, JobScheduler};
use uuid::Uuid;

use crate::db::r#yield::get_current_yield_rate;
use crate::services::yield_calc::SECONDS_PER_YEAR;

const EXPO_PUSH_URL: &str = "https://exp.host/--/api/v2/push/send";
const EXPO_PUSH_BATCH_SIZE: usize = 100;
const DEFAULT_YIELD_REPORT_THRESHOLD: i64 = 1_000;
// Cron expressions are `sec min hour day month day-of-week`, evaluated in UTC.
const DEFAULT_DAILY_CRON: &str = "0 0 0 * * *";
const DEFAULT_WEEKLY_CRON: &str = "0 0 0 * * Mon";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum YieldReportCadence {
    Daily,
    Weekly,
}

pub struct NotificationSchedulerConfig {
    pub daily_cron: String,
    pub weekly_cron: String,
    pub yield_threshold: i64,
    pub expo_access_token: Option<String>,
}

impl NotificationSchedulerConfig {
    pub fn from_env() -> Self {
        let daily_cron = std::env::var("YIELD_REPORT_DAILY_CRON")
            .unwrap_or_else(|_| DEFAULT_DAILY_CRON.to_string());
        let weekly_cron = std::env::var("YIELD_REPORT_WEEKLY_CRON")
            .unwrap_or_else(|_| DEFAULT_WEEKLY_CRON.to_string());
        let threshold = std::env::var("YIELD_REPORT_THRESHOLD")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_YIELD_REPORT_THRESHOLD);

        Self {
            daily_cron,
            weekly_cron,
            yield_threshold: threshold,
            expo_access_token: std::env::var("EXPO_ACCESS_TOKEN").ok(),
        }
    }
}

struct YieldReportCandidate {
    user_id: Uuid,
    username: String,
    earning_balance: i64,
    last_yield_sync_at: NaiveDateTime,
    last_report_at: Option<NaiveDateTime>,
    push_tokens: Vec<String>,
}

#[derive(Serialize)]
struct ExpoPushMessage {
    to: String,
    title: &'static str,
    body: String,
    data: serde_json::Value,
    sound: &'static str,
}

/// BE-032/BE-056: Fire daily and weekly yield summary push notifications at
/// exact wall-clock times using a cron schedule, rather than an interval
/// timer that drifts based on process start time.
pub async fn run(pool: PgPool, config: NotificationSchedulerConfig) {
    tracing::info!(
        daily_cron = %config.daily_cron,
        weekly_cron = %config.weekly_cron,
        "Starting yield report notification scheduler"
    );

    let scheduler = match JobScheduler::new().await {
        Ok(scheduler) => scheduler,
        Err(err) => {
            tracing::error!("Failed to create yield report notification scheduler: {err:?}");
            return;
        }
    };

    if let Err(err) = schedule_cadence(&scheduler, &pool, &config, YieldReportCadence::Daily).await
    {
        tracing::error!("Failed to schedule daily yield report job: {err:?}");
        return;
    }

    if let Err(err) =
        schedule_cadence(&scheduler, &pool, &config, YieldReportCadence::Weekly).await
    {
        tracing::error!("Failed to schedule weekly yield report job: {err:?}");
        return;
    }

    if let Err(err) = scheduler.start().await {
        tracing::error!("Failed to start yield report notification scheduler: {err:?}");
        return;
    }

    // The scheduler ticks on its own background task; keep this task alive
    // for as long as the process runs so that task isn't torn down.
    std::future::pending::<()>().await;
}

async fn schedule_cadence(
    scheduler: &JobScheduler,
    pool: &PgPool,
    config: &NotificationSchedulerConfig,
    cadence: YieldReportCadence,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let cron_expr = match cadence {
        YieldReportCadence::Daily => config.daily_cron.clone(),
        YieldReportCadence::Weekly => config.weekly_cron.clone(),
    };

    let job_pool = pool.clone();
    let job_config = config.clone_for_worker();
    let job = Job::new_async(cron_expr.as_str(), move |_uuid, _scheduler| {
        let pool = job_pool.clone();
        let config = job_config.clone_for_worker();
        Box::pin(async move {
            if let Err(err) = send_yield_reports(&pool, &config, cadence).await {
                tracing::error!(cadence = ?cadence, "Yield report scheduler run failed: {err:?}");
            }
        })
    })?;

    scheduler.add(job).await?;
    Ok(())
}

impl NotificationSchedulerConfig {
    fn clone_for_worker(&self) -> Self {
        Self {
            daily_cron: self.daily_cron.clone(),
            weekly_cron: self.weekly_cron.clone(),
            yield_threshold: self.yield_threshold,
            expo_access_token: self.expo_access_token.clone(),
        }
    }
}

async fn send_yield_reports(
    pool: &PgPool,
    config: &NotificationSchedulerConfig,
    cadence: YieldReportCadence,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let apy_bps = get_current_yield_rate(pool).await?.unwrap_or(500);
    let candidates = load_report_candidates(pool, cadence).await?;
    let now = Utc::now();

    let mut sent = 0usize;
    let mut messages = Vec::new();
    let mut report_user_ids = Vec::new();
    for candidate in candidates {
        let period_start = candidate
            .last_report_at
            .unwrap_or(candidate.last_yield_sync_at);
        let period_secs = (now - period_start.and_utc()).num_seconds().max(0);
        if period_secs <= 0 {
            continue;
        }

        let earned = estimate_period_yield(candidate.earning_balance, apy_bps, period_secs);

        if earned < config.yield_threshold {
            continue;
        }

        let title = match cadence {
            YieldReportCadence::Daily => "Your daily yield report",
            YieldReportCadence::Weekly => "Your weekly yield report",
        };
        let body = format!(
            "@{}, you earned {} micro-units in yield. Tap to view details.",
            candidate.username, earned
        );
        let data = json!({
            "target": "home",
            "cadence": match cadence {
                YieldReportCadence::Daily => "daily",
                YieldReportCadence::Weekly => "weekly",
            },
            "earned": earned,
        });

        for token in candidate.push_tokens {
            messages.push(ExpoPushMessage {
                to: token,
                title,
                body: body.clone(),
                data: data.clone(),
                sound: "default",
            });
        }

        report_user_ids.push(candidate.user_id);
        sent += 1;
    }

    let client = reqwest::Client::new();
    for (batch_index, batch) in expo_push_batches(&messages).enumerate() {
        if let Err(err) =
            send_expo_push_batch(&client, batch, config.expo_access_token.as_deref()).await
        {
            tracing::warn!(
                batch_number = batch_index + 1,
                batch_size = batch.len(),
                error = ?err,
                "Failed to send yield report push notification batch"
            );
        }
    }

    for user_id in report_user_ids {
        mark_report_sent(pool, user_id, cadence, now).await?;
    }

    tracing::info!(
        cadence = ?cadence,
        sent,
        notifications = messages.len(),
        "Yield report notification cycle complete"
    );
    Ok(())
}

fn estimate_period_yield(earning_balance: i64, apy_bps: i32, period_secs: i64) -> i64 {
    if earning_balance <= 0 || period_secs <= 0 {
        return 0;
    }

    earning_balance
        .saturating_mul(apy_bps as i64)
        .saturating_mul(period_secs)
        / (10_000 * SECONDS_PER_YEAR)
}

async fn load_report_candidates(
    pool: &PgPool,
    cadence: YieldReportCadence,
) -> Result<Vec<YieldReportCandidate>, sqlx::Error> {
    let report_column = match cadence {
        YieldReportCadence::Daily => "u.last_daily_yield_report_at",
        YieldReportCadence::Weekly => "u.last_weekly_yield_report_at",
    };

    let query = format!(
        r#"
        SELECT
            u.id AS user_id,
            u.username,
            b.earning_balance,
            b.last_yield_sync_at,
            {report_column} AS last_report_at,
            COALESCE(
                array_agg(t.expo_push_token) FILTER (WHERE t.expo_push_token IS NOT NULL),
                '{{}}'
            ) AS push_tokens
        FROM users u
        JOIN user_yield_balances b ON b.user_id = u.id
        LEFT JOIN user_push_tokens t ON t.user_id = u.id
        WHERE b.earning_balance > 0
        GROUP BY u.id, u.username, b.earning_balance, b.last_yield_sync_at, {report_column}
        "#
    );

    let rows = sqlx::query(&query).fetch_all(pool).await?;

    Ok(rows
        .into_iter()
        .filter_map(|row| {
            let tokens: Option<Vec<String>> = row.try_get("push_tokens").ok();
            let push_tokens: Vec<String> = tokens.unwrap_or_default();
            if push_tokens.is_empty() {
                return None;
            }
            Some(YieldReportCandidate {
                user_id: row.get("user_id"),
                username: row.get("username"),
                earning_balance: row.get("earning_balance"),
                last_yield_sync_at: row.get("last_yield_sync_at"),
                last_report_at: row.get("last_report_at"),
                push_tokens,
            })
        })
        .collect())
}

async fn mark_report_sent(
    pool: &PgPool,
    user_id: Uuid,
    cadence: YieldReportCadence,
    now: DateTime<Utc>,
) -> Result<(), sqlx::Error> {
    let naive = now.naive_utc();
    match cadence {
        YieldReportCadence::Daily => {
            sqlx::query("UPDATE users SET last_daily_yield_report_at = $2 WHERE id = $1")
                .bind(user_id)
                .bind(naive)
                .execute(pool)
                .await?;
        }
        YieldReportCadence::Weekly => {
            sqlx::query("UPDATE users SET last_weekly_yield_report_at = $2 WHERE id = $1")
                .bind(user_id)
                .bind(naive)
                .execute(pool)
                .await?;
        }
    }
    Ok(())
}

fn expo_push_batches(messages: &[ExpoPushMessage]) -> impl Iterator<Item = &[ExpoPushMessage]> {
    messages.chunks(EXPO_PUSH_BATCH_SIZE)
}

fn build_expo_push_request(
    client: &reqwest::Client,
    messages: &[ExpoPushMessage],
    access_token: Option<&str>,
) -> reqwest::RequestBuilder {
    let mut request = client.post(EXPO_PUSH_URL).json(messages);
    if let Some(token) = access_token.filter(|t| !t.is_empty()) {
        request = request.bearer_auth(token);
    }
    request
}

async fn send_expo_push_batch(
    client: &reqwest::Client,
    messages: &[ExpoPushMessage],
    access_token: Option<&str>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let response = build_expo_push_request(client, messages, access_token)
        .send()
        .await?;
    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(format!("Expo push API returned {status}: {text}").into());
    }

    let body: serde_json::Value = response.json().await?;
    if has_device_not_registered(&body) {
        sqlx::query(
            "DELETE FROM user_push_tokens WHERE user_id = $1 AND expo_push_token = $2",
        )
        .bind(user_id)
        .bind(token)
        .execute(pool)
        .await?;
        tracing::info!(
            user_id = %user_id,
            %token,
            "Deleted invalid Expo push token (DeviceNotRegistered)"
        );
    }

    Ok(())
}

fn has_device_not_registered(body: &serde_json::Value) -> bool {
    body.get("data")
        .and_then(|d| d.as_array())
        .map(|arr| {
            arr.iter().any(|ticket| {
                ticket.get("status").and_then(|s| s.as_str()) == Some("error")
                    && ticket
                        .get("details")
                        .and_then(|d| d.get("error"))
                        .and_then(|e| e.as_str())
                        == Some("DeviceNotRegistered")
            })
        })
        .unwrap_or(false)
}

/// Upsert an Expo push token for a user (used by mobile registration endpoint).
pub async fn upsert_push_token(
    pool: &PgPool,
    user_id: Uuid,
    token: &str,
    platform: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO user_push_tokens (user_id, expo_push_token, platform, updated_at)
        VALUES ($1, $2, $3, NOW())
        ON CONFLICT (user_id, expo_push_token) DO UPDATE
        SET platform = EXCLUDED.platform,
            updated_at = NOW()
        "#,
    )
    .bind(user_id)
    .bind(token)
    .bind(platform)
    .execute(pool)
    .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn period_yield_matches_linear_formula() {
        let earned = estimate_period_yield(2_000_000, 500, 86_400);
        let expected = 2_000_000 * 500 * 86_400 / (10_000 * SECONDS_PER_YEAR);
        assert_eq!(earned, expected);
    }

    #[test]
    fn expo_push_messages_are_batched_in_groups_of_100() {
        let messages = (0..201)
            .map(|index| test_message(format!("ExponentPushToken[{index}]")))
            .collect::<Vec<_>>();

        let batch_sizes = expo_push_batches(&messages)
            .map(|batch| batch.len())
            .collect::<Vec<_>>();

        assert_eq!(batch_sizes, vec![100, 100, 1]);
    }

    #[test]
    fn expo_push_request_serializes_a_message_array() {
        let client = reqwest::Client::new();
        let messages = vec![
            test_message("ExponentPushToken[first]".to_string()),
            test_message("ExponentPushToken[second]".to_string()),
        ];

        let request = build_expo_push_request(&client, &messages, None)
            .build()
            .unwrap();
        let payload: serde_json::Value =
            serde_json::from_slice(request.body().unwrap().as_bytes().unwrap()).unwrap();

        let payload = payload.as_array().unwrap();
        assert_eq!(payload.len(), 2);
        assert_eq!(payload[0]["to"], "ExponentPushToken[first]");
        assert_eq!(payload[1]["to"], "ExponentPushToken[second]");
    }

    fn test_message(to: String) -> ExpoPushMessage {
        ExpoPushMessage {
            to,
            title: "Test notification",
            body: "Test body".to_string(),
            data: json!({ "target": "home" }),
            sound: "default",
        }
    }
}
