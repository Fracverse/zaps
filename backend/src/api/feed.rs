use axum::{
    async_trait,
    extract::{FromRequestParts, State},
    http::{request::Parts, StatusCode},
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Serialize)]
pub struct FeedItem {
    pub id: String,
    pub tx_hash: String,
    pub sender_username: String,
    pub sender_avatar: Option<String>,
    pub receiver_username: String,
    pub receiver_avatar: Option<String>,
    pub amount: String,
    pub currency: String,
    pub memo: String,
    pub likes_count: usize,
    pub comments_count: usize,
    pub has_liked: bool,
    pub created_at: String,
}

#[derive(Deserialize)]
pub struct FeedQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Clone)]
pub struct AuthUser {
    pub id: Uuid,
    pub address: String,
    pub username: String,
}

#[async_trait]
impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
    PgPool: axum::extract::FromRef<S>,
{
    type Rejection = (StatusCode, Json<serde_json::Value>);

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let pool = PgPool::from_ref(state);

        let auth_header = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .ok_or_else(|| {
                (
                    StatusCode::UNAUTHORIZED,
                    Json(serde_json::json!({ "error": "Missing authorization header" })),
                )
            })?;

        if !auth_header.starts_with("Bearer ") {
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "error": "Invalid authorization header format" })),
            ));
        }

        let token = &auth_header["Bearer ".len()..];

        // Map token to mock user info
        let (username, address) = if token == "mock-jwt-token-string" {
            ("ebube.zaps".to_string(), "GABC1234EXAMPLESTELLARADDRESSXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX".to_string())
        } else {
            // Attempt to decode as JWT
            let secret = std::env::var("JWT_SECRET").unwrap_or_else(|_| "zaps-jwt-secret-placeholder-very-long-key".into());
            let validation = jsonwebtoken::Validation::default();
            match jsonwebtoken::decode::<crate::api::auth::Claims>(
                token,
                &jsonwebtoken::DecodingKey::from_secret(secret.as_bytes()),
                &validation,
            ) {
                Ok(token_data) => {
                    let addr = token_data.claims.sub;
                    let short_uname = format!("u_{}", &addr[1..15]);
                    (short_uname, addr)
                }
                Err(_) => {
                    let short_uname = if token.len() > 20 {
                        format!("u_{}", &token[1..15])
                    } else {
                        token.to_string()
                    };
                    (short_uname, token.to_string())
                }
            }
        };

        // Find or create the user in the database to get a valid UUID
        let row = sqlx::query(
            r#"
            INSERT INTO users (address, username, display_name)
            VALUES ($1, $2, $3)
            ON CONFLICT (address)
            DO UPDATE SET username = COALESCE(users.username, EXCLUDED.username)
            RETURNING id, address, username
            "#,
        )
        .bind(&address)
        .bind(&username)
        .bind(Some(&username))
        .fetch_one(&pool)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": format!("Database error during auth: {}", e) })),
            )
        })?;

        Ok(AuthUser {
            id: row.get("id"),
            address: row.get("address"),
            username: row.get("username"),
        })
    }
}

pub struct OptionalAuthUser(pub Option<AuthUser>);

#[async_trait]
impl<S> FromRequestParts<S> for OptionalAuthUser
where
    S: Send + Sync,
    PgPool: axum::extract::FromRef<S>,
{
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        match AuthUser::from_request_parts(parts, state).await {
            Ok(user) => Ok(OptionalAuthUser(Some(user))),
            Err(_) => Ok(OptionalAuthUser(None)),
        }
    }
}

