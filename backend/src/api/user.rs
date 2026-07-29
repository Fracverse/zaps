use crate::api::feed::AuthUser;
use crate::services::redis_cache::UsernameAddressCache;
use axum::{
    extract::{FromRef, Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

/// #544: state for routes that need the username->address Redis cache
/// alongside the DB pool. Existing handlers keep extracting `State<PgPool>`
/// directly (via the `FromRef` impl below) — only `resolve_address` needs
/// the full state.
#[derive(Clone)]
pub struct UserState {
    pub pool: sqlx::PgPool,
    pub cache: Option<UsernameAddressCache>,
}

impl UserState {
    pub fn new(pool: sqlx::PgPool, cache: Option<UsernameAddressCache>) -> Self {
        Self { pool, cache }
    }
}

impl FromRef<UserState> for sqlx::PgPool {
    fn from_ref(state: &UserState) -> Self {
        state.pool.clone()
    }
}

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

#[derive(Serialize)]
pub struct UserSuggestionsResponse {
    pub query: String,
    pub results: Vec<UserSearchItem>,
    pub limit: i64,
    pub offset: i64,
    pub total: i64,
    pub has_more: bool,
}

#[derive(Deserialize)]
pub struct FriendRequest {
    pub friend_address: String,
}

// ── #545: username syntax validation ───────────────────────────────────────

/// Shortest registerable username, in characters.
pub const USERNAME_MIN_LEN: usize = 3;
/// Longest registerable username, in characters.
pub const USERNAME_MAX_LEN: usize = 15;

/// Why a candidate username (or username prefix) was rejected.
#[derive(Debug, PartialEq, Eq)]
pub enum UsernameError {
    Empty,
    TooShort,
    TooLong,
    InvalidCharacters,
}

impl UsernameError {
    /// Client-facing reason, safe to return in an error body.
    pub fn message(&self) -> String {
        match self {
            UsernameError::Empty => "username must not be empty".to_string(),
            UsernameError::TooShort => {
                format!("username must be at least {USERNAME_MIN_LEN} characters")
            }
            UsernameError::TooLong => {
                format!("username must be at most {USERNAME_MAX_LEN} characters")
            }
            UsernameError::InvalidCharacters => {
                "username may only contain lowercase letters and digits".to_string()
            }
        }
    }
}

/// Validates a complete username against the registry's syntax rules:
/// lowercase ASCII alphanumeric, 3-15 characters.
pub fn validate_username(username: &str) -> Result<(), UsernameError> {
    let len = username.chars().count();
    if len < USERNAME_MIN_LEN {
        return Err(UsernameError::TooShort);
    }
    if len > USERNAME_MAX_LEN {
        return Err(UsernameError::TooLong);
    }
    validate_username_charset(username)
}

/// Validates a *partial* username as typed into an autocomplete box. Same
/// charset as a full username but no minimum length, since a suggestion query
/// is by definition shorter than the name it matches.
pub fn validate_username_prefix(prefix: &str) -> Result<(), UsernameError> {
    let len = prefix.chars().count();
    if len == 0 {
        return Err(UsernameError::Empty);
    }
    if len > USERNAME_MAX_LEN {
        return Err(UsernameError::TooLong);
    }
    validate_username_charset(prefix)
}

fn validate_username_charset(value: &str) -> Result<(), UsernameError> {
    if value
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
    {
        Ok(())
    } else {
        Err(UsernameError::InvalidCharacters)
    }
}

/// Escapes the LIKE metacharacters so a caller-supplied term is matched
/// literally. Pair with `ESCAPE '\'` in the query — without this, a `q` of `%`
/// matches every row in the table.
fn escape_like_pattern(term: &str) -> String {
    let mut escaped = String::with_capacity(term.len());
    for c in term.chars() {
        if matches!(c, '\\' | '%' | '_') {
            escaped.push('\\');
        }
        escaped.push(c);
    }
    escaped
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

/// Longest accepted `q` on /search. Sized for a Stellar address (56 chars),
/// which the endpoint also matches against.
const SEARCH_TERM_MAX_LEN: usize = 64;

/// GET /api/users/search?q=&limit=&offset=
///
/// Prefix-matches `q` against registered usernames (case-insensitively, via the
/// `LOWER(username)` index from #541) and against Stellar addresses. Returns a
/// bare array for backwards compatibility with the dashboard and mobile app;
/// `/api/users/suggestions` is the paginated form.
pub async fn search_users(
    State(pool): State<sqlx::PgPool>,
    axum::extract::Query(params): axum::extract::Query<SearchQuery>,
) -> impl IntoResponse {
    let term = params.q.trim();
    if term.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "q must not be empty" })),
        )
            .into_response();
    }
    if term.chars().count() > SEARCH_TERM_MAX_LEN {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": format!("q must be at most {SEARCH_TERM_MAX_LEN} characters")
            })),
        )
            .into_response();
    }

    let limit = params.limit.unwrap_or(20).clamp(1, 50);
    let offset = params.offset.unwrap_or(0).max(0);
    let escaped = escape_like_pattern(term);
    let username_pattern = format!("{}%", escaped.to_lowercase());
    let address_pattern = format!("{escaped}%");

    let rows = match sqlx::query(
        r#"
        SELECT username, address, avatar_url
        FROM users
        WHERE LOWER(username) LIKE $1 ESCAPE '\'
           OR address LIKE $2 ESCAPE '\'
        ORDER BY username ASC
        LIMIT $3 OFFSET $4
        "#,
    )
    .bind(&username_pattern)
    .bind(&address_pattern)
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

