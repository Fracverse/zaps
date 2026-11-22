//! #1224 — Idempotency for circle contribution and payout endpoints.
//!
//! Clients send `Idempotency-Key` (or `X-Idempotency-Key`) on financial POSTs.
//! The key is scoped to the authenticated user and the operation, and a unique
//! database constraint serialises concurrent duplicates: one request performs
//! the operation, the others either wait/retry (`409`) or receive the original
//! response verbatim. Completed records are retained and cleaned up by
//! [`spawn_cleanup`].

use axum::{
    body::{to_bytes, Body},
    extract::{Request, State},
    http::{header::CONTENT_TYPE, HeaderMap, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};
use std::{env, time::Duration};
use uuid::Uuid;

use super::auth_middleware::AuthenticatedUser;

const MAX_BODY: usize = 2 * 1024 * 1024;

fn key_from_headers(headers: &HeaderMap) -> Option<String> {
    for name in ["idempotency-key", "x-idempotency-key"] {
        if let Some(value) = headers.get(name).and_then(|v| v.to_str().ok()) {
            let value = value.trim();
            if !value.is_empty() && value.len() <= 255 {
                return Some(value.to_string());
            }
        }
    }
    None
}

fn operation_for(path: &str) -> Option<&'static str> {
    if path.contains("/payout") && path.contains("/approve") {
        Some("payout_approve")
    } else if path.contains("/payout") {
        Some("payout")
    } else if path.contains("/contribute") {
        Some("contribute")
    } else {
        None
    }
}

fn error(status: StatusCode, code: &'static str, message: &'static str) -> Response {
    (status, Json(serde_json::json!({ "error": code, "message": message }))).into_response()
}

/// Periodic removal of expired idempotency records.
pub fn spawn_cleanup(pool: PgPool) {
    let hours: i64 = env::var("CIRCLE_IDEMPOTENCY_RETENTION_HOURS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(24 * 7)
        .max(1);
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(3600));
        loop {
            ticker.tick().await;
            let result = sqlx::query(
                "DELETE FROM circle_idempotency_records
                 WHERE created_at < NOW() - ($1 * INTERVAL '1 hour')",
            )
            .bind(hours)
            .execute(&pool)
            .await;
            if let Err(e) = result {
                tracing::warn!(event = "circle_idempotency_cleanup_failed", error = %e, "cleanup failed");
            }
        }
    });
}