pub async fn get_public_feed(
    State(pool): State<PgPool>,
    OptionalAuthUser(auth): OptionalAuthUser,
    axum::extract::Query(params): axum::extract::Query<FeedQuery>,
) -> impl IntoResponse {
    let limit = params.limit.unwrap_or(20);
    let offset = params.offset.unwrap_or(0);
    let user_id = auth.map(|u| u.id);

    let result = sqlx::query(
        r#"
        SELECT
            p.id,
            p.tx_hash,
            p.amount,
            p.currency,
            p.memo,
            p.created_at,
            sender.username as sender_username,
            sender.avatar_url as sender_avatar,
            receiver.username as receiver_username,
            receiver.avatar_url as receiver_avatar,
            (SELECT COUNT(*) FROM likes WHERE payment_id = p.id) as likes_count,
            (SELECT COUNT(*) FROM comments WHERE payment_id = p.id) as comments_count,
            CASE 
                WHEN $1::uuid IS NOT NULL THEN EXISTS(SELECT 1 FROM likes WHERE payment_id = p.id AND user_id = $1::uuid)
                ELSE FALSE
            END as has_liked
        FROM payments p
        JOIN users sender ON p.sender_id = sender.id
        JOIN users receiver ON p.receiver_id = receiver.id
        WHERE p.visibility = 'PUBLIC'
        ORDER BY p.created_at DESC
        LIMIT $2 OFFSET $3
        "#,
    )
    .bind(user_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(&pool)
    .await;

    match result {
        Ok(rows) => {
            let feed: Vec<FeedItem> = rows
                .into_iter()
                .map(|row| {
                    let created_at: chrono::NaiveDateTime = row.get("created_at");
                    let amount: i64 = row.get("amount");
                    let likes_count: i64 = row.get("likes_count");
                    let comments_count: i64 = row.get("comments_count");
                    FeedItem {
                        id: row.get::<uuid::Uuid, _>("id").to_string(),
                        tx_hash: row.get("tx_hash"),
                        sender_username: row.get("sender_username"),
                        sender_avatar: row.get("sender_avatar"),
                        receiver_username: row.get("receiver_username"),
                        receiver_avatar: row.get("receiver_avatar"),
                        amount: format!("{:.2}", amount as f64 / 100.0),
                        currency: row.get("currency"),
                        memo: row.get("memo"),
                        likes_count: likes_count as usize,
                        comments_count: comments_count as usize,
                        has_liked: row.get("has_liked"),
                        created_at: created_at.and_utc().to_rfc3339(),
                    }
                })
                .collect();
            Json(feed).into_response()
        }
        Err(e) => {
            tracing::error!("Failed to fetch public feed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Internal database error" })),
            )
                .into_response()
        }
    }
}

pub async fn get_friends_feed(
    State(pool): State<PgPool>,
    auth: AuthUser,
    axum::extract::Query(params): axum::extract::Query<FeedQuery>,
) -> impl IntoResponse {
    let limit = params.limit.unwrap_or(20);
    let offset = params.offset.unwrap_or(0);

    let result = sqlx::query(
        r#"
        SELECT DISTINCT
            p.id,
            p.tx_hash,
            p.amount,
            p.currency,
            p.memo,
            p.created_at,
            sender.username as sender_username,
            sender.avatar_url as sender_avatar,
            receiver.username as receiver_username,
            receiver.avatar_url as receiver_avatar,
            (SELECT COUNT(*) FROM likes WHERE payment_id = p.id) as likes_count,
            (SELECT COUNT(*) FROM comments WHERE payment_id = p.id) as comments_count,
            EXISTS(SELECT 1 FROM likes WHERE payment_id = p.id AND user_id = $1) as has_liked
        FROM payments p
        JOIN users sender ON p.sender_id = sender.id
        JOIN users receiver ON p.receiver_id = receiver.id
        LEFT JOIN friendships f ON (
            f.status = 'ACCEPTED' AND (
                (f.user_id = $1 AND f.friend_id = p.sender_id) OR
                (f.friend_id = $1 AND f.user_id = p.sender_id) OR
                (f.user_id = $1 AND f.friend_id = p.receiver_id) OR
                (f.friend_id = $1 AND f.user_id = p.receiver_id)
            )
        )
        WHERE (p.sender_id = $1 OR p.receiver_id = $1 OR f.id IS NOT NULL)
          AND (p.visibility = 'PUBLIC' OR p.visibility = 'FRIENDS')
        ORDER BY p.created_at DESC
        LIMIT $2 OFFSET $3
        "#,
    )
    .bind(auth.id)
    .bind(limit)
    .bind(offset)
    .fetch_all(&pool)
    .await;

    match result {
        Ok(rows) => {
            let feed: Vec<FeedItem> = rows
                .into_iter()
                .map(|row| {
                    let created_at: chrono::NaiveDateTime = row.get("created_at");
                    let amount: i64 = row.get("amount");
                    let likes_count: i64 = row.get("likes_count");
                    let comments_count: i64 = row.get("comments_count");
                    FeedItem {
                        id: row.get::<uuid::Uuid, _>("id").to_string(),
                        tx_hash: row.get("tx_hash"),
                        sender_username: row.get("sender_username"),
                        sender_avatar: row.get("sender_avatar"),
                        receiver_username: row.get("receiver_username"),
                        receiver_avatar: row.get("receiver_avatar"),
                        amount: format!("{:.2}", amount as f64 / 100.0),
                        currency: row.get("currency"),
                        memo: row.get("memo"),
                        likes_count: likes_count as usize,
                        comments_count: comments_count as usize,
                        has_liked: row.get("has_liked"),
                        created_at: created_at.and_utc().to_rfc3339(),
                    }
                })
                .collect();
            Json(feed).into_response()
        }
        Err(e) => {
            tracing::error!("Failed to fetch friends feed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Internal database error" })),
            )
                .into_response()
        }
    }
}