/// GET /api/users/suggestions?q=&limit=&offset=
///
/// Username-only autocomplete. Unlike /search this validates `q` as a username
/// prefix (#545) and returns a paginated envelope carrying the total match
/// count alongside each profile's avatar.
pub async fn suggest_usernames(
    State(pool): State<sqlx::PgPool>,
    axum::extract::Query(params): axum::extract::Query<SearchQuery>,
) -> impl IntoResponse {
    let term = params.q.trim().to_lowercase();
    if let Err(e) = validate_username_prefix(&term) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": e.message() })),
        )
            .into_response();
    }

    let limit = params.limit.unwrap_or(10).clamp(1, 25);
    let offset = params.offset.unwrap_or(0).max(0);
    // `term` is already known to be lowercase alphanumeric, so it carries no
    // LIKE metacharacters — but escape anyway so the two endpoints cannot drift.
    let pattern = format!("{}%", escape_like_pattern(&term));

    let total: i64 = match sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM users WHERE LOWER(username) LIKE $1 ESCAPE '\'"#,
    )
    .bind(&pattern)
    .fetch_one(&pool)
    .await
    {
        Ok(count) => count,
        Err(e) => {
            tracing::error!("Username suggestion count failed: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Internal database error" })),
            )
                .into_response();
        }
    };

    let rows = match sqlx::query(
        r#"
        SELECT username, address, avatar_url
        FROM users
        WHERE LOWER(username) LIKE $1 ESCAPE '\'
        ORDER BY LOWER(username) ASC
        LIMIT $2 OFFSET $3
        "#,
    )
    .bind(&pattern)
    .bind(limit)
    .bind(offset)
    .fetch_all(&pool)
    .await
    {
        Ok(rows) => rows,
        Err(e) => {
            tracing::error!("Username suggestion query failed: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Internal database error" })),
            )
                .into_response();
        }
    };

    let results: Vec<UserSearchItem> = rows
        .into_iter()
        .map(|row| UserSearchItem {
            username: row.get("username"),
            address: row.get("address"),
            avatar_url: row.get("avatar_url"),
        })
        .collect();

    let has_more = offset + (results.len() as i64) < total;

    Json(UserSuggestionsResponse {
        query: term,
        results,
        limit,
        offset,
        total,
        has_more,
    })
    .into_response()
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

// ── #544: cached username -> address resolution ────────────────────────────

#[derive(Serialize)]
pub struct ResolveAddressResponse {
    pub username: String,
    pub address: String,
}

/// GET /api/users/resolve/:username — resolve a username to its registered
/// Stellar address for transfer/payout flows, checking the Redis cache
/// before Postgres and populating it (30-minute TTL) on a DB hit.
pub async fn resolve_address(
    State(state): State<UserState>,
    Path(username): Path<String>,
) -> impl IntoResponse {
    // #545: reject syntactically impossible usernames before touching Redis or
    // Postgres — they can never resolve.
    if let Err(e) = validate_username(&username) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": e.message() })),
        )
            .into_response();
    }

    if let Some(cache) = &state.cache {
        if let Some(address) = cache.get_address(&username).await {
            return Json(ResolveAddressResponse { username, address }).into_response();
        }
    }

    let row = sqlx::query("SELECT address FROM users WHERE username = $1")
        .bind(&username)
        .fetch_optional(&state.pool)
        .await;

    match row {
        Ok(Some(row)) => {
            let address: String = row.get("address");
            if let Some(cache) = &state.cache {
                cache.set_address(&username, &address).await;
            }
            Json(ResolveAddressResponse { username, address }).into_response()
        }
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "username not found" })),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("Failed to resolve username: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Internal database error" })),
            )
                .into_response()
        }
    }
}

// ── Registry endpoints for administrative dashboard ─────────────────────────

