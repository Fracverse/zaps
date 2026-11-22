//! #1204 — Circle savings goals.
//!
//! A circle may run several goals at once, up to its `max_active_goals`
//! setting. Progress is never taken from client input: it is the sum of the
//! circle's contributions inside the goal's window (`created_at` up to the
//! deadline / completion time) whose transaction the reconciliation worker has
//! confirmed on-chain (`on_chain_state = 'success'`, no discrepancy).
//!
//! Status is derived, not stored, by [`goal_status`]:
//! `completed` (closed by an authorized member) › `reached` (verified progress
//! ≥ target, awaiting completion) › `expired` (deadline passed) › `active`.

use crate::services::circle_membership::{self, CirclePermission};
use crate::services::circle_notifications as circle_notifier;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize};
use sqlx::postgres::PgRow;
use sqlx::{PgPool, Row};
use uuid::Uuid;

pub const MAX_GOAL_NAME_LEN: usize = 80;
pub const MAX_GOAL_DESCRIPTION_LEN: usize = 500;
/// 10^17 stroops = 10 billion whole tokens: a generous sanity cap.
pub const MAX_TARGET_AMOUNT: i64 = 100_000_000_000_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GoalStatus {
    Active,
    Reached,
    Completed,
    Expired,
}

impl GoalStatus {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "active" => Some(Self::Active),
            "reached" => Some(Self::Reached),
            "completed" => Some(Self::Completed),
            "expired" => Some(Self::Expired),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct GoalEntry {
    pub id: Uuid,
    pub circle_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub target_amount: String,
    /// Sum of on-chain-verified contributions inside the goal window.
    pub verified_progress: String,
    pub remaining_amount: String,
    pub progress_pct: f64,
    pub deadline: Option<DateTime<Utc>>,
    pub status: GoalStatus,
    pub created_by: Option<String>,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub completed_by: Option<String>,
}

fn double_option<'de, T, D>(de: D) -> Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    Option::<T>::deserialize(de).map(Some)
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewGoal {
    pub name: String,
    pub description: Option<String>,
    /// Stroops as a decimal string, matching the rest of the circles API.
    pub target_amount: String,
    pub deadline: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GoalUpdate {
    pub name: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    pub description: Option<Option<String>>,
    pub target_amount: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    pub deadline: Option<Option<DateTime<Utc>>>,
}

// ── Pure rules ────────────────────────────────────────────────────────────────

pub fn goal_status(
    completed: bool,
    deadline: Option<DateTime<Utc>>,
    progress: i64,
    target: i64,
    now: DateTime<Utc>,
) -> GoalStatus {
    if completed {
        GoalStatus::Completed
    } else if progress >= target {
        GoalStatus::Reached
    } else if deadline.map(|d| d <= now).unwrap_or(false) {
        GoalStatus::Expired
    } else {
        GoalStatus::Active
    }
}

/// Percentage of target reached, clamped to 0–100 and rounded to 2 decimals.
pub fn progress_pct(progress: i64, target: i64) -> f64 {
    if target <= 0 {
        return 0.0;
    }
    let pct = (progress.max(0) as f64 / target as f64 * 100.0).min(100.0);
    (pct * 100.0).round() / 100.0
}

pub fn validate_goal_name(raw: &str) -> Result<String, &'static str> {
    let name = raw.trim();
    if name.is_empty() {
        return Err("name_required");
    }
    if name.chars().count() > MAX_GOAL_NAME_LEN {
        return Err("name_too_long");
    }
    Ok(name.to_string())
}

pub fn validate_description(raw: Option<String>) -> Result<Option<String>, &'static str> {
    let desc = raw.map(|d| d.trim().to_string()).filter(|d| !d.is_empty());
    match desc {
        Some(d) if d.chars().count() > MAX_GOAL_DESCRIPTION_LEN => Err("description_too_long"),
        d => Ok(d),
    }
}

