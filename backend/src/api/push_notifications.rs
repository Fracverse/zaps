//! #1218 — Push notification device registration and preferences.
//!
//! Device tokens are always associated with the authenticated user id from the
//! bearer token. Preferences control whether circle events are pushed and
//! whether sensitive financial details may appear in lock-screen messages.

use crate::api::auth_middleware::AuthenticatedUser;
use crate::services::notifications::upsert_push_token;
use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

#[derive(Clone)]
pub struct PushState {
    pub pool: PgPool,
}

impl PushState {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(Debug, Deserialize)]
pub struct RegisterDeviceRequest {
    pub token: String,
    pub platform: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UnregisterDeviceRequest {
    pub token: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NotificationPreferences {
    pub circle_push_enabled: bool,
    pub push_financial_details: bool,
}

fn error(status: StatusCode, code: &'static str) -> Response {
    (status, Json(serde_json::json!({ "error": code }))).into_response()
}

fn valid_token(token: &str) -> bool {
    let token = token.trim();
    token.len() <= 512
        && (token.starts_with("ExponentPushToken[") || token.starts_with("["))
        && token.ends_with(']')
}

/// POST /api/notifications/device-tokens
pub async fn register_device(
    State(state): State<PushState>,
    auth: AuthenticatedUser,
    Json(req): Json<RegisterDeviceRequest>,
) -> Response {
    if !valid_token(&req.token) {
        return error(StatusCode::BAD_REQUEST, "invalid_device_token");
    }
    let platform = req
        .platform
        .as_deref()
        .map(str::trim)
        .filter(|p| !p.is_empty());
    if platform.is_some_and(|p| p.len() > 20) {
        return error(StatusCode::BAD_REQUEST, "invalid_platform");
    }
    match upsert_push_token(&state.pool, auth.id, req.token.trim(), platform).await {
        Ok(()) => Json(serde_json::json!({ "status": "registered" })).into_response(),
        Err(_) => error(StatusCode::INTERNAL_SERVER_ERROR, "internal_error"),
    }
}

/// DELETE /api/notifications/device-tokens
pub async fn unregister_device(
    State(state): State<PushState>,
    auth: AuthenticatedUser,
    Json(req): Json<UnregisterDeviceRequest>,
) -> Response {
    let result =
        sqlx::query("DELETE FROM user_push_tokens WHERE user_id = $1 AND expo_push_token = $2")
            .bind(auth.id)
            .bind(req.token.trim())
            .execute(&state.pool)
            .await;

    match result {
        Ok(_) => Json(serde_json::json!({ "status": "removed" })).into_response(),
        Err(_) => error(StatusCode::INTERNAL_SERVER_ERROR, "internal_error"),
    }
}

/// GET /api/notifications/preferences
pub async fn get_preferences(State(state): State<PushState>, auth: AuthenticatedUser) -> Response {
    let row =
        sqlx::query("SELECT circle_push_enabled, push_financial_details FROM users WHERE id = $1")
            .bind(auth.id)
            .fetch_optional(&state.pool)
            .await;

    match row {
        Ok(Some(row)) => Json(serde_json::json!({
            "circle_push_enabled": row.get::<bool, _>("circle_push_enabled"),
            "push_financial_details": row.get::<bool, _>("push_financial_details"),
        }))
        .into_response(),
        _ => error(StatusCode::NOT_FOUND, "user_not_found"),
    }
}

/// PUT /api/notifications/preferences
pub async fn update_preferences(
    State(state): State<PushState>,
    auth: AuthenticatedUser,
    Json(prefs): Json<NotificationPreferences>,
) -> Response {
    let result = sqlx::query(
        r#"
        UPDATE users
           SET circle_push_enabled = $1, push_financial_details = $2
         WHERE id = $3
        "#,
    )
    .bind(prefs.circle_push_enabled)
    .bind(prefs.push_financial_details)
    .bind(auth.id)
    .execute(&state.pool)
    .await;

    match result {
        Ok(r) if r.rows_affected() > 0 => Json(prefs).into_response(),
        Ok(_) => error(StatusCode::NOT_FOUND, "user_not_found"),
        Err(_) => error(StatusCode::INTERNAL_SERVER_ERROR, "internal_error"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_expo_tokens_are_accepted() {
        assert!(valid_token("ExponentPushToken[abc123]"));
        assert!(!valid_token("not-a-token"));
        assert!(!valid_token("ExponentPushToken[abc123"));
    }
}
