//! Circle Reactions Service (#1215).
//!
//! Allows members to react to circle activities and comments with emoji.
//! Enforces one active reaction per user per target.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

// ── Configurable emoji list ───────────────────────────────────────────────────

/// Supported emoji reactions. Product can adjust this list here (or move it to
/// a config table in the future) without changing the API contract.
pub const ALLOWED_REACTIONS: &[&str] = &[
    "👍", "👎", "❤️", "🔥", "🎉", "😂", "👏", "🙌", "💯", "🚀", "💎", "😊", "😮", "😢", "😡",
];

pub fn is_valid_emoji(emoji: &str) -> bool {
    ALLOWED_REACTIONS.contains(&emoji)
}

// ── Request / Response types ──────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ReactionRequest {
    pub emoji: String,
}

#[derive(Debug, Serialize)]
pub struct ReactionEntry {
    pub user_address: String,
    pub emoji: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct ReactionCount {
    pub emoji: String,
    pub count: i64,
    pub viewer_reaction: bool,
}

#[derive(Debug, Serialize)]
pub struct ReactionListResponse {
    pub reactions: Vec<ReactionCount>,
    pub total: i64,
}

// ── Service functions ─────────────────────────────────────────────────────────

/// Add or update a reaction. If the user already reacted to this target, the
/// emoji is updated (change reaction). Duplicate requests are handled safely
/// via the unique constraint on (circle_id, target_type, target_id, user_address).
pub async fn add_or_update_reaction(
    pool: &PgPool,
    circle_id: Uuid,
    target_type: &str,
    target_id: Uuid,
    address: &str,
    emoji: &str,
) -> Result<(), &'static str> {
    if !is_valid_emoji(emoji) {
        return Err("invalid_emoji");
    }

    let now = Utc::now();

    let result = sqlx::query!(
        r#"
        INSERT INTO circle_reactions (circle_id, target_type, target_id, user_address, emoji, created_at, updated_at)
        VALUES ($1, $2, $3, $4, $5, $6, $6)
        ON CONFLICT (circle_id, target_type, target_id, user_address)
        DO UPDATE SET emoji = $5, updated_at = $6
        "#,
        circle_id,
        target_type,
        target_id,
        address,
        emoji,
        now
    )
    .execute(pool)
    .await;

    match result {
        Ok(_) => Ok(()),
        Err(_) => Err("internal_error"),
    }
}

/// Remove a user's reaction from a target. Returns `true` if a row was deleted.
pub async fn remove_reaction(
    pool: &PgPool,
    circle_id: Uuid,
    target_type: &str,
    target_id: Uuid,
    address: &str,
) -> Result<bool, &'static str> {
    let result = sqlx::query!(
        r#"
        DELETE FROM circle_reactions
        WHERE circle_id = $1 AND target_type = $2 AND target_id = $3 AND user_address = $4
        "#,
        circle_id,
        target_type,
        target_id,
        address
    )
    .execute(pool)
    .await;

    match result {
        Ok(res) => Ok(res.rows_affected() > 0),
        Err(_) => Err("internal_error"),
    }
}

/// Get reaction counts for a target, optionally marking which emoji the viewer used.
pub async fn get_reaction_counts(
    pool: &PgPool,
    circle_id: Uuid,
    target_type: &str,
    target_id: Uuid,
    viewer_address: Option<&str>,
) -> Result<Vec<ReactionCount>, sqlx::Error> {
    let rows = sqlx::query_as!(
        ReactionCountRow,
        r#"
        SELECT emoji, COUNT(*) as count
        FROM circle_reactions
        WHERE circle_id = $1 AND target_type = $2 AND target_id = $3
        GROUP BY emoji
        ORDER BY count DESC, emoji ASC
        "#,
        circle_id,
        target_type,
        target_id
    )
    .fetch_all(pool)
    .await?;

    let viewer_emoji = if let Some(addr) = viewer_address {
        sqlx::query_scalar!(
            "SELECT emoji FROM circle_reactions WHERE circle_id = $1 AND target_type = $2 AND target_id = $3 AND user_address = $4",
            circle_id,
            target_type,
            target_id,
            addr
        )
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
    } else {
        None
    };

    Ok(rows
        .into_iter()
        .map(|r| ReactionCount {
            emoji: r.emoji,
            count: r.count,
            viewer_reaction: viewer_emoji.as_ref() == Some(&r.emoji),
        })
        .collect())
}

/// Get the specific reaction a user has on a target, if any.
pub async fn get_user_reaction(
    pool: &PgPool,
    circle_id: Uuid,
    target_type: &str,
    target_id: Uuid,
    address: &str,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT emoji FROM circle_reactions WHERE circle_id = $1 AND target_type = $2 AND target_id = $3 AND user_address = $4",
        circle_id,
        target_type,
        target_id,
        address
    )
    .fetch_optional(pool)
    .await
}

#[derive(sqlx::FromRow)]
struct ReactionCountRow {
    emoji: String,
    count: i64,
}

// ── Unit Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_allowed_emojis_non_empty() {
        assert!(!ALLOWED_REACTIONS.is_empty());
    }

    #[test]
    fn test_is_valid_emoji() {
        assert!(is_valid_emoji("👍"));
        assert!(is_valid_emoji("❤️"));
        assert!(!is_valid_emoji("💩"));
        assert!(!is_valid_emoji(""));
    }
}