/// Targets are positive integer stroop amounts below [`MAX_TARGET_AMOUNT`].
pub fn validate_target(raw: &str) -> Result<i64, &'static str> {
    let raw = raw.trim();
    if raw.is_empty() || !raw.bytes().all(|b| b.is_ascii_digit()) {
        return Err("invalid_target_amount");
    }
    match raw.parse::<i64>() {
        Ok(v) if v > 0 && v <= MAX_TARGET_AMOUNT => Ok(v),
        Ok(v) if v > MAX_TARGET_AMOUNT => Err("target_amount_too_large"),
        Err(_) => Err("target_amount_too_large"),
        _ => Err("invalid_target_amount"),
    }
}

pub fn validate_deadline(
    deadline: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> Result<Option<DateTime<Utc>>, &'static str> {
    match deadline {
        Some(d) if d <= now => Err("deadline_in_past"),
        d => Ok(d),
    }
}

// ── Persistence ───────────────────────────────────────────────────────────────

/// Goal columns plus verified progress, computed in one round trip.
const GOAL_SELECT: &str = r#"
    SELECT g.id, g.circle_id, g.name, g.description, g.target_amount, g.deadline,
           g.completed, g.created_by, g.created_at, g.completed_at, g.completed_by,
           COALESCE((
               SELECT SUM(c.amount)
               FROM circle_contributions c
               WHERE c.circle_id = g.circle_id
                 AND c.created_at >= g.created_at
                 AND (g.deadline IS NULL OR c.created_at <= g.deadline)
                 AND (g.completed_at IS NULL OR c.created_at <= g.completed_at)
                 AND EXISTS (
                     SELECT 1 FROM circle_reconciliation_records r
                     WHERE r.record_type = 'contribution'
                       AND r.local_record_id = c.id
                       AND r.on_chain_state = 'success'
                       AND r.discrepancy = 'none'
                 )
           ), 0)::BIGINT AS verified_progress
    FROM circle_goals g
"#;

fn row_to_goal(row: &PgRow, now: DateTime<Utc>) -> GoalEntry {
    let target: i64 = row.get("target_amount");
    let progress: i64 = row.get("verified_progress");
    let deadline: Option<DateTime<Utc>> = row.get("deadline");
    GoalEntry {
        id: row.get("id"),
        circle_id: row.get("circle_id"),
        name: row.get("name"),
        description: row.get("description"),
        target_amount: target.to_string(),
        verified_progress: progress.to_string(),
        remaining_amount: (target - progress).max(0).to_string(),
        progress_pct: progress_pct(progress, target),
        deadline,
        status: goal_status(row.get("completed"), deadline, progress, target, now),
        created_by: row.get("created_by"),
        created_at: row.get("created_at"),
        completed_at: row.get("completed_at"),
        completed_by: row.get("completed_by"),
    }
}

async fn fetch_goal(
    pool: &PgPool,
    circle_id: Uuid,
    goal_id: Uuid,
) -> Result<GoalEntry, &'static str> {
    let row = sqlx::query(&format!(
        "{GOAL_SELECT} WHERE g.id = $1 AND g.circle_id = $2"
    ))
    .bind(goal_id)
    .bind(circle_id)
    .fetch_optional(pool)
    .await
    .map_err(|_| "internal_error")?
    .ok_or("goal_not_found")?;
    Ok(row_to_goal(&row, Utc::now()))
}

async fn record_activity(
    pool: &PgPool,
    circle_id: Uuid,
    actor: &str,
    activity_type: &str,
    metadata: serde_json::Value,
) {
    if let Err(e) = sqlx::query(
        "INSERT INTO circle_activities (circle_id, actor_address, activity_type, metadata)
         VALUES ($1, $2, $3, $4)",
    )
    .bind(circle_id)
    .bind(actor)
    .bind(activity_type)
    .bind(metadata)
    .execute(pool)
    .await
    {
        tracing::warn!(circle_id = %circle_id, activity_type, error = %e, "Failed to record goal activity");
    }
}

