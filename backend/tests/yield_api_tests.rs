//! Integration tests for /api/yield endpoints.
//!
//! These tests build the real Axum router and call it via
//! `tower::ServiceExt::oneshot`, so no TCP socket is needed.
//!
//! # Prerequisites
//! Set `DATABASE_URL` (or `TEST_DATABASE_URL`) to a live PostgreSQL instance
//! that has the Zaps schema applied.

use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use http_body_util::BodyExt;
use serde_json::Value;
use sqlx::PgPool;
use tower::ServiceExt; // for `oneshot`
use uuid::Uuid;

// Re-exported from crate.
use zaps_backend::api::feed::AuthUser;

// ── helpers ─────────────────────────────────────────────────────────────────

async fn test_pool() -> PgPool {
    let url = std::env::var("TEST_DATABASE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .expect("Set TEST_DATABASE_URL or DATABASE_URL to run integration tests");
    PgPool::connect(&url)
        .await
        .expect("Failed to connect to test database")
}

fn yield_router(pool: PgPool) -> Router {
    // Build the yield-only router at the same paths used by the app.
    zaps_backend::api::yield_routes(pool)
}

async fn seed_user(pool: &PgPool, address: &str) -> Uuid {
    // AuthUser extractor upserts by address and returns the UUID.
    sqlx::query_scalar::<_, Uuid>(
        r#"
        INSERT INTO users (address, username, display_name)
        VALUES ($1, $2, $2)
        ON CONFLICT (address) DO UPDATE SET username = EXCLUDED.username
        RETURNING id
        "#,
    )
    .bind(address)
    .bind(format!("user_{}", &address[1..std::cmp::min(10, address.len())]))
    .fetch_one(pool)
    .await
    .expect("Failed to seed user")
}

/// Parse JSON response body from a oneshot call.
async fn response_json(router: &Router, req: Request<Body>) -> (StatusCode, Value) {
    let response = router.clone().oneshot(req).await.expect("oneshot failed");
    let status = response.status();
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("Failed to read body")
        .to_bytes();
    let json: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, json)
}

fn get_req(path: &str, token: Option<&str>) -> Request<Body> {
    let mut builder = Request::builder().method("GET").uri(path);
    if let Some(t) = token {
        builder = builder.header("Authorization", format!("Bearer {}", t));
    }
    builder.body(Body::empty()).unwrap()
}

fn post_req_json(path: &str, token: Option<&str>, payload: Value) -> Request<Body> {
    let mut builder = Request::builder().method("POST").uri(path);
    if let Some(t) = token {
        builder = builder.header("Authorization", format!("Bearer {}", t));
    }
    builder
        .header("content-type", "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap()
}

// ── tests ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_yield_endpoints_require_auth() {
    let pool = test_pool().await;
    let router = yield_router(pool);

    // Missing auth header
    for path in [
        "/balance",
        "/history?limit=1&offset=0",
    ]
    {
        let (status, _) = response_json(&router, get_req(path, None)).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{path} must be 401 without auth");
    }

    // Missing auth header for POST
    for (path, payload) in [
        ("/toggle-auto", serde_json::json!({ "enabled": true })),
        ("/deposit", serde_json::json!({ "amount": 1000 })),
        ("/withdraw", serde_json::json!({ "amount": 1000 })),
    ] {
        let req = post_req_json(path, None, payload);
        let (status, _) = response_json(&router, req).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{path} must be 401 without auth");
    }
}

