use axum::{response::IntoResponse, Json, extract::State};
use serde::{Deserialize, Serialize};
use sqlx::Row;

#[derive(Deserialize)]
pub struct UpdateProfileRequest {
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
}

#[derive(Serialize)]
pub struct ProfileResponse {
    pub address: String,
    pub username: String,
    pub display_name: Option<String>,
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

pub async fn get_profile() -> impl IntoResponse {
    // TODO: Implement BE-004 (Get User Profile details)
    Json(ProfileResponse {
        address: "GABC1234EXAMPLESTELLARADDRESSXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX".to_string(),
        username: "ebube.zaps".to_string(),
        display_name: Some("Ebube One".to_string()),
        avatar_url: None,
    })
}

pub async fn update_profile(Json(_payload): Json<UpdateProfileRequest>) -> impl IntoResponse {
    // TODO: Implement BE-004 (Update avatar, display name)
    Json(ProfileResponse {
        address: "GABC1234EXAMPLESTELLARADDRESSXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX".to_string(),
        username: "ebube.zaps".to_string(),
        display_name: Some("Ebube Updated".to_string()),
        avatar_url: Some("https://example.com/avatar.png".to_string()),
    })
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