/// List a circle's goals, optionally filtered by derived status. Members only.
pub async fn list_goals(
    pool: &PgPool,
    circle_id: Uuid,
    viewer: &str,
    status: Option<GoalStatus>,
) -> Result<Vec<GoalEntry>, &'static str> {
    circle_membership::require_circle_permission(
        pool,
        circle_id,
        viewer,
        CirclePermission::ViewFinancialInfo,
    )
    .await?;

    let rows = sqlx::query(&format!(
        "{GOAL_SELECT} WHERE g.circle_id = $1 ORDER BY g.completed, g.created_at"
    ))
    .bind(circle_id)
    .fetch_all(pool)
    .await
    .map_err(|_| "internal_error")?;

    let now = Utc::now();
    Ok(rows
        .iter()
        .map(|r| row_to_goal(r, now))
        .filter(|g| status.map(|s| g.status == s).unwrap_or(true))
        .collect())
}

/// Fetch one goal. Members only.
pub async fn get_goal(
    pool: &PgPool,
    circle_id: Uuid,
    goal_id: Uuid,
    viewer: &str,
) -> Result<GoalEntry, &'static str> {
    circle_membership::require_circle_permission(
        pool,
        circle_id,
        viewer,
        CirclePermission::ViewFinancialInfo,
    )
    .await?;
    fetch_goal(pool, circle_id, goal_id).await
}

/// Create a goal. Requires `ManageGoals` and a free active-goal slot.
pub async fn create_goal(
    pool: &PgPool,
    circle_id: Uuid,
    actor: &str,
    input: NewGoal,
) -> Result<GoalEntry, &'static str> {
    circle_membership::require_circle_permission(
        pool,
        circle_id,
        actor,
        CirclePermission::ManageGoals,
    )
    .await?;

    let now = Utc::now();
    let name = validate_goal_name(&input.name)?;
    let description = validate_description(input.description)?;
    let target = validate_target(&input.target_amount)?;
    let deadline = validate_deadline(input.deadline, now)?;

    let mut tx = pool.begin().await.map_err(|_| "internal_error")?;

    // The circle row lock serialises concurrent creates against the limit.
    let max_active: i32 =
        sqlx::query_scalar("SELECT max_active_goals FROM circles WHERE id = $1 FOR UPDATE")
            .bind(circle_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|_| "internal_error")?
            .ok_or("circle_not_found")?;

    let open: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM circle_goals
         WHERE circle_id = $1 AND NOT completed AND (deadline IS NULL OR deadline > NOW())",
    )
    .bind(circle_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|_| "internal_error")?;

    if open >= max_active as i64 {
        return Err("active_goal_limit_reached");
    }

    let goal_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO circle_goals
             (id, circle_id, name, description, target_amount, deadline, created_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(goal_id)
    .bind(circle_id)
    .bind(&name)
    .bind(&description)
    .bind(target)
    .bind(deadline)
    .bind(actor)
    .execute(&mut *tx)
    .await
    .map_err(|_| "internal_error")?;

    tx.commit().await.map_err(|_| "internal_error")?;

    record_activity(
        pool,
        circle_id,
        actor,
        "goal_created",
        serde_json::json!({ "goal_id": goal_id, "name": name, "target_amount": target.to_string() }),
    )
    .await;

    fetch_goal(pool, circle_id, goal_id).await
}

