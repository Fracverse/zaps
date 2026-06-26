use crate::api::feed::AuthUser;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

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

pub async fn get_profile(State(pool): State<sqlx::PgPool>, auth: AuthUser) -> impl IntoResponse {
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
                StatusCode::INTERNAL_SERVER_ERROR,
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
                StatusCode::INTERNAL_SERVER_ERROR,
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
                StatusCode::INTERNAL_SERVER_ERROR,
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

pub async fn list_friends(State(pool): State<sqlx::PgPool>, auth: AuthUser) -> impl IntoResponse {
    let rows = match sqlx::query(
        r#"
        SELECT u.username, u.address, u.avatar_url
        FROM users u
        JOIN friendships f ON (
            (f.user_id = $1 AND f.friend_id = u.id) OR
            (f.friend_id = $1 AND f.user_id = u.id)
        )
        WHERE f.status = 'ACCEPTED' AND u.id != $1
        "#,
    )
    .bind(auth.id)
    .fetch_all(&pool)
    .await
    {
        Ok(rows) => rows,
        Err(e) => {
            tracing::error!("Failed to fetch friends: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Internal database error" })),
            )
                .into_response();
        }
    };

    let friends: Vec<UserSearchItem> = rows
        .into_iter()
        .map(|row| UserSearchItem {
            username: row.get("username"),
            address: row.get("address"),
            avatar_url: row.get("avatar_url"),
        })
        .collect();

    Json(friends).into_response()
}

/// POST /api/users/friends/request
/// Sends a friend request from the authenticated user to `friend_address`.
/// Returns 409 if a friendship record already exists in either direction.
pub async fn send_friend_request(
    State(pool): State<sqlx::PgPool>,
    auth: AuthUser,
    Json(payload): Json<FriendRequest>,
) -> impl IntoResponse {
    // Resolve the target user's id from their address.
    let friend_id: Uuid = match sqlx::query_scalar("SELECT id FROM users WHERE address = $1")
        .bind(&payload.friend_address)
        .fetch_optional(&pool)
        .await
    {
        Ok(Some(id)) => id,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "User not found" })),
            )
                .into_response()
        }
        Err(e) => {
            tracing::error!("send_friend_request lookup failed: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Internal database error" })),
            )
                .into_response();
        }
    };

    if friend_id == auth.id {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "Cannot send a friend request to yourself" })),
        )
            .into_response();
    }

    // Guard: reject if any friendship record already exists in either direction.
    let exists: bool = match sqlx::query_scalar(
        r#"
        SELECT EXISTS (
            SELECT 1 FROM friendships
            WHERE (user_id = $1 AND friend_id = $2)
               OR (user_id = $2 AND friend_id = $1)
        )
        "#,
    )
    .bind(auth.id)
    .bind(friend_id)
    .fetch_one(&pool)
    .await
    {
        Ok(v) => v,
        Err(e) => {
            tracing::error!("send_friend_request existence check failed: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Internal database error" })),
            )
                .into_response();
        }
    };

    if exists {
        return (
            StatusCode::CONFLICT,
            Json(serde_json::json!({ "error": "Friend request already exists" })),
        )
            .into_response();
    }

    match sqlx::query(
        "INSERT INTO friendships (user_id, friend_id, status) VALUES ($1, $2, 'PENDING')",
    )
    .bind(auth.id)
    .bind(friend_id)
    .execute(&pool)
    .await
    {
        Ok(_) => (
            StatusCode::CREATED,
            Json(serde_json::json!({ "status": "PENDING" })),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("send_friend_request insert failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to send friend request" })),
            )
                .into_response()
        }
    }
}

/// POST /api/users/friends/:id/accept
/// Accepts an incoming PENDING friend request where the authenticated user is
/// the recipient (friend_id).
pub async fn accept_friend_request(
    State(pool): State<sqlx::PgPool>,
    auth: AuthUser,
    Path(friendship_id): Path<Uuid>,
) -> impl IntoResponse {
    match sqlx::query(
        r#"
        UPDATE friendships
        SET status = 'ACCEPTED'
        WHERE id = $1 AND friend_id = $2 AND status = 'PENDING'
        "#,
    )
    .bind(friendship_id)
    .bind(auth.id)
    .execute(&pool)
    .await
    {
        Ok(result) if result.rows_affected() == 0 => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Pending friend request not found" })),
        )
            .into_response(),
        Ok(_) => Json(serde_json::json!({ "status": "ACCEPTED" })).into_response(),
        Err(e) => {
            tracing::error!("accept_friend_request failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Internal database error" })),
            )
                .into_response()
        }
    }
}

/// POST /api/users/friends/:id/reject
/// Rejects an incoming PENDING friend request where the authenticated user is
/// the recipient (friend_id).
pub async fn reject_friend_request(
    State(pool): State<sqlx::PgPool>,
    auth: AuthUser,
    Path(friendship_id): Path<Uuid>,
) -> impl IntoResponse {
    match sqlx::query(
        r#"
        UPDATE friendships
        SET status = 'REJECTED'
        WHERE id = $1 AND friend_id = $2 AND status = 'PENDING'
        "#,
    )
    .bind(friendship_id)
    .bind(auth.id)
    .execute(&pool)
    .await
    {
        Ok(result) if result.rows_affected() == 0 => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Pending friend request not found" })),
        )
            .into_response(),
        Ok(_) => Json(serde_json::json!({ "status": "REJECTED" })).into_response(),
        Err(e) => {
            tracing::error!("reject_friend_request failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Internal database error" })),
            )
                .into_response()
        }
    }
}