#[derive(Serialize)]
pub struct RegistryClaimResponse {
    pub username: String,
    pub public_key: String,
    pub registered_at: String,
    pub tx_hash: Option<String>,
}

#[derive(Serialize)]
pub struct RegistryStatsResponse {
    pub total_usernames: i64,
    pub weekly_growth: i64,
    pub active_registrations: i64,
}

/// GET /api/registry/claims - fetch all registered usernames
pub async fn get_registry_claims(
    State(pool): State<sqlx::PgPool>,
    _auth: AuthUser,
) -> impl IntoResponse {
    let rows = match sqlx::query(
        r#"
        SELECT username, address as public_key, created_at as registered_at
        FROM users
        ORDER BY created_at DESC
        LIMIT 1000
        "#,
    )
    .fetch_all(&pool)
    .await
    {
        Ok(rows) => rows,
        Err(e) => {
            tracing::error!("Failed to fetch registry claims: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Internal database error" })),
            )
                .into_response();
        }
    };

    let claims: Vec<RegistryClaimResponse> = rows
        .into_iter()
        .map(|row| {
            let registered_at: chrono::NaiveDateTime = row.get("registered_at");
            let registered_at_str = registered_at.and_utc().to_rfc3339();
            RegistryClaimResponse {
                username: row.get("username"),
                public_key: row.get("public_key"),
                registered_at: registered_at_str,
                tx_hash: None,
            }
        })
        .collect();

    Json(claims).into_response()
}

/// GET /api/registry/stats - get username registry metrics
pub async fn get_registry_stats(
    State(pool): State<sqlx::PgPool>,
    _auth: AuthUser,
) -> impl IntoResponse {
    let total_usernames: i64 = match sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM users")
        .fetch_one(&pool)
        .await
    {
        Ok(count) => count,
        Err(e) => {
            tracing::error!("Failed to count total usernames: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Internal database error" })),
            )
                .into_response();
        }
    };

    let weekly_growth: i64 = match sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM users WHERE created_at >= NOW() - INTERVAL '7 days'",
    )
    .fetch_one(&pool)
    .await
    {
        Ok(count) => count,
        Err(e) => {
            tracing::error!("Failed to calculate weekly growth: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Internal database error" })),
            )
                .into_response();
        }
    };

    Json(RegistryStatsResponse {
        total_usernames,
        weekly_growth,
        active_registrations: total_usernames,
    })
    .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_lowercase_alphanumeric_usernames() {
        assert_eq!(validate_username("ebube"), Ok(()));
        assert_eq!(validate_username("zap"), Ok(()));
        assert_eq!(validate_username("user123"), Ok(()));
        assert_eq!(validate_username("123456789012345"), Ok(()));
    }

    #[test]
    fn rejects_usernames_outside_length_bounds() {
        assert_eq!(validate_username(""), Err(UsernameError::TooShort));
        assert_eq!(validate_username("ab"), Err(UsernameError::TooShort));
        assert_eq!(
            validate_username("1234567890123456"),
            Err(UsernameError::TooLong)
        );
    }

    #[test]
    fn rejects_uppercase_usernames() {
        assert_eq!(
            validate_username("Ebube"),
            Err(UsernameError::InvalidCharacters)
        );
        assert_eq!(
            validate_username("EBUBE"),
            Err(UsernameError::InvalidCharacters)
        );
    }

    #[test]
    fn rejects_symbols_and_whitespace() {
        for candidate in ["e-bube", "e_bube", "e.bube", "e bube", "ebube!", "ébube"] {
            assert_eq!(
                validate_username(candidate),
                Err(UsernameError::InvalidCharacters),
                "expected {candidate:?} to be rejected"
            );
        }
    }

    #[test]
    fn prefix_validation_allows_short_terms_but_not_empty_or_invalid() {
        assert_eq!(validate_username_prefix("e"), Ok(()));
        assert_eq!(validate_username_prefix("eb"), Ok(()));
        assert_eq!(validate_username_prefix(""), Err(UsernameError::Empty));
        assert_eq!(
            validate_username_prefix("1234567890123456"),
            Err(UsernameError::TooLong)
        );
        assert_eq!(
            validate_username_prefix("Eb"),
            Err(UsernameError::InvalidCharacters)
        );
    }

    #[test]
    fn like_metacharacters_are_escaped() {
        assert_eq!(escape_like_pattern("100%"), r"100\%");
        assert_eq!(escape_like_pattern("a_b"), r"a\_b");
        assert_eq!(escape_like_pattern(r"a\b"), r"a\\b");
        assert_eq!(escape_like_pattern("ebube"), "ebube");
    }
}