/// Update an open goal's name, description, target or deadline.
pub async fn update_goal(
    pool: &PgPool,
    circle_id: Uuid,
    goal_id: Uuid,
    actor: &str,
    update: GoalUpdate,
) -> Result<GoalEntry, &'static str> {
    circle_membership::require_circle_permission(
        pool,
        circle_id,
        actor,
        CirclePermission::ManageGoals,
    )
    .await?;

    if update.name.is_none()
        && update.description.is_none()
        && update.target_amount.is_none()
        && update.deadline.is_none()
    {
        return Err("empty_update");
    }

    let now = Utc::now();
    let current = fetch_goal(pool, circle_id, goal_id).await?;
    if current.status == GoalStatus::Completed {
        return Err("goal_already_completed");
    }

    let name = update
        .name
        .as_deref()
        .map(validate_goal_name)
        .transpose()?
        .unwrap_or(current.name);
    let description = match update.description {
        Some(d) => validate_description(d)?,
        None => current.description,
    };
    let target = match update.target_amount.as_deref() {
        Some(t) => validate_target(t)?,
        None => current
            .target_amount
            .parse()
            .map_err(|_| "internal_error")?,
    };
    let deadline = match update.deadline {
        Some(d) => validate_deadline(d, now)?,
        None => current.deadline,
    };

    let updated = sqlx::query(
        "UPDATE circle_goals
         SET name = $3, description = $4, target_amount = $5, deadline = $6, updated_at = NOW()
         WHERE id = $1 AND circle_id = $2 AND NOT completed",
    )
    .bind(goal_id)
    .bind(circle_id)
    .bind(&name)
    .bind(&description)
    .bind(target)
    .bind(deadline)
    .execute(pool)
    .await
    .map_err(|_| "internal_error")?;

    if updated.rows_affected() == 0 {
        // Completed concurrently between the read and the write.
        return Err("goal_already_completed");
    }

    record_activity(
        pool,
        circle_id,
        actor,
        "goal_updated",
        serde_json::json!({ "goal_id": goal_id, "target_amount": target.to_string(), "deadline": deadline }),
    )
    .await;

    fetch_goal(pool, circle_id, goal_id).await
}

/// Mark a goal completed once verified progress has reached its target.
///
/// Returns `(goal, newly_completed)`; completing an already-completed goal is
/// an idempotent success with `newly_completed = false`.
pub async fn complete_goal(
    pool: &PgPool,
    circle_id: Uuid,
    goal_id: Uuid,
    actor: &str,
) -> Result<(GoalEntry, bool), &'static str> {
    circle_membership::require_circle_permission(
        pool,
        circle_id,
        actor,
        CirclePermission::ManageGoals,
    )
    .await?;

    let current = fetch_goal(pool, circle_id, goal_id).await?;
    match current.status {
        GoalStatus::Completed => return Ok((current, false)),
        GoalStatus::Reached => {}
        GoalStatus::Active | GoalStatus::Expired => return Err("goal_target_not_reached"),
    }

    let progress: i64 = current
        .verified_progress
        .parse()
        .map_err(|_| "internal_error")?;
    let updated = sqlx::query(
        "UPDATE circle_goals
         SET completed = TRUE, completed_at = NOW(), completed_by = $3,
             current_amount = $4, updated_at = NOW()
         WHERE id = $1 AND circle_id = $2 AND NOT completed",
    )
    .bind(goal_id)
    .bind(circle_id)
    .bind(actor)
    .bind(progress)
    .execute(pool)
    .await
    .map_err(|_| "internal_error")?;

    let goal = fetch_goal(pool, circle_id, goal_id).await?;
    if updated.rows_affected() == 0 {
        // Another request completed it first: report idempotently.
        return Ok((goal, false));
    }

    record_activity(
        pool,
        circle_id,
        actor,
        "goal_completed",
        serde_json::json!({ "goal_id": goal_id, "verified_progress": goal.verified_progress }),
    )
    .await;
    notify_members_goal_completed(pool, &goal).await;

    Ok((goal, true))
}

