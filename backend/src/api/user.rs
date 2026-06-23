use axum::{extract::State, response::IntoResponse, Json};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use crate::api::feed::AuthUser;

#[derive(Deserialize)]
pub struct UpdateProfileRequest {
    pub display_name: Option<String>,
    pub bio: Option<String>,
    pub avatar_url: Option<String>,
}

#[derive(Serialize)]
pub struct ProfileResponse {
    pub address: String,
    pub username: String,
    pub display_name: Option<String>,
    pub bio: Option<String>,
    pub avatar_url: Option<String>,
}

#[derive(Deserialize)]
pub struct SearchQuery {
    pub q: String,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Serialize)]
pub struct UserSearchItem {
    pub username: String,
    pub address: String,
    pub avatar_url: Option<String>,
}

#[derive(Deserialize)]
pub struct FriendRequest {
    pub friend_address: String,
}

pub async fn get_profile(
    State(pool): State<sqlx::PgPool>,
    auth: AuthUser,
) -> impl IntoResponse {
    let row = match sqlx::query(
        r#"
        SELECT address, username, display_name, bio, avatar_url
        FROM users
        WHERE id = $1
        "#,
    )
    .bind(auth.id)
    .fetch_one(&pool)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("Database query error in get_profile: {:?}", e);
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Internal database error" })),
            )
                .into_response();
        }
    };

    Json(ProfileResponse {
        address: row.get("address"),
        username: row.get("username"),
        display_name: row.get("display_name"),
        bio: row.get("bio"),
        avatar_url: row.get("avatar_url"),
    })
    .into_response()
}

pub async fn update_profile(
    State(pool): State<sqlx::PgPool>,
    auth: AuthUser,
    Json(payload): Json<UpdateProfileRequest>,
) -> impl IntoResponse {
    let row = match sqlx::query(
        r#"
        UPDATE users
        SET display_name = COALESCE($1, display_name),
            bio = COALESCE($2, bio),
            avatar_url = COALESCE($3, avatar_url)
        WHERE id = $4
        RETURNING address, username, display_name, bio, avatar_url
        "#,
    )
    .bind(payload.display_name)
    .bind(payload.bio)
    .bind(payload.avatar_url)
    .bind(auth.id)
    .fetch_one(&pool)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("Database update error in update_profile: {:?}", e);
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to update profile" })),
            )
                .into_response();
        }
    };

    Json(ProfileResponse {
        address: row.get("address"),
        username: row.get("username"),
        display_name: row.get("display_name"),
        bio: row.get("bio"),
        avatar_url: row.get("avatar_url"),
    })
    .into_response()
}

pub async fn search_users(
    State(pool): State<sqlx::PgPool>,
    axum::extract::Query(params): axum::extract::Query<SearchQuery>,
) -> impl IntoResponse {
    let limit = params.limit.unwrap_or(20);
    let offset = params.offset.unwrap_or(0);
    let query_pattern = format!("{}%", params.q);

    let rows = match sqlx::query(
        r#"
        SELECT username, address, avatar_url
        FROM users
        WHERE username LIKE $1 OR address LIKE $1
        ORDER BY username ASC
        LIMIT $2 OFFSET $3
        "#,
    )
    .bind(&query_pattern)
    .bind(limit)
    .bind(offset)
    .fetch_all(&pool)
    .await
    {
        Ok(rows) => rows,
        Err(e) => {
            tracing::error!("Search users query failed: {:?}", e);
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Internal database error" })),
            )
                .into_response();
        }
    };

    let users: Vec<UserSearchItem> = rows
        .into_iter()
        .map(|row| UserSearchItem {
            username: row.get("username"),
            address: row.get("address"),
            avatar_url: row.get("avatar_url"),
        })
        .collect();

    Json(users).into_response()
}

pub async fn list_friends() -> impl IntoResponse {
    // TODO: Implement BE-012 (Friend list retrieval endpoint)
    let mock_friends: Vec<UserSearchItem> = vec![];
    Json(mock_friends)
}

pub async fn send_friend_request(Json(_payload): Json<FriendRequest>) -> impl IntoResponse {
    // TODO: Implement BE-011 (Send friend request endpoint)
    Json(serde_json::json!({ "status": "pending" }))
}