#[tokio::test]
async fn test_yield_balance_history_toggle() {
    let pool = test_pool().await;
    let router = yield_router(pool.clone());

    // Use stable, collision-resistant address suffixes.
    let run = Uuid::new_v4().to_string().replace('-', "")[..8].to_string();

    // This address string is what the AuthUser extractor maps from JWT `sub`.
    // It may not be a real Stellar address; the extractor only uses it as an opaque key.
    let address = format!(
        "GTESTYIELDUSER{}XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX",
        run
    );
    let user_id = seed_user(&pool, &address).await;

    // Token mapping:
    // - If JWT decoding fails, extractor falls back to using `token` as address.
    // - We can set Authorization token to the address itself.
    let token = address.clone();

    // 1) GET /api/yield/balance
    let (status, body) = response_json(&router, get_req("/balance", Some(&token))).await;
    assert_eq!(status, StatusCode::OK);

    // Validate shape & defaults.
    assert!(body.get("available_balance").is_some(), "available_balance missing");
    assert!(body.get("earning_balance").is_some(), "earning_balance missing");
    assert!(body.get("accrued_interest").is_some(), "accrued_interest missing");
    assert!(body.get("total_earning_balance").is_some(), "total_earning_balance missing");
    assert!(body.get("apy").is_some(), "apy missing");

    // auto_earn_enabled default comes from `users.auto_earn_enabled`.
    assert_eq!(
        body.get("auto_earn_enabled").and_then(|v| v.as_bool()).unwrap_or(true),
        false,
        "auto_earn_enabled should default to false"
    );

    // 2) Seed at least one yield history transaction by POST /deposit.
    let deposit_payload = serde_json::json!({ "amount": 5000 });
    let (deposit_status, deposit_body) = response_json(
        &router,
        post_req_json("/deposit", Some(&token), deposit_payload),
    )
    .await;

    // Deposit requires sufficient available_balance. Since we only seeded yield balance row on demand (0s),
    // the safest way to create history is to directly create a yield_transactions row.
    // However the acceptance criteria asks to test balances/history/toggle; so we insert history row here.
    //
    // If deposit failed, we still can create history by a direct insert.
    if deposit_status != StatusCode::OK {
        // Insert a DEPOSIT history record and balances.
        let tx_hash = format!("test-yield-tx-{}", Uuid::new_v4());
        sqlx::query(
            r#"
            INSERT INTO yield_transactions (user_id, tx_hash, type, amount, created_at)
            VALUES ($1, $2, 'DEPOSIT', $3, NOW())
            "#,
        )
        .bind(user_id)
        .bind(&tx_hash)
        .bind(1234_i64)
        .execute(&pool)
        .await
        .expect("Failed to insert yield transaction");

        // Ensure user yield balance exists with non-zero earning/available.
        sqlx::query(
            r#"
            INSERT INTO user_yield_balances (user_id, available_balance, earning_balance, updated_at, last_yield_sync_at)
            VALUES ($1, 1000, 2000, NOW(), NOW())
            ON CONFLICT (user_id) DO UPDATE
            SET available_balance = EXCLUDED.available_balance,
                earning_balance = EXCLUDED.earning_balance,
                last_yield_sync_at = EXCLUDED.last_yield_sync_at,
                updated_at = NOW()
            "#,
        )
        .bind(user_id)
        .execute(&pool)
        .await
        .expect("Failed to seed yield balances");
    } else {
        // If deposit succeeded, we still want history to exist (it should), but we can't assert tx_hash.
        assert!(deposit_body.get("envelope_xdr").is_some());
    }

    // 3) GET /api/yield/history
    let (hist_status, hist_body) = response_json(
        &router,
        get_req("/history?limit=10&offset=0", Some(&token)),
    )
    .await;
    assert_eq!(hist_status, StatusCode::OK);

    let items = hist_body
        .get("items")
        .and_then(|v| v.as_array())
        .expect("history items must be array");
    assert!(!items.is_empty(), "expected at least one yield history item");

    // Validate item fields exist.
    let first = &items[0];
    assert!(first.get("id").is_some());
    assert!(first.get("tx_hash").is_some());
    assert!(first.get("type").is_some(), "history item must have type");
    assert!(first.get("amount").is_some());
    assert!(first.get("created_at").is_some());

    // 4) POST /toggle-auto: enable then disable.
    let (toggle_on_status, toggle_on_body) = response_json(
        &router,
        post_req_json(
            "/toggle-auto",
            Some(&token),
            serde_json::json!({ "enabled": true }),
        ),
    )
    .await;
    assert_eq!(toggle_on_status, StatusCode::OK);
    assert_eq!(
        toggle_on_body
            .get("auto_earn_enabled")
            .and_then(|v| v.as_bool()),
        Some(true)
    );

    let (toggle_off_status, toggle_off_body) = response_json(
        &router,
        post_req_json(
            "/toggle-auto",
            Some(&token),
            serde_json::json!({ "enabled": false }),
        ),
    )
    .await;
    assert_eq!(toggle_off_status, StatusCode::OK);
    assert_eq!(
        toggle_off_body
            .get("auto_earn_enabled")
            .and_then(|v| v.as_bool()),
        Some(false)
    );

    // Cleanup inserted data.
    // Remove only records tied to our seeded user.
    sqlx::query("DELETE FROM yield_transactions WHERE user_id = $1")
        .bind(user_id)
        .execute(&pool)
        .await
        .ok();
    sqlx::query("DELETE FROM user_yield_balances WHERE user_id = $1")
        .bind(user_id)
        .execute(&pool)
        .await
        .ok();
    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(user_id)
        .execute(&pool)
        .await
        .ok();

    // Avoid unused import warning.
    let _ = AuthUser;
}

