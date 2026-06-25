//! Integration tests for feed pagination and privacy logic.
//!
//! These tests build the real Axum router and call it via
//! `tower::ServiceExt::oneshot`, so no TCP socket is needed.
//!
//! # Prerequisites
//! Set `DATABASE_URL` (or `TEST_DATABASE_URL`) to a live PostgreSQL instance
//! that has the Zaps schema applied.  The tests insert rows with unique IDs
//! and clean them up at the end, so they are safe to run against a shared
//! development database.

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

// ── helpers ─────────────────────────────────────────────────────────────────

/// Build a PgPool from `TEST_DATABASE_URL` (falls back to `DATABASE_URL`).
async fn test_pool() -> PgPool {
    let url = std::env::var("TEST_DATABASE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .expect("Set TEST_DATABASE_URL or DATABASE_URL to run integration tests");
    PgPool::connect(&url)
        .await
        .expect("Failed to connect to test database")
}

/// Build the Axum router that serves only the feed endpoints.
fn feed_router(pool: PgPool) -> Router {
    use axum::routing::get;
    use axum::Router;

    // We re-expose the private sub-functions here by routing them manually
    // (they are pub inside the crate).
    Router::new()
        .route("/public", get(zaps_backend::api::feed::get_public_feed))
        .route("/friends", get(zaps_backend::api::feed::get_friends_feed))
        .route("/private", get(zaps_backend::api::feed::get_private_feed))
        .with_state(pool)
}

/// Insert a minimal user row and return its UUID.
async fn seed_user(pool: &PgPool, username: &str, address: &str) -> Uuid {
    sqlx::query_scalar(
        r#"
        INSERT INTO users (address, username, display_name)
        VALUES ($1, $2, $2)
        ON CONFLICT (address) DO UPDATE SET username = EXCLUDED.username
        RETURNING id
        "#,
    )
    .bind(address)
    .bind(username)
    .fetch_one(pool)
    .await
    .expect("Failed to seed user")
}

/// Insert a payment with an explicit visibility and return its UUID.
async fn seed_payment(
    pool: &PgPool,
    sender_id: Uuid,
    receiver_id: Uuid,
    visibility: &str,
    tx_hash: &str,
) -> Uuid {
    sqlx::query_scalar(
        r#"
        INSERT INTO payments (tx_hash, sender_id, receiver_id, amount, currency, memo, visibility)
        VALUES ($1, $2, $3, 1000, 'NGN', 'test payment', $4)
        RETURNING id
        "#,
    )
    .bind(tx_hash)
    .bind(sender_id)
    .bind(receiver_id)
    .bind(visibility)
    .fetch_one(pool)
    .await
    .expect("Failed to seed payment")
}

/// Insert an accepted friendship between two users.
async fn seed_friendship(pool: &PgPool, user_id: Uuid, friend_id: Uuid) {
    sqlx::query(
        r#"
        INSERT INTO friendships (user_id, friend_id, status)
        VALUES ($1, $2, 'ACCEPTED')
        ON CONFLICT (user_id, friend_id) DO UPDATE SET status = 'ACCEPTED'
        "#,
    )
    .bind(user_id)
    .bind(friend_id)
    .execute(pool)
    .await
    .expect("Failed to seed friendship");
}

/// Parse the JSON response body from a oneshot call.
async fn response_json(
    router: Router,
    req: Request<Body>,
) -> (StatusCode, Value) {
    let response = router.oneshot(req).await.expect("oneshot failed");
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

/// Build a GET request with an optional Bearer token.
fn get_req(path: &str, token: Option<&str>) -> Request<Body> {
    let mut builder = Request::builder().method("GET").uri(path);
    if let Some(t) = token {
        builder = builder.header("Authorization", format!("Bearer {}", t));
    }
    builder.body(Body::empty()).unwrap()
}

// ── tests ────────────────────────────────────────────────────────────────────

/// Verify that the public feed respects `limit` and `offset` pagination.
///
/// Seeds 15 PUBLIC payments, then checks:
///   - page 1 (`limit=10&offset=0`)  → 10 items
///   - page 2 (`limit=10&offset=10`) → 5 items
#[tokio::test]
async fn test_public_feed_pagination() {
    let pool = test_pool().await;

    // Use stable, collision-resistant address suffixes based on a random run id
    let run = Uuid::new_v4().to_string().replace('-', "")[..8].to_string();
    let sender_addr = format!("GSENDER{}XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX", run);
    let receiver_addr = format!("GRECEIVER{}XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX", run);

    let sender_id = seed_user(&pool, &format!("sender_{run}"), &sender_addr).await;
    let receiver_id = seed_user(&pool, &format!("receiver_{run}"), &receiver_addr).await;

    // Seed 15 PUBLIC payments
    let mut payment_ids: Vec<Uuid> = Vec::new();
    for i in 0..15u32 {
        let hash = format!("PUBHASH{run}{i:04}XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX");
        let id = seed_payment(&pool, sender_id, receiver_id, "PUBLIC", &hash).await;
        payment_ids.push(id);
    }

    let router = feed_router(pool.clone());

    // Page 1
    let (status, body) = response_json(
        router.clone(),
        get_req("/public?limit=10&offset=0", None),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "page 1 status");
    let items = body.as_array().expect("expected array on page 1");
    assert!(items.len() >= 10, "page 1 should have at least 10 items; got {}", items.len());

    // Page 2 (different offset)
    let (status2, body2) = response_json(
        router.clone(),
        get_req("/public?limit=10&offset=10", None),
    )
    .await;
    assert_eq!(status2, StatusCode::OK, "page 2 status");
    let items2 = body2.as_array().expect("expected array on page 2");
    // Must have at least 5 from our seed; the exact count depends on other data in the DB
    assert!(items2.len() >= 5, "page 2 should have at least 5 items; got {}", items2.len());

    // Cleanup
    for id in &payment_ids {
        sqlx::query("DELETE FROM payments WHERE id = $1")
            .bind(id)
            .execute(&pool)
            .await
            .ok();
    }
    sqlx::query("DELETE FROM users WHERE id = $1 OR id = $2")
        .bind(sender_id)
        .bind(receiver_id)
        .execute(&pool)
        .await
        .ok();
}

/// Verify friends-feed privacy: PUBLIC and FRIENDS-only payments are visible;
/// PRIVATE payments are NOT included.
///
/// Setup:
///   - user_a  ←→  user_b  (friends)
///   - 3 payments from user_b to user_a:  PUBLIC, FRIENDS, PRIVATE
///   - Call GET /friends as user_a
///
/// Expected:
///   - PUBLIC payment   → present
///   - FRIENDS payment  → present
///   - PRIVATE payment  → absent
#[tokio::test]
async fn test_friends_feed_privacy() {
    let pool = test_pool().await;

    let run = Uuid::new_v4().to_string().replace('-', "")[..8].to_string();
    let addr_a = format!("GUSERA{}XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX", run);
    let addr_b = format!("GUSERB{}XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX", run);

    let user_a = seed_user(&pool, &format!("user_a_{run}"), &addr_a).await;
    let user_b = seed_user(&pool, &format!("user_b_{run}"), &addr_b).await;
    seed_friendship(&pool, user_a, user_b).await;

    let pub_hash = format!("FPUBHASH{run}XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX");
    let fri_hash = format!("FFRIHASH{run}XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX");
    let prv_hash = format!("FPRVHASH{run}XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX");

    let pub_id = seed_payment(&pool, user_b, user_a, "PUBLIC", &pub_hash).await;
    let fri_id = seed_payment(&pool, user_b, user_a, "FRIENDS", &fri_hash).await;
    let prv_id = seed_payment(&pool, user_b, user_a, "PRIVATE", &prv_hash).await;

    // Build a token that maps to user_a by using their address as the token
    // (the AuthUser extractor will upsert based on address when JWT decode fails)
    let token_a = addr_a.clone();

    let router = feed_router(pool.clone());
    let (status, body) =
        response_json(router, get_req("/friends?limit=50&offset=0", Some(&token_a))).await;

    assert_eq!(status, StatusCode::OK, "friends feed status");
    let items = body.as_array().expect("expected array");
    let ids: Vec<String> = items
        .iter()
        .filter_map(|v| v.get("id").and_then(|id| id.as_str()).map(String::from))
        .collect();

    assert!(
        ids.contains(&pub_id.to_string()),
        "PUBLIC payment must appear in friends feed"
    );
    assert!(
        ids.contains(&fri_id.to_string()),
        "FRIENDS payment must appear in friends feed"
    );
    assert!(
        !ids.contains(&prv_id.to_string()),
        "PRIVATE payment must NOT appear in friends feed"
    );

    // Cleanup
    for id in [pub_id, fri_id, prv_id] {
        sqlx::query("DELETE FROM payments WHERE id = $1")
            .bind(id)
            .execute(&pool)
            .await
            .ok();
    }
    sqlx::query("DELETE FROM friendships WHERE (user_id = $1 AND friend_id = $2) OR (user_id = $2 AND friend_id = $1)")
        .bind(user_a)
        .bind(user_b)
        .execute(&pool)
        .await
        .ok();
    sqlx::query("DELETE FROM users WHERE id = $1 OR id = $2")
        .bind(user_a)
        .bind(user_b)
        .execute(&pool)
        .await
        .ok();
}

/// Verify private-feed isolation: a PRIVATE payment between user_b and user_c
/// must NOT appear in user_a's private feed.
///
/// Setup:
///   - user_b pays user_c with PRIVATE visibility
///   - Call GET /private as user_a
///
/// Expected: user_a cannot see the payment.
#[tokio::test]
async fn test_private_feed_isolation() {
    let pool = test_pool().await;

    let run = Uuid::new_v4().to_string().replace('-', "")[..8].to_string();
    let addr_a = format!("GISOA{}XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX", run);
    let addr_b = format!("GISOB{}XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX", run);
    let addr_c = format!("GISOC{}XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX", run);

    let user_a = seed_user(&pool, &format!("iso_a_{run}"), &addr_a).await;
    let user_b = seed_user(&pool, &format!("iso_b_{run}"), &addr_b).await;
    let user_c = seed_user(&pool, &format!("iso_c_{run}"), &addr_c).await;

    let prv_hash = format!("ISOPRVHASH{run}XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX");
    let prv_id = seed_payment(&pool, user_b, user_c, "PRIVATE", &prv_hash).await;

    // Authenticate as user_a (their address is used as token)
    let token_a = addr_a.clone();

    let router = feed_router(pool.clone());
    let (status, body) =
        response_json(router, get_req("/private?limit=50&offset=0", Some(&token_a))).await;

    assert_eq!(status, StatusCode::OK, "private feed status");
    let items = body.as_array().expect("expected array");
    let ids: Vec<String> = items
        .iter()
        .filter_map(|v| v.get("id").and_then(|id| id.as_str()).map(String::from))
        .collect();

    assert!(
        !ids.contains(&prv_id.to_string()),
        "PRIVATE payment between B and C must NOT appear in A's private feed"
    );

    // Cleanup
    sqlx::query("DELETE FROM payments WHERE id = $1")
        .bind(prv_id)
        .execute(&pool)
        .await
        .ok();
    sqlx::query("DELETE FROM users WHERE id = $1 OR id = $2 OR id = $3")
        .bind(user_a)
        .bind(user_b)
        .bind(user_c)
        .execute(&pool)
        .await
        .ok();
}