/// Idempotency middleware for circle financial operations.
pub async fn circle_idempotency(
    State(pool): State<PgPool>,
    req: Request,
    next: Next,
) -> Response {
    let path = req.uri().path().to_string();
    let operation = match operation_for(&path) {
        Some(op) => op,
        None => return next.run(req).await,
    };

    let key = match key_from_headers(req.headers()) {
        Some(key) => key,
        None => {
            if env::var("CIRCLE_REQUIRE_IDEMPOTENCY_KEY")
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or(false)
            {
                return error(
                    StatusCode::BAD_REQUEST,
                    "idempotency_key_required",
                    "Idempotency-Key header is required for this operation",
                );
            }
            // Backwards compatibility for existing clients: unkeyed requests
            // are processed normally.
            return next.run(req).await;
        }
    };

    let scope = req
        .extensions()
        .get::<AuthenticatedUser>()
        .map(|u| format!("user:{}", u.address))
        .unwrap_or_else(|| {
            format!(
                "ip:{}",
                req.headers()
                    .get("x-forwarded-for")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("unknown")
            )
        });

    let (parts, body) = req.into_parts();
    let bytes = match to_bytes(body, MAX_BODY).await {
        Ok(bytes) => bytes,
        Err(_) => {
            return error(
                StatusCode::PAYLOAD_TOO_LARGE,
                "payload_too_large",
                "Request body is too large",
            )
        }
    };
    let request_hash = hex::encode(Sha256::digest(&bytes));
    let req = Request::from_parts(parts, Body::from(bytes.clone()));

    // Replay a completed operation, or reject a conflicting reuse of the key.
    match sqlx::query(
        r#"SELECT status, request_hash, response_status, response_body
             FROM circle_idempotency_records
            WHERE scope = $1 AND operation = $2 AND idempotency_key = $3"#,
    )
    .bind(&scope)
    .bind(operation)
    .bind(&key)
    .fetch_optional(&pool)
    .await
    {
        Ok(Some(record)) => {
            let stored_hash: String = record.get("request_hash");
            let stored_status: String = record.get("status");
            if stored_hash != request_hash {
                return error(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "idempotency_key_reused",
                    "Idempotency key was already used with a different payload",
                );
            }
            if stored_status == "completed" {
                let response_status: Option<i32> = record.get("response_status");
                let status = StatusCode::from_u16(response_status.unwrap_or(200) as u16)
                    .unwrap_or(StatusCode::OK);
                let body: Option<String> = record.get("response_body");
                let body = body.unwrap_or_default();
                tracing::info!(
                    event = "circle_idempotency_replay",
                    operation,
                    scope = %scope,
                    "replaying original operation result"
                );
                return Response::builder()
                    .status(status)
                    .header(CONTENT_TYPE, "application/json")
                    .body(Body::from(body))
                    .unwrap_or_else(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "internal_error", "Internal server error"));
            }
            return error(
                StatusCode::CONFLICT,
                "idempotency_in_progress",
                "A duplicate request is already being processed; retry shortly",
            );
        }
        Ok(None) => {}
        Err(e) => {
            tracing::warn!(event = "circle_idempotency_lookup_failed", error = %e, "lookup failed");
            return error(StatusCode::INTERNAL_SERVER_ERROR, "internal_error", "Internal server error");
        }
    }

    // Claim the key atomically; concurrent duplicates lose the unique insert.
    let claimed = sqlx::query(
        r#"INSERT INTO circle_idempotency_records
               (id, scope, operation, idempotency_key, request_hash, status)
           VALUES ($1, $2, $3, $4, $5, 'in_progress')
           ON CONFLICT DO NOTHING"#,
    )
    .bind(Uuid::new_v4())
    .bind(&scope)
    .bind(operation)
    .bind(&key)
    .bind(&request_hash)
    .execute(&pool)
    .await;

    match claimed {
        Ok(result) if result.rows_affected() == 1 => {}
        Ok(_) => {
            return error(
                StatusCode::CONFLICT,
                "idempotency_in_progress",
                "A duplicate request is already being processed; retry shortly",
            )
        }
        Err(e) => {
            tracing::warn!(event = "circle_idempotency_claim_failed", error = %e, "claim failed");
            return error(StatusCode::INTERNAL_SERVER_ERROR, "internal_error", "Internal server error");
        }
    }

    let response = next.run(req).await;
    let status = response.status();
    let (parts, body) = response.into_parts();
    let body_bytes = to_bytes(body, MAX_BODY).await.unwrap_or_default();
    let response = Response::from_parts(parts, Body::from(body_bytes.clone()));

    if status.is_success() {
        let _ = sqlx::query(
            r#"UPDATE circle_idempotency_records
                  SET status = 'completed', response_status = $1,
                      response_body = $2, completed_at = NOW()
                WHERE scope = $3 AND operation = $4 AND idempotency_key = $5"#,
        )
        .bind(status.as_u16() as i32)
        .bind(String::from_utf8_lossy(&body_bytes).as_ref())
        .bind(&scope)
        .bind(operation)
        .bind(&key)
        .execute(&pool)
        .await;
    } else {
        // Failed operations are not cached; the client may safely retry.
        let _ = sqlx::query(
            "DELETE FROM circle_idempotency_records
              WHERE scope = $1 AND operation = $2 AND idempotency_key = $3",
        )
        .bind(&scope)
        .bind(operation)
        .bind(&key)
        .execute(&pool)
        .await;
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operations_and_keys_are_scoped() {
        assert_eq!(operation_for("/api/circles/1/contribute"), Some("contribute"));
        assert_eq!(operation_for("/api/circles/1/payout"), Some("payout"));
        assert_eq!(
            operation_for("/api/circles/1/payout/2/approve"),
            Some("payout_approve")
        );
        assert_eq!(operation_for("/api/circles/search"), None);
        let mut headers = HeaderMap::new();
        headers.insert("idempotency-key", "abc".parse().unwrap());
        assert_eq!(key_from_headers(&headers).as_deref(), Some("abc"));
    }
}
