//! #1216 — Circle announcements API.
//!
//! Administrators and moderators publish, edit and remove circle announcements.
//! Members can read the announcement history. Publishing fans out an in-app
//! notification to every other circle member (deduplicated by announcement id).

use crate::api::auth_middleware::AuthenticatedUser;
use crate::api::circles::CirclesState;
use crate::services::circle_membership::{self, CirclePermission};
use crate::services::circle_notifications::{self as notifier, CircleNotificationType};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{PgPool, Row};
use uuid::Uuid;

const MAX_CONTENT_LEN: usize = 10_000;
const MAX_ATTACHMENTS: usize = 10;

#[derive(Debug, Deserialize)]
pub struct CreateAnnouncementRequest {
    pub content: String,
    pub attachments: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateAnnouncementRequest {
    pub content: String,
    pub attachments: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub struct AnnouncementQuery {
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct AnnouncementEntry {
    pub id: Uuid,
    pub circle_id: Uuid,
    pub author_address: String,
    pub content: String,
    pub attachments: Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

fn error(status: StatusCode, code: &'static str) -> Response {
    (status, Json(serde_json::json!({ "error": code }))).into_response()
}

fn validate_payload(content: &str, attachments: Option<Vec<String>>) -> Option<(String, Value)> {
    let content = content.trim();
    if content.is_empty() || content.len() > MAX_CONTENT_LEN {
        return None;
    }
    let attachments = attachments.unwrap_or_default();
    if attachments.len() > MAX_ATTACHMENTS
        || attachments.iter().any(|a| a.trim().is_empty() || a.len() > 512)
    {
        return None;
    }
    Some((content.to_string(), serde_json::to_value(attachments).ok()?))
}

async fn require_reader(pool: &PgPool, circle_id: Uuid, address: &str) -> Result<(), Response> {
    circle_membership::require_circle_permission(
        pool,
        circle_id,
        address,
        CirclePermission::ViewFinancialInfo,
    )
    .await
    .map(|_| ())
    .map_err(|e| match e {
        "not_member" => error(StatusCode::FORBIDDEN, "not_member"),
        "internal_error" => error(StatusCode::INTERNAL_SERVER_ERROR, "internal_error"),
        _ => error(StatusCode::FORBIDDEN, "permission_denied"),
    })
}

async fn require_publisher(pool: &PgPool, circle_id: Uuid, address: &str) -> Result<(), Response> {
    circle_membership::require_circle_permission(
        pool,
        circle_id,
        address,
        CirclePermission::ModerateContent,
    )
    .await
    .map(|_| ())
    .map_err(|e| match e {
        "internal_error" => error(StatusCode::INTERNAL_SERVER_ERROR, "internal_error"),
        _ => error(StatusCode::FORBIDDEN, "permission_denied"),
    })
}

fn entry_from_row(r: sqlx::postgres::PgRow) -> AnnouncementEntry {
    AnnouncementEntry {
        id: r.get("id"),
        circle_id: r.get("circle_id"),
        author_address: r.get("author_address"),
        content: r.get("content"),
        attachments: r.get("attachments"),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
    }
}

/// Notify circle members that a new announcement was published.
async fn notify_members(pool: &PgPool, circle_id: Uuid, author: &str, id: Uuid, content: &str) {
    let recipients: Vec<String> = sqlx::query_scalar(
        "SELECT address FROM circle_members WHERE circle_id = $1 AND address <> $2",
    )
    .bind(circle_id)
    .bind(author)
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    let preview: String = content.chars().take(240).collect();
    let title = "New circle announcement";
    for recipient in recipients {
        let key = format!("announcement:{id}:{recipient}");
        if let Err(e) = notifier::notify(
            pool,
            &notifier::NewCircleNotification {
                circle_id: Some(circle_id),
                recipient_address: &recipient,
                kind: CircleNotificationType::Announcement,
                title,
                body: &preview,
                metadata: serde_json::json!({ "announcement_id": id, "circle_id": circle_id }),
                dedupe_key: Some(&key),
                push_token: None,
            },
        )
        .await
        {
            tracing::warn!(error = %e, recipient = %recipient, "announcement notification failed");
        }
    }
}

/// POST /api/circles/:id/announcements
pub async fn create_announcement(
    State(state): State<CirclesState>,
    auth: AuthenticatedUser,
    Path(circle_id): Path<Uuid>,
    Json(req): Json<CreateAnnouncementRequest>,
) -> Response {
    let Some((content, attachments)) = validate_payload(&req.content, req.attachments) else {
        return error(StatusCode::BAD_REQUEST, "invalid_announcement");
    };
    if let Err(response) = require_publisher(&state.pool, circle_id, &auth.address).await {
        return response;
    }

    let row = sqlx::query(
        r#"
        INSERT INTO circle_announcements (circle_id, author_address, content, attachments)
        VALUES ($1, $2, $3, $4)
        RETURNING id, circle_id, author_address, content, attachments, created_at, updated_at
        "#,
    )
    .bind(circle_id)
    .bind(&auth.address)
    .bind(&content)
    .bind(&attachments)
    .fetch_optional(&state.pool)
    .await;

    match row {
        Ok(Some(row)) => {
            let entry = entry_from_row(row);
            notify_members(
                &state.pool,
                circle_id,
                &auth.address,
                entry.id,
                &entry.content,
            )
            .await;
            (StatusCode::CREATED, Json(serde_json::json!(entry))).into_response()
        }
        _ => error(StatusCode::INTERNAL_SERVER_ERROR, "internal_error"),
    }
}

/// GET /api/circles/:id/announcements
pub async fn list_announcements(
    State(state): State<CirclesState>,
    auth: AuthenticatedUser,
    Path(circle_id): Path<Uuid>,
    Query(query): Query<AnnouncementQuery>,
) -> Response {
    if let Err(response) = require_reader(&state.pool, circle_id, &auth.address).await {
        return response;
    }
    let page = query.page.unwrap_or(1).max(1);
    let per_page = query.per_page.unwrap_or(20).clamp(1, 100);
    let rows = sqlx::query(
        r#"
        SELECT id, circle_id, author_address, content, attachments, created_at, updated_at
          FROM circle_announcements
         WHERE circle_id = $1
         ORDER BY created_at DESC
         LIMIT $2 OFFSET $3
        "#,
    )
    .bind(circle_id)
    .bind(per_page)
    .bind((page - 1) * per_page)
    .fetch_all(&state.pool)
    .await;

    match rows {
        Ok(rows) => Json(serde_json::json!({
            "announcements": rows.into_iter().map(entry_from_row).collect::<Vec<_>>(),
            "page": page,
            "per_page": per_page,
        }))
        .into_response(),
        Err(_) => error(StatusCode::INTERNAL_SERVER_ERROR, "internal_error"),
    }
}

/// GET /api/circles/:id/announcements/:announcement_id
pub async fn get_announcement(
    State(state): State<CirclesState>,
    auth: AuthenticatedUser,
    Path((circle_id, announcement_id)): Path<(Uuid, Uuid)>,
) -> Response {
    if let Err(response) = require_reader(&state.pool, circle_id, &auth.address).await {
        return response;
    }
    let row = sqlx::query(
        r#"
        SELECT id, circle_id, author_address, content, attachments, created_at, updated_at
          FROM circle_announcements WHERE circle_id = $1 AND id = $2
        "#,
    )
    .bind(circle_id)
    .bind(announcement_id)
    .fetch_optional(&state.pool)
    .await;

    match row {
        Ok(Some(row)) => Json(serde_json::json!(entry_from_row(row))).into_response(),
        Ok(None) => error(StatusCode::NOT_FOUND, "announcement_not_found"),
        Err(_) => error(StatusCode::INTERNAL_SERVER_ERROR, "internal_error"),
    }
}

/// PUT /api/circles/:id/announcements/:announcement_id
pub async fn update_announcement(
    State(state): State<CirclesState>,
    auth: AuthenticatedUser,
    Path((circle_id, announcement_id)): Path<(Uuid, Uuid)>,
    Json(req): Json<UpdateAnnouncementRequest>,
) -> Response {
    let Some((content, attachments)) = validate_payload(&req.content, req.attachments) else {
        return error(StatusCode::BAD_REQUEST, "invalid_announcement");
    };
    if let Err(response) = require_publisher(&state.pool, circle_id, &auth.address).await {
        return response;
    }
    let result = sqlx::query(
        r#"
        UPDATE circle_announcements
           SET content = $1, attachments = $2, updated_at = NOW()
         WHERE circle_id = $3 AND id = $4
        "#,
    )
    .bind(content)
    .bind(attachments)
    .bind(circle_id)
    .bind(announcement_id)
    .execute(&state.pool)
    .await;

    match result {
        Ok(r) if r.rows_affected() > 0 => {
            Json(serde_json::json!({ "status": "updated" })).into_response()
        }
        Ok(_) => error(StatusCode::NOT_FOUND, "announcement_not_found"),
        Err(_) => error(StatusCode::INTERNAL_SERVER_ERROR, "internal_error"),
    }
}

/// DELETE /api/circles/:id/announcements/:announcement_id
pub async fn delete_announcement(
    State(state): State<CirclesState>,
    auth: AuthenticatedUser,
    Path((circle_id, announcement_id)): Path<(Uuid, Uuid)>,
) -> Response {
    if let Err(response) = require_publisher(&state.pool, circle_id, &auth.address).await {
        return response;
    }
    let result =
        sqlx::query("DELETE FROM circle_announcements WHERE circle_id = $1 AND id = $2")
            .bind(circle_id)
            .bind(announcement_id)
            .execute(&state.pool)
            .await;

    match result {
        Ok(r) if r.rows_affected() > 0 => {
            Json(serde_json::json!({ "status": "deleted" })).into_response()
        }
        Ok(_) => error(StatusCode::NOT_FOUND, "announcement_not_found"),
        Err(_) => error(StatusCode::INTERNAL_SERVER_ERROR, "internal_error"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn announcement_payload_validation() {
        let (content, attachments) =
            validate_payload("  hello  ", Some(vec!["ipfs://abc".to_string()])).unwrap();
        assert_eq!(content, "hello");
        assert_eq!(attachments[0], "ipfs://abc");
        assert!(validate_payload("  ", None).is_none());
        assert!(validate_payload("ok", Some(vec![String::new()])).is_none());
    }
}