pub async fn get_private_feed(
    State(pool): State<PgPool>,
    auth: AuthUser,
    axum::extract::Query(params): axum::extract::Query<FeedQuery>,
) -> impl IntoResponse {
    let limit = params.limit.unwrap_or(20);
    let offset = params.offset.unwrap_or(0);

    let result = sqlx::query(
        r#"
        SELECT
            p.id,
            p.tx_hash,
            p.amount,
            p.currency,
            p.memo,
            p.created_at,
            sender.username as sender_username,
            sender.avatar_url as sender_avatar,
            receiver.username as receiver_username,
            receiver.avatar_url as receiver_avatar,
            (SELECT COUNT(*) FROM likes WHERE payment_id = p.id) as likes_count,
            (SELECT COUNT(*) FROM comments WHERE payment_id = p.id) as comments_count,
            EXISTS(SELECT 1 FROM likes WHERE payment_id = p.id AND user_id = $1) as has_liked
        FROM payments p
        JOIN users sender ON p.sender_id = sender.id
        JOIN users receiver ON p.receiver_id = receiver.id
        WHERE p.visibility = 'PRIVATE'
          AND (p.sender_id = $1 OR p.receiver_id = $1)
        ORDER BY p.created_at DESC
        LIMIT $2 OFFSET $3
        "#,
    )
    .bind(auth.id)
    .bind(limit)
    .bind(offset)
    .fetch_all(&pool)
    .await;

    match result {
        Ok(rows) => {
            let feed: Vec<FeedItem> = rows
                .into_iter()
                .map(|row| {
                    let created_at: chrono::NaiveDateTime = row.get("created_at");
                    let amount: i64 = row.get("amount");
                    let likes_count: i64 = row.get("likes_count");
                    let comments_count: i64 = row.get("comments_count");
                    FeedItem {
                        id: row.get::<uuid::Uuid, _>("id").to_string(),
                        tx_hash: row.get("tx_hash"),
                        sender_username: row.get("sender_username"),
                        sender_avatar: row.get("sender_avatar"),
                        receiver_username: row.get("receiver_username"),
                        receiver_avatar: row.get("receiver_avatar"),
                        amount: format!("{:.2}", amount as f64 / 100.0),
                        currency: row.get("currency"),
                        memo: row.get("memo"),
                        likes_count: likes_count as usize,
                        comments_count: comments_count as usize,
                        has_liked: row.get("has_liked"),
                        created_at: created_at.and_utc().to_rfc3339(),
                    }
                })
                .collect();
            Json(feed).into_response()
        }
        Err(e) => {
            tracing::error!("Failed to fetch private feed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Internal database error" })),
            )
                .into_response()
        }
    }
}
