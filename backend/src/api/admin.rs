use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use sqlx::{PgPool, Row};
use std::collections::HashMap;
use uuid::Uuid;

pub fn admin_routes(pool: PgPool) -> Router {
    Router::new()
        .route("/dashboard/stats", get(get_dashboard_stats))
        .route("/vault/stats", get(get_yield_stats))
        .route("/identity/links", get(list_identity_links))
        .route("/identity/links/:user_id", get(get_identity_link))
        .with_state(pool)
}

pub async fn get_dashboard_stats(
    State(pool): State<PgPool>,
) -> impl IntoResponse {
    let total_users: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&pool)
        .await
        .unwrap_or(0);

    let total_payments: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM payments")
        .fetch_one(&pool)
        .await
        .unwrap_or(0);

    let total_transfers: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM bridge_transactions")
        .fetch_one(&pool)
        .await
        .unwrap_or(0);

    let total_withdrawals: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM yield_transactions WHERE type = 'WITHDRAW'")
        .fetch_one(&pool)
        .await
        .unwrap_or(0);

    let active_merchants: i64 = sqlx::query_scalar("SELECT COUNT(DISTINCT user_id) FROM user_yield_balances")
        .fetch_one(&pool)
        .await
        .unwrap_or(0);

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "total_users": total_users,
            "total_payments": total_payments,
            "total_transfers": total_transfers,
            "total_withdrawals": total_withdrawals,
            "active_merchants": active_merchants.max(1),
        }))
    ).into_response()
}

pub async fn get_yield_stats(
    State(pool): State<PgPool>,
) -> impl IntoResponse {
    let tvl: i64 = sqlx::query_scalar("SELECT COALESCE(SUM(earning_balance), 0) FROM user_yield_balances")
        .fetch_one(&pool)
        .await
        .unwrap_or(0);

    let total_yield_distributed: i64 = sqlx::query_scalar("SELECT COALESCE(SUM(amount), 0) FROM yield_transactions WHERE type = 'EARNED'")
        .fetch_one(&pool)
        .await
        .unwrap_or(0);

    let apy_bps: i32 = sqlx::query_scalar("SELECT apy FROM yield_rates_history ORDER BY created_at DESC LIMIT 1")
        .fetch_one(&pool)
        .await
        .unwrap_or(500);

    // Convert micro-units to whole units for the API stats formatting
    let tvl_usd = tvl as f64 / 1_000_000.0;
    let yield_distributed_usd = total_yield_distributed as f64 / 1_000_000.0;
    let apy = apy_bps as f64 / 100.0;

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "total_value_locked": tvl_usd,
            "total_yield_distributed": yield_distributed_usd,
            "apy": apy,
        }))
    ).into_response()
}

pub async fn list_identity_links(
    State(pool): State<PgPool>,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let limit = params.get("limit").and_then(|v| v.parse::<i64>().ok()).unwrap_or(50);
    let offset = params.get("offset").and_then(|v| v.parse::<i64>().ok()).unwrap_or(0);
    let query_param = params.get("query").map(|s| format!("%{}%", s.to_lowercase()));

    let total: i64 = if let Some(ref q) = query_param {
        sqlx::query_scalar(
            r#"
            SELECT COUNT(*) FROM users
            WHERE privy_did IS NOT NULL
              AND (LOWER(username) LIKE $1 OR LOWER(address) LIKE $1 OR LOWER(display_name) LIKE $1)
            "#
        )
        .bind(q)
        .fetch_one(&pool)
        .await
        .unwrap_or(0)
    } else {
        sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE privy_did IS NOT NULL")
            .fetch_one(&pool)
            .await
            .unwrap_or(0)
    };

    let rows = if let Some(ref q) = query_param {
        sqlx::query(
            r#"
            SELECT id, address, username, display_name, privy_did, privy_linked_at
            FROM users
            WHERE privy_did IS NOT NULL
              AND (LOWER(username) LIKE $1 OR LOWER(address) LIKE $1 OR LOWER(display_name) LIKE $1)
            ORDER BY privy_linked_at DESC
            LIMIT $2 OFFSET $3
            "#
        )
        .bind(q)
        .bind(limit)
        .bind(offset)
        .fetch_all(&pool)
        .await
    } else {
        sqlx::query(
            r#"
            SELECT id, address, username, display_name, privy_did, privy_linked_at
            FROM users
            WHERE privy_did IS NOT NULL
            ORDER BY privy_linked_at DESC
            LIMIT $1 OFFSET $2
            "#
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&pool)
        .await
    };

    match rows {
        Ok(r) => {
            let links: Vec<serde_json::Value> = r.iter().map(|row| {
                let user_id: Uuid = row.get("id");
                let address: String = row.get("address");
                let username: String = row.get("username");
                let display_name: Option<String> = row.get("display_name");
                let privy_did: String = row.get("privy_did");
                let privy_linked_at: Option<chrono::NaiveDateTime> = row.get("privy_linked_at");

                serde_json::json!({
                    "user_id": user_id.to_string(),
                    "privy_did": privy_did,
                    "stellar_address": address,
                    "display_name": display_name.unwrap_or_else(|| username.clone()),
                    "email": format!("{}@zaps.fi", username),
                    "status": "active",
                    "linked_at": privy_linked_at.map(|t| t.and_utc().to_rfc3339()).unwrap_or_default(),
                })
            }).collect();

            (StatusCode::OK, Json(serde_json::json!({ "links": links, "total": total }))).into_response()
        }
        Err(e) => {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))).into_response()
        }
    }
}

pub async fn get_identity_link(
    State(pool): State<PgPool>,
    Path(user_id): Path<String>,
) -> impl IntoResponse {
    let user_uuid = match Uuid::parse_str(&user_id) {
        Ok(uid) => uid,
        Err(_) => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": "Invalid user ID" }))).into_response(),
    };

    let row = sqlx::query(
        r#"
        SELECT id, address, username, display_name, privy_did, privy_linked_at
        FROM users
        WHERE id = $1 AND privy_did IS NOT NULL
        "#
    )
    .bind(user_uuid)
    .fetch_optional(&pool)
    .await;

    match row {
        Ok(Some(row)) => {
            let user_id: Uuid = row.get("id");
            let address: String = row.get("address");
            let username: String = row.get("username");
            let display_name: Option<String> = row.get("display_name");
            let privy_did: String = row.get("privy_did");
            let privy_linked_at: Option<chrono::NaiveDateTime> = row.get("privy_linked_at");

            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "user_id": user_id.to_string(),
                    "privy_did": privy_did,
                    "stellar_address": address,
                    "display_name": display_name.unwrap_or_else(|| username.clone()),
                    "email": format!("{}@zaps.fi", username),
                    "status": "active",
                    "linked_at": privy_linked_at.map(|t| t.and_utc().to_rfc3339()).unwrap_or_default(),
                }))
            ).into_response()
        }
        Ok(None) => (StatusCode::NOT_FOUND, Json(serde_json::json!({ "error": "Identity link not found" }))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))).into_response(),
    }
}
