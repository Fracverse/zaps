//! #1222 — Append-only, tamper-evident circle audit log.
//!
//! Each row stores the SHA-256 hash of its predecessor and a canonical hash of
//! its own contents. A database trigger makes UPDATE/DELETE/TRUNCATE fail, and
//! `verify_chain` detects any out-of-band modification of existing rows.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct AuditEvent<'a> {
    pub circle_id: Uuid,
    pub actor: &'a str,
    pub action: &'a str,
    pub target: &'a str,
    pub details: Value,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct AuditEntry {
    pub seq: i64,
    pub id: Uuid,
    pub circle_id: Uuid,
    pub actor: String,
    pub action: String,
    pub target: String,
    pub details: Value,
    pub occurred_at: DateTime<Utc>,
    pub previous_hash: String,
    pub entry_hash: String,
}

const GENESIS_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";

/// Strip fields that could contain private keys or authentication secrets.
fn sanitize_details(mut value: Value) -> Value {
    const SENSITIVE: &[&str] = &[
        "private_key",
        "secret",
        "password",
        "authorization",
        "access_token",
        "auth_token",
        "session_token",
        "api_key",
    ];
    if let Some(map) = value.as_object_mut() {
        map.retain(|key, _| {
            let lower = key.to_lowercase();
            !SENSITIVE.iter().any(|s| lower.contains(s))
        });
    }
    value
}

fn entry_hash(
    id: Uuid,
    circle_id: Uuid,
    actor: &str,
    action: &str,
    target: &str,
    occurred_at: &str,
    details: &str,
    previous_hash: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(format!(
        "{id}|{circle_id}|{actor}|{action}|{target}|{occurred_at}|{details}|{previous_hash}"
    ));
    hex::encode(hasher.finalize())
}

/// Append an immutable audit entry. Concurrent appends to a circle are
/// serialized with a transaction-scoped advisory lock so the hash chain has a
/// single, well-defined order.
pub async fn append(pool: &PgPool, event: AuditEvent<'_>) -> Result<String, sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))")
        .bind(event.circle_id.to_string())
        .execute(&mut *tx)
        .await?;

    let previous_hash: String = sqlx::query_scalar(
        "SELECT COALESCE((SELECT entry_hash FROM circle_audit_logs WHERE circle_id = $1 ORDER BY seq DESC LIMIT 1), $2)",
    )
    .bind(event.circle_id)
    .bind(GENESIS_HASH)
    .fetch_one(&mut *tx)
    .await?;

    let id = Uuid::new_v4();
    let occurred_at = Utc::now();
    let details = sanitize_details(event.details);
    let hash = entry_hash(
        id,
        event.circle_id,
        event.actor,
        event.action,
        event.target,
        &occurred_at.to_rfc3339(),
        &details.to_string(),
        &previous_hash,
    );

    sqlx::query(
        r#"
        INSERT INTO circle_audit_logs
            (id, circle_id, actor, action, target, details, occurred_at,
             previous_hash, entry_hash)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        "#,
    )
    .bind(id)
    .bind(event.circle_id)
    .bind(event.actor)
    .bind(event.action)
    .bind(event.target)
    .bind(&details)
    .bind(occurred_at)
    .bind(&previous_hash)
    .bind(&hash)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(hash)
}

pub async fn list(
    pool: &PgPool,
    circle_id: Uuid,
    limit: i64,
    offset: i64,
) -> Result<Vec<AuditEntry>, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT seq, id, circle_id, actor, action, target, details,
               occurred_at, previous_hash, entry_hash
        FROM circle_audit_logs
        WHERE circle_id = $1
        ORDER BY seq DESC
        LIMIT $2 OFFSET $3
        "#,
    )
    .bind(circle_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| AuditEntry {
            seq: r.get("seq"),
            id: r.get("id"),
            circle_id: r.get("circle_id"),
            actor: r.get("actor"),
            action: r.get("action"),
            target: r.get("target"),
            details: r.get("details"),
            occurred_at: r.get("occurred_at"),
            previous_hash: r.get("previous_hash"),
            entry_hash: r.get("entry_hash"),
        })
        .collect())
}

/// Recompute every hash in order and report the first broken link, if any.
pub async fn verify_chain(pool: &PgPool, circle_id: Uuid) -> Result<bool, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT id, actor, action, target, details, occurred_at,
               previous_hash, entry_hash
        FROM circle_audit_logs WHERE circle_id = $1 ORDER BY seq ASC
        "#,
    )
    .bind(circle_id)
    .fetch_all(pool)
    .await?;

    let mut expected = GENESIS_HASH.to_string();
    for r in rows {
        let previous: String = r.get("previous_hash");
        let stored: String = r.get("entry_hash");
        let details: Value = r.get("details");
        let computed = entry_hash(
            r.get("id"),
            circle_id,
            r.get::<String, _>("actor").as_str(),
            r.get::<String, _>("action").as_str(),
            r.get::<String, _>("target").as_str(),
            &r.get::<DateTime<Utc>, _>("occurred_at").to_rfc3339(),
            &details.to_string(),
            &previous,
        );
        if previous != expected || computed != stored {
            return Ok(false);
        }
        expected = stored;
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sensitive_fields_are_removed_from_audit_details() {
        let value = serde_json::json!({
            "amount": 100,
            "private_key": "secret",
            "authorization": "Bearer x",
            "tx_hash": "abc"
        });
        let clean = sanitize_details(value);
        assert!(clean.get("amount").is_some());
        assert!(clean.get("tx_hash").is_some());
        assert!(clean.get("private_key").is_none());
        assert!(clean.get("authorization").is_none());
    }
}
