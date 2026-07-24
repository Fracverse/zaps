use chrono::{DateTime, NaiveDateTime, Utc};
use serde::Serialize;
use serde_json::json;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::db::r#yield::get_current_yield_rate;
use crate::services::yield_calc::SECONDS_PER_YEAR;

const EXPO_PUSH_URL: &str = "https://exp.host/--/api/v2/push/send";
const DEFAULT_YIELD_REPORT_THRESHOLD: i64 = 1_000;
const DEFAULT_DAILY_INTERVAL_SECS: u64 = 86_400;
const DEFAULT_WEEKLY_INTERVAL_SECS: u64 = 604_800;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum YieldReportCadence {
    Daily,
    Weekly,
}

pub struct NotificationSchedulerConfig {
    pub daily_interval: std::time::Duration,
    pub weekly_interval: std::time::Duration,
    pub yield_threshold: i64,
    pub expo_access_token: Option<String>,
}

impl NotificationSchedulerConfig {
    pub fn from_env() -> Self {
        let daily_secs = std::env::var("YIELD_REPORT_DAILY_INTERVAL_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_DAILY_INTERVAL_SECS);
        let weekly_secs = std::env::var("YIELD_REPORT_WEEKLY_INTERVAL_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_WEEKLY_INTERVAL_SECS);
        let threshold = std::env::var("YIELD_REPORT_THRESHOLD")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_YIELD_REPORT_THRESHOLD);

        Self {
            daily_interval: std::time::Duration::from_secs(daily_secs),
            weekly_interval: std::time::Duration::from_secs(weekly_secs),
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
struct ExpoPushMessage<'a> {
    to: &'a str,
    title: &'a str,
    body: &'a str,
    data: serde_json::Value,
    sound: &'a str,
}

/// BE-032: Fire daily and weekly yield summary push notifications.
pub async fn run(pool: PgPool, config: NotificationSchedulerConfig) {
    tracing::info!("Starting yield report notification scheduler");

    let daily_pool = pool.clone();
    let daily_config = config.clone_for_worker();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(daily_config.daily_interval);
        loop {
            interval.tick().await;
            if let Err(err) =
                send_yield_reports(&daily_pool, &daily_config, YieldReportCadence::Daily).await
            {
                tracing::error!("Daily yield report scheduler failed: {err:?}");
            }
        }
    });

    let mut weekly_interval = tokio::time::interval(config.weekly_interval);
    loop {
        weekly_interval.tick().await;
        if let Err(err) = send_yield_reports(&pool, &config, YieldReportCadence::Weekly).await {
            tracing::error!("Weekly yield report scheduler failed: {err:?}");
        }
    }
}

impl NotificationSchedulerConfig {
    fn clone_for_worker(&self) -> Self {
        Self {
            daily_interval: self.daily_interval,
            weekly_interval: self.weekly_interval,
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

        for token in &candidate.push_tokens {
            if let Err(err) = send_expo_push(
                token,
                title,
                &body,
                json!({
                    "target": "home",
                    "cadence": match cadence {
                        YieldReportCadence::Daily => "daily",
                        YieldReportCadence::Weekly => "weekly",
                    },
                    "earned": earned,
                }),
                config.expo_access_token.as_deref(),
            )
            .await
            {
                tracing::warn!(
                    user_id = %candidate.user_id,
                    error = ?err,
                    "Failed to send yield report push notification"
                );
            }
        }

        mark_report_sent(pool, candidate.user_id, cadence, now).await?;
        sent += 1;
    }

    tracing::info!(
        cadence = ?cadence,
        sent,
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

async fn send_expo_push(
    token: &str,
    title: &str,
    body: &str,
    data: serde_json::Value,
    access_token: Option<&str>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let message = ExpoPushMessage {
        to: token,
        title,
        body,
        data,
        sound: "default",
    };

    let mut request = reqwest::Client::new().post(EXPO_PUSH_URL).json(&message);
    if let Some(token) = access_token.filter(|t| !t.is_empty()) {
        request = request.bearer_auth(token);
    }

    let response = request.send().await?;
    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(format!("Expo push API returned {status}: {text}").into());
    }

    Ok(())
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
}