async fn notify_members_goal_completed(pool: &PgPool, goal: &GoalEntry) {
    let members: Vec<String> = match sqlx::query_scalar(
        "SELECT address FROM circle_members WHERE circle_id = $1",
    )
    .bind(goal.circle_id)
    .fetch_all(pool)
    .await
    {
        Ok(m) => m,
        Err(e) => {
            tracing::warn!(goal_id = %goal.id, error = %e, "Failed to load members for goal notification");
            return;
        }
    };

    let body = format!("The circle reached its \"{}\" savings goal.", goal.name);
    for member in &members {
        let dedupe_key = format!("circle_goal_completed:{}:{}", goal.id, member);
        let _ = circle_notifier::notify(
            pool,
            &circle_notifier::NewCircleNotification {
                circle_id: Some(goal.circle_id),
                recipient_address: member,
                kind: circle_notifier::CircleNotificationType::GoalCompleted,
                title: "Savings goal completed",
                body: &body,
                metadata: serde_json::json!({ "goal_id": goal.id }),
                dedupe_key: Some(&dedupe_key),
                push_token: None,
            },
        )
        .await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn status_precedence() {
        let now = Utc::now();
        let past = Some(now - Duration::days(1));
        let future = Some(now + Duration::days(1));
        assert_eq!(goal_status(true, past, 0, 100, now), GoalStatus::Completed);
        assert_eq!(goal_status(false, past, 100, 100, now), GoalStatus::Reached);
        assert_eq!(goal_status(false, past, 99, 100, now), GoalStatus::Expired);
        assert_eq!(goal_status(false, future, 99, 100, now), GoalStatus::Active);
        assert_eq!(goal_status(false, None, 0, 100, now), GoalStatus::Active);
    }

    #[test]
    fn progress_percentage_is_clamped_and_rounded() {
        assert_eq!(progress_pct(0, 100), 0.0);
        assert_eq!(progress_pct(1, 3), 33.33);
        assert_eq!(progress_pct(250, 100), 100.0);
        assert_eq!(progress_pct(-5, 100), 0.0);
        assert_eq!(progress_pct(5, 0), 0.0);
    }

    #[test]
    fn target_validation() {
        assert_eq!(validate_target("5000000"), Ok(5_000_000));
        assert_eq!(validate_target(" 42 "), Ok(42));
        assert_eq!(validate_target("0"), Err("invalid_target_amount"));
        assert_eq!(validate_target("-10"), Err("invalid_target_amount"));
        assert_eq!(validate_target("1.5"), Err("invalid_target_amount"));
        assert_eq!(validate_target(""), Err("invalid_target_amount"));
        assert_eq!(
            validate_target("99999999999999999999"),
            Err("target_amount_too_large")
        );
        assert_eq!(
            validate_target(&(MAX_TARGET_AMOUNT + 1).to_string()),
            Err("target_amount_too_large")
        );
    }

    #[test]
    fn name_and_description_validation() {
        assert_eq!(validate_goal_name("  Rent  "), Ok("Rent".to_string()));
        assert_eq!(validate_goal_name(""), Err("name_required"));
        assert_eq!(
            validate_goal_name(&"g".repeat(MAX_GOAL_NAME_LEN + 1)),
            Err("name_too_long")
        );
        assert_eq!(validate_description(Some("   ".into())), Ok(None));
        assert_eq!(
            validate_description(Some("d".repeat(MAX_GOAL_DESCRIPTION_LEN + 1))),
            Err("description_too_long")
        );
    }

    #[test]
    fn deadline_must_be_in_future() {
        let now = Utc::now();
        assert_eq!(validate_deadline(None, now), Ok(None));
        assert_eq!(
            validate_deadline(Some(now - Duration::seconds(1)), now),
            Err("deadline_in_past")
        );
        assert!(validate_deadline(Some(now + Duration::hours(1)), now).is_ok());
    }

    #[test]
    fn status_filter_parses() {
        assert_eq!(GoalStatus::parse("Reached"), Some(GoalStatus::Reached));
        assert_eq!(GoalStatus::parse("nope"), None);
    }

    #[test]
    fn update_payload_distinguishes_null_from_absent() {
        let u: GoalUpdate = serde_json::from_str(r#"{"deadline": null}"#).unwrap();
        assert_eq!(u.deadline, Some(None));
        let u: GoalUpdate = serde_json::from_str(r#"{"name": "x"}"#).unwrap();
        assert_eq!(u.deadline, None);
        assert!(serde_json::from_str::<GoalUpdate>(r#"{"completed": true}"#).is_err());
    }
}
