use crate::api::feed::AuthUser;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use sqlx::Row;

#[derive(Deserialize)]
pub struct LikeRequest {
    pub payment_id: String,
}

#[derive(Deserialize)]
pub struct CommentRequest {
    pub payment_id: String,
    pub content: String,
}

#[derive(Serialize)]
pub struct CommentResponse {
    pub id: String,
    pub username: String,
    pub content: String,
    pub created_at: String,
}

pub async fn like_payment(Json(_payload): Json<LikeRequest>) -> impl IntoResponse {
    // TODO: Implement BE-009 (Like payment action)
    Json(serde_json::json!({ "success": true }))
}

pub async fn unlike_payment(Json(_payload): Json<LikeRequest>) -> impl IntoResponse {
    // TODO: Implement BE-009 (Unlike payment action)
    Json(serde_json::json!({ "success": true }))
}

pub async fn add_comment(
    State(pool): State<sqlx::PgPool>,
    auth: AuthUser,
    Json(payload): Json<CommentRequest>,
) -> impl IntoResponse {
    // Parse payment_id as UUID
    let payment_id = match uuid::Uuid::parse_str(&payload.payment_id) {
        Ok(id) => id,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "Invalid payment ID" })),
            )
                .into_response();
        }
    };

    let result = sqlx::query(
        r#"
        INSERT INTO comments (payment_id, user_id, content)
        VALUES ($1, $2, $3)
        RETURNING id, created_at
        "#,
    )
    .bind(payment_id)
    .bind(auth.id)
    .bind(&payload.content)
    .fetch_one(&pool)
    .await;

    match result {
        Ok(row) => {
            let created_at: chrono::NaiveDateTime = row.get("created_at");
            Json(CommentResponse {
                id: row.get::<uuid::Uuid, _>("id").to_string(),
                username: auth.username,
                content: payload.content,
                created_at: created_at.and_utc().to_rfc3339(),
            })
            .into_response()
        }
        Err(e) => {
            tracing::error!("Failed to add comment: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to add comment" })),
            )
                .into_response()
        }
    }
}

pub async fn delete_comment(
    State(pool): State<sqlx::PgPool>,
    auth: AuthUser,
    Path(comment_id): Path<String>,
) -> impl IntoResponse {
    // Parse comment_id as UUID
    let comment_uuid = match uuid::Uuid::parse_str(&comment_id) {
        Ok(id) => id,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "Invalid comment ID" })),
            )
                .into_response();
        }
    };

    let result = sqlx::query(
        r#"
        DELETE FROM comments
        WHERE id = $1 AND user_id = $2
        "#,
    )
    .bind(comment_uuid)
    .bind(auth.id)
    .execute(&pool)
    .await;

    match result {
        Ok(query_result) => {
            if query_result.rows_affected() == 0 {
                (
                    StatusCode::NOT_FOUND,
                    Json(serde_json::json!({ "error": "Comment not found or not authorized" })),
                )
                    .into_response()
            } else {
                Json(serde_json::json!({ "success": true })).into_response()
            }
        }
        Err(e) => {
            tracing::error!("Failed to delete comment: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to delete comment" })),
            )
                .into_response()
        }
    }
}
