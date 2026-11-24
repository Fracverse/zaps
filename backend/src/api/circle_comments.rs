//! #1214 — Circle activity comments API.
//!
//! Comments belong to a circle activity and may only be written by circle
//! members. Authors may edit/remove their own comments; moderators (and above)
//! may moderate any comment in their circle. Rate limiting is provided by the
//! shared circle rate-limit middleware (the `social` bucket matches `/comment`).

use crate::api::auth_middleware::AuthenticatedUser;
use crate::api::circles::CirclesState;
use crate::services::circle_membership::{self, CirclePermission, CircleRole};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use uuid::Uuid;

const MAX_CONTENT_LEN: usize = 2_000;

#[derive(Debug, Deserialize)]
pub struct CreateCommentRequest {
    pub content: String,
    pub parent_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateCommentRequest {
    pub content: String,
}

#[derive(Debug, Deserialize)]
pub struct CommentQuery {
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct CommentEntry {
    pub id: Uuid,
    pub circle_id: Uuid,
    pub activity_id: Uuid,
    pub author_address: String,
    pub content: String,
    pub parent_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

fn error(status: StatusCode, code: &'static str) -> Response {
    (status, Json(serde_json::json!({ "error": code }))).into_response()
}

fn validate_content(content: &str) -> Option<String> {
    let trimmed = content.trim();
    if trimmed.is_empty() || trimmed.len() > MAX_CONTENT_LEN {
        None
    } else {
        Some(trimmed.to_string())
    }
}

async fn require_member(
    pool: &PgPool,
    circle_id: Uuid,
    address: &str,
) -> Result<CircleRole, Response> {
    circle_membership::require_circle_permission(
        pool,
        circle_id,
        address,
        CirclePermission::Contribute,
    )
    .await
    .map_err(|e| match e {
        "not_member" => error(StatusCode::FORBIDDEN, "not_member"),
        "internal_error" => error(StatusCode::INTERNAL_SERVER_ERROR, "internal_error"),
        _ => error(StatusCode::FORBIDDEN, "permission_denied"),
    })
}

async fn activity_in_circle(pool: &PgPool, circle_id: Uuid, activity_id: Uuid) -> bool {
    sqlx::query_scalar::<_, i32>("SELECT 1 FROM circle_activities WHERE id = $1 AND circle_id = $2")
        .bind(activity_id)
        .bind(circle_id)
        .fetch_optional(pool)
        .await
        .unwrap_or(None)
        .is_some()
}

/// POST /api/circles/:id/activities/:activity_id/comments
pub async fn create_comment(
    State(state): State<CirclesState>,
    auth: AuthenticatedUser,
    Path((circle_id, activity_id)): Path<(Uuid, Uuid)>,
    Json(req): Json<CreateCommentRequest>,
) -> Response {
    let Some(content) = validate_content(&req.content) else {
        return error(StatusCode::BAD_REQUEST, "invalid_content");
    };
    if let Err(response) = require_member(&state.pool, circle_id, &auth.address).await {
        return response;
    }
    if !activity_in_circle(&state.pool, circle_id, activity_id).await {
        return error(StatusCode::NOT_FOUND, "activity_not_found");
    }
    if let Some(parent_id) = req.parent_id {
        let parent_ok = sqlx::query_scalar::<_, i32>(
            "SELECT 1 FROM circle_comments WHERE id = $1 AND circle_id = $2 AND activity_id = $3",
        )
        .bind(parent_id)
        .bind(circle_id)
        .bind(activity_id)
        .fetch_optional(&state.pool)
        .await
        .unwrap_or(None)
        .is_some();
        if !parent_ok {
            return error(StatusCode::BAD_REQUEST, "invalid_parent");
        }
    }

    let id: Option<Uuid> = sqlx::query_as(
        r#"
        INSERT INTO circle_comments
            (circle_id, activity_id, author_address, content, parent_id)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING id
        "#,
    )
    .bind(circle_id)
    .bind(activity_id)
    .bind(&auth.address)
    .bind(&content)
    .bind(req.parent_id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    match id {
        Some(id) => (
            StatusCode::CREATED,
            Json(serde_json::json!({ "id": id, "status": "created" })),
        )
            .into_response(),
        None => error(StatusCode::INTERNAL_SERVER_ERROR, "internal_error"),
    }
}

/// GET /api/circles/:id/activities/:activity_id/comments
pub async fn list_comments(
    State(state): State<CirclesState>,
    auth: AuthenticatedUser,
    Path((circle_id, activity_id)): Path<(Uuid, Uuid)>,
    Query(query): Query<CommentQuery>,
) -> Response {
    if let Err(response) = require_member(&state.pool, circle_id, &auth.address).await {
        return response;
    }
    if !activity_in_circle(&state.pool, circle_id, activity_id).await {
        return error(StatusCode::NOT_FOUND, "activity_not_found");
    }
    let page = query.page.unwrap_or(1).max(1);
    let per_page = query.per_page.unwrap_or(20).clamp(1, 100);
    let offset = (page - 1) * per_page;

    let rows = sqlx::query(
        r#"
        SELECT id, circle_id, activity_id, author_address, content, parent_id,
               created_at, updated_at
          FROM circle_comments
         WHERE circle_id = $1 AND activity_id = $2
         ORDER BY created_at ASC
         LIMIT $3 OFFSET $4
        "#,
    )
    .bind(circle_id)
    .bind(activity_id)
    .bind(per_page)
    .bind(offset)
    .fetch_all(&state.pool)
    .await;

    match rows {
        Ok(rows) => {
            let comments: Vec<CommentEntry> = rows
                .into_iter()
                .map(|r| CommentEntry {
                    id: r.get("id"),
                    circle_id: r.get("circle_id"),
                    activity_id: r.get("activity_id"),
                    author_address: r.get("author_address"),
                    content: r.get("content"),
                    parent_id: r.get("parent_id"),
                    created_at: r.get("created_at"),
                    updated_at: r.get("updated_at"),
                })
                .collect();
            Json(serde_json::json!({
                "comments": comments,
                "page": page,
                "per_page": per_page,
            }))
            .into_response()
        }
        Err(_) => error(StatusCode::INTERNAL_SERVER_ERROR, "internal_error"),
    }
}

async fn can_modify(
    pool: &PgPool,
    circle_id: Uuid,
    comment_id: Uuid,
    auth: &AuthenticatedUser,
) -> Result<(), Response> {
    let role = require_member(pool, circle_id, &auth.address).await?;
    let author: Option<String> = sqlx::query_scalar(
        "SELECT author_address FROM circle_comments WHERE id = $1 AND circle_id = $2",
    )
    .bind(comment_id)
    .bind(circle_id)
    .fetch_optional(pool)
    .await
    .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "internal_error"))?
    .ok_or_else(|| error(StatusCode::NOT_FOUND, "comment_not_found"))?;

    let is_author = author == auth.address;
    if !is_author && !role.has_permission(CirclePermission::ModerateContent) {
        return Err(error(StatusCode::FORBIDDEN, "permission_denied"));
    }
    Ok(())
}

/// PUT /api/circles/:id/comments/:comment_id — author or moderator only.
pub async fn update_comment(
    State(state): State<CirclesState>,
    auth: AuthenticatedUser,
    Path((circle_id, comment_id)): Path<(Uuid, Uuid)>,
    Json(req): Json<UpdateCommentRequest>,
) -> Response {
    let Some(content) = validate_content(&req.content) else {
        return error(StatusCode::BAD_REQUEST, "invalid_content");
    };
    if let Err(response) = can_modify(&state.pool, circle_id, comment_id, &auth).await {
        return response;
    }
    let result = sqlx::query(
        "UPDATE circle_comments SET content = $1, updated_at = NOW() WHERE id = $2 AND circle_id = $3",
    )
    .bind(content)
    .bind(comment_id)
    .bind(circle_id)
    .execute(&state.pool)
    .await;

    match result {
        Ok(_) => Json(serde_json::json!({ "status": "updated" })).into_response(),
        Err(_) => error(StatusCode::INTERNAL_SERVER_ERROR, "internal_error"),
    }
}

/// DELETE /api/circles/:id/comments/:comment_id — author or moderator only.
pub async fn delete_comment(
    State(state): State<CirclesState>,
    auth: AuthenticatedUser,
    Path((circle_id, comment_id)): Path<(Uuid, Uuid)>,
) -> Response {
    if let Err(response) = can_modify(&state.pool, circle_id, comment_id, &auth).await {
        return response;
    }
    let result = sqlx::query("DELETE FROM circle_comments WHERE id = $1 AND circle_id = $2")
        .bind(comment_id)
        .bind(circle_id)
        .execute(&state.pool)
        .await;

    match result {
        Ok(_) => Json(serde_json::json!({ "status": "deleted" })).into_response(),
        Err(_) => error(StatusCode::INTERNAL_SERVER_ERROR, "internal_error"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_is_trimmed_and_bounded() {
        assert_eq!(validate_content("  hello  ").as_deref(), Some("hello"));
        assert_eq!(validate_content("   "), None);
        assert_eq!(validate_content(&"x".repeat(MAX_CONTENT_LEN + 1)), None);
    }
}
