//! Integration tests for the Circles API (#1227).
//!
//! These tests run against a real Postgres instance.  Set `DATABASE_URL` before
//! running:
//!
//! ```sh
//! DATABASE_URL=postgres://zaps:zaps@localhost/zaps_test \
//!   cargo test --test circles_integration_test
//! ```
//!
//! The test suite spins up an isolated schema per test using random UUIDs so
//! tests can run in parallel without interfering.

use axum::{
    body::Body,
    http::{header, Method, Request, StatusCode},
    Router,
};
use serde_json::{json, Value};
use tower::ServiceExt;
use uuid::Uuid;
use zaps_backend::api::circles::{self, CirclesState};

// ── Helpers ───────────────────────────────────────────────────────────────────

fn circles_router(pool: sqlx::PgPool) -> Router {
    use axum::routing::{delete, get, post, put};
    let state = CirclesState::new(pool);
    Router::new()
        .route("/circles", post(circles::create_circle).get(circles::list_circles))
        .route("/circles/discover", get(circles::discover_circles))
        .route("/circles/search", get(circles::search_circles))
        .route("/circles/:id", get(circles::get_circle))
        .route("/circles/:id/join", post(circles::join_circle))
        .route("/circles/:id/leave", post(circles::leave_circle))
        .route("/circles/:id/eligibility", get(circles::check_eligibility))
        .route("/circles/:id/membership/history", get(circles::get_membership_history))
        .route("/circles/:id/roles", get(circles::list_roles))
        .route("/circles/:id/members/:address/role", put(circles::update_member_role))
        .route(
            "/circles/:id/reminders/preferences",
            get(circles::get_reminder_preferences).put(circles::update_reminder_preferences),
        )
        .route(
            "/circles/:id/reminders/trigger",
            post(circles::trigger_contribution_reminders),
        )
        .route("/circles/:id/contribute", post(circles::contribute))
        .route("/circles/:id/payout", post(circles::request_payout))
        .route("/circles/:id/payout/:req_id", get(circles::get_payout_status))
        .route("/circles/:id/payout/:req_id/approve", post(circles::approve_payout))
        .route("/circles/:id/members", get(circles::list_members).post(circles::add_member))
        .route("/circles/:id/members/:address", delete(circles::remove_member))
        .route("/circles/:id/goals", get(circles::list_goals).post(circles::create_goal))
        .with_state(state)
}

/// Build a request with a mock bearer token (interpreted by the test auth stub).
fn authed(method: Method, uri: &str, body: Value, address: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::AUTHORIZATION, format!("Bearer test-token-{address}"))
        .body(Body::from(body.to_string()))
        .unwrap()
}

async fn json_body(response: axum::response::Response) -> Value {
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&bytes).unwrap_or(Value::Null)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn create_circle_returns_201(pool: sqlx::PgPool) {
    let app = circles_router(pool);

    let resp = app
        .oneshot(authed(
            Method::POST,
            "/circles",
            json!({
                "name": "Lagos Ajo",
                "token": "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC",
                "threshold": 2,
                "approvers": ["GDCV2TZJQFVSB7HTXJSMJJQELOIB4BYVZ6U7WFQMMDV36LCJD3CQZQJ"]
            }),
            "GADMIN1111111111111111111111111111111111111111111111111111",
        ))
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = json_body(resp).await;
    assert!(body["id"].is_string(), "response must include 'id'");
}

#[sqlx::test(migrations = "./migrations")]
async fn list_circles_returns_only_caller_circles(pool: sqlx::PgPool) {
    let app = circles_router(pool.clone());

    // Create a circle owned by ADMIN_A.
    let _ = app
        .clone()
        .oneshot(authed(
            Method::POST,
            "/circles",
            json!({
                "name": "Circle A",
                "token": "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC",
                "threshold": 1,
                "approvers": []
            }),
            "GADMINA11111111111111111111111111111111111111111111111111",
        ))
        .await
        .unwrap();

    // ADMIN_B lists circles — they should see none of their own yet.
    let resp = app
        .oneshot(authed(
            Method::GET,
            "/circles",
            json!(null),
            "GADMINB11111111111111111111111111111111111111111111111111",
        ))
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body = json_body(resp).await;
    assert_eq!(body["circles"].as_array().unwrap().len(), 0);
}

#[sqlx::test(migrations = "./migrations")]
async fn get_circle_requires_membership(pool: sqlx::PgPool) {
    let app = circles_router(pool.clone());

    // Create circle as ADMIN_A.
    let create_resp = app
        .clone()
        .oneshot(authed(
            Method::POST,
            "/circles",
            json!({
                "name": "Private Circle",
                "token": "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC",
                "threshold": 1,
                "approvers": []
            }),
            "GADMINA11111111111111111111111111111111111111111111111111",
        ))
        .await
        .unwrap();
    let body = json_body(create_resp).await;
    let circle_id = body["id"].as_str().unwrap().to_string();

    // Non-member GADMINB should get 403.
    let resp = app
        .oneshot(authed(
            Method::GET,
            &format!("/circles/{circle_id}"),
            json!(null),
            "GADMINB11111111111111111111111111111111111111111111111111",
        ))
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[sqlx::test(migrations = "./migrations")]
async fn contribute_increases_balance(pool: sqlx::PgPool) {
    let app = circles_router(pool.clone());

    let create_resp = app
        .clone()
        .oneshot(authed(
            Method::POST,
            "/circles",
            json!({
                "name": "Savings Group",
                "token": "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC",
                "threshold": 1,
                "approvers": ["GADMIN1111111111111111111111111111111111111111111111111111"]
            }),
            "GADMIN1111111111111111111111111111111111111111111111111111",
        ))
        .await
        .unwrap();
    let body = json_body(create_resp).await;
    let circle_id = body["id"].as_str().unwrap().to_string();

    let contrib_resp = app
        .clone()
        .oneshot(authed(
            Method::POST,
            &format!("/circles/{circle_id}/contribute"),
            json!({ "amount": "10000000", "tx_hash": "abc123" }),
            "GADMIN1111111111111111111111111111111111111111111111111111",
        ))
        .await
        .unwrap();

    assert_eq!(contrib_resp.status(), StatusCode::CREATED);

    // Balance should be reflected in detail.
    let detail_resp = app
        .oneshot(authed(
            Method::GET,
            &format!("/circles/{circle_id}"),
            json!(null),
            "GADMIN1111111111111111111111111111111111111111111111111111",
        ))
        .await
        .unwrap();
    let detail = json_body(detail_resp).await;
    assert_eq!(detail["balance"].as_str().unwrap(), "10000000");
}

#[sqlx::test(migrations = "./migrations")]
async fn payout_request_creates_pending_status(pool: sqlx::PgPool) {
    let app = circles_router(pool.clone());
    let admin = "GADMIN1111111111111111111111111111111111111111111111111111";
    let token = "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC";

    let create_body = json!({
        "name": "Payout Test Circle",
        "token": token,
        "threshold": 2,
        "approvers": [admin]
    });
    let circle_id = json_body(
        app.clone().oneshot(authed(Method::POST, "/circles", create_body, admin)).await.unwrap(),
    )
    .await["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Fund the circle first.
    let _ = app
        .clone()
        .oneshot(authed(
            Method::POST,
            &format!("/circles/{circle_id}/contribute"),
            json!({ "amount": "100000000", "tx_hash": "fund_tx" }),
            admin,
        ))
        .await
        .unwrap();

    let payout_resp = app
        .clone()
        .oneshot(authed(
            Method::POST,
            &format!("/circles/{circle_id}/payout"),
            json!({ "to": admin, "amount": "50000000", "asset": token }),
            admin,
        ))
        .await
        .unwrap();

    assert_eq!(payout_resp.status(), StatusCode::CREATED);
    let payout_body = json_body(payout_resp).await;
    assert_eq!(payout_body["status"].as_str().unwrap(), "pending");

    let status_resp = app
        .oneshot(authed(
            Method::GET,
            &format!("/circles/{circle_id}/payout/1"),
            json!(null),
            admin,
        ))
        .await
        .unwrap();
    let status = json_body(status_resp).await;
    assert_eq!(status["current_approvals"].as_i64().unwrap(), 0);
    assert_eq!(status["required_threshold"].as_i64().unwrap(), 2);
}

#[sqlx::test(migrations = "./migrations")]
async fn approve_payout_reaches_threshold(pool: sqlx::PgPool) {
    let app = circles_router(pool.clone());
    let admin = "GADMIN1111111111111111111111111111111111111111111111111111";
    let approver2 = "GAPPRV21111111111111111111111111111111111111111111111111111";
    let token = "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC";

    let circle_id = json_body(
        app.clone()
            .oneshot(authed(
                Method::POST,
                "/circles",
                json!({
                    "name": "Approval Circle",
                    "token": token,
                    "threshold": 2,
                    "approvers": [admin, approver2]
                }),
                admin,
            ))
            .await
            .unwrap(),
    )
    .await["id"]
        .as_str()
        .unwrap()
        .to_string();

    let _ = app
        .clone()
        .oneshot(authed(
            Method::POST,
            &format!("/circles/{circle_id}/contribute"),
            json!({ "amount": "200000000", "tx_hash": "fund" }),
            admin,
        ))
        .await
        .unwrap();

    let _ = app
        .clone()
        .oneshot(authed(
            Method::POST,
            &format!("/circles/{circle_id}/payout"),
            json!({ "to": admin, "amount": "50000000", "asset": token }),
            admin,
        ))
        .await
        .unwrap();

    // First approval.
    let r1 = json_body(
        app.clone()
            .oneshot(authed(
                Method::POST,
                &format!("/circles/{circle_id}/payout/1/approve"),
                json!(null),
                admin,
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(r1["is_approved"].as_bool().unwrap(), false);

    // Second approval reaches threshold.
    let r2 = json_body(
        app.oneshot(authed(
            Method::POST,
            &format!("/circles/{circle_id}/payout/1/approve"),
            json!(null),
            approver2,
        ))
        .await
        .unwrap(),
    )
    .await;
    assert_eq!(r2["is_approved"].as_bool().unwrap(), true);
}

#[sqlx::test(migrations = "./migrations")]
async fn duplicate_approval_returns_409(pool: sqlx::PgPool) {
    let app = circles_router(pool.clone());
    let admin = "GADMIN1111111111111111111111111111111111111111111111111111";
    let token = "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC";

    let circle_id = json_body(
        app.clone()
            .oneshot(authed(
                Method::POST,
                "/circles",
                json!({ "name": "Dupe Test", "token": token, "threshold": 1, "approvers": [admin] }),
                admin,
            ))
            .await
            .unwrap(),
    )
    .await["id"]
        .as_str()
        .unwrap()
        .to_string();

    let _ = app
        .clone()
        .oneshot(authed(
            Method::POST,
            &format!("/circles/{circle_id}/contribute"),
            json!({ "amount": "50000000", "tx_hash": "t" }),
            admin,
        ))
        .await
        .unwrap();

    let _ = app
        .clone()
        .oneshot(authed(
            Method::POST,
            &format!("/circles/{circle_id}/payout"),
            json!({ "to": admin, "amount": "10000000", "asset": token }),
            admin,
        ))
        .await
        .unwrap();

    let _ = app
        .clone()
        .oneshot(authed(
            Method::POST,
            &format!("/circles/{circle_id}/payout/1/approve"),
            json!(null),
            admin,
        ))
        .await
        .unwrap();

    let dup = app
        .oneshot(authed(
            Method::POST,
            &format!("/circles/{circle_id}/payout/1/approve"),
            json!(null),
            admin,
        ))
        .await
        .unwrap();
    assert_eq!(dup.status(), StatusCode::CONFLICT);
}

#[sqlx::test(migrations = "./migrations")]
async fn member_exit_blocked_by_pending_payout(pool: sqlx::PgPool) {
    let app = circles_router(pool.clone());
    let admin = "GADMIN1111111111111111111111111111111111111111111111111111";
    let token = "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC";

    let circle_id = json_body(
        app.clone()
            .oneshot(authed(
                Method::POST,
                "/circles",
                json!({ "name": "Exit Test", "token": token, "threshold": 2, "approvers": [admin] }),
                admin,
            ))
            .await
            .unwrap(),
    )
    .await["id"]
        .as_str()
        .unwrap()
        .to_string();

    let _ = app
        .clone()
        .oneshot(authed(
            Method::POST,
            &format!("/circles/{circle_id}/contribute"),
            json!({ "amount": "100000000", "tx_hash": "t" }),
            admin,
        ))
        .await
        .unwrap();

    let _ = app
        .clone()
        .oneshot(authed(
            Method::POST,
            &format!("/circles/{circle_id}/payout"),
            json!({ "to": admin, "amount": "10000000", "asset": token }),
            admin,
        ))
        .await
        .unwrap();

    let exit_resp = app
        .oneshot(authed(
            Method::DELETE,
            &format!("/circles/{circle_id}/members/{admin}"),
            json!(null),
            admin,
        ))
        .await
        .unwrap();

    assert_eq!(exit_resp.status(), StatusCode::CONFLICT);
    let body = json_body(exit_resp).await;
    assert_eq!(body["error"].as_str().unwrap(), "pending_obligations");
}

#[sqlx::test(migrations = "./migrations")]
async fn create_goal_requires_admin(pool: sqlx::PgPool) {
    let app = circles_router(pool.clone());
    let admin = "GADMIN1111111111111111111111111111111111111111111111111111";
    let member = "GMEMBER111111111111111111111111111111111111111111111111111";
    let token = "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC";

    let circle_id = json_body(
        app.clone()
            .oneshot(authed(
                Method::POST,
                "/circles",
                json!({ "name": "Goal Test", "token": token, "threshold": 1, "approvers": [] }),
                admin,
            ))
            .await
            .unwrap(),
    )
    .await["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Add member to allow membership check to pass.
    let _ = app
        .clone()
        .oneshot(authed(
            Method::POST,
            &format!("/circles/{circle_id}/members"),
            json!({ "address": member }),
            admin,
        ))
        .await
        .unwrap();

    // Non-admin member tries to create a goal.
    let resp = app
        .oneshot(authed(
            Method::POST,
            &format!("/circles/{circle_id}/goals"),
            json!({ "name": "Save for Eid", "target_amount": "1000000000" }),
            member,
        ))
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[sqlx::test(migrations = "./migrations")]
async fn search_circles_partial_match_and_privacy(pool: sqlx::PgPool) {
    let app = circles_router(pool);
    let token = "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC";
    let admin_a = "GADMINA11111111111111111111111111111111111111111111111111";
    let user_b = "GUSERB111111111111111111111111111111111111111111111111111";

    // 1. Create public circle: "Lagos Traders Club"
    let _ = app
        .clone()
        .oneshot(authed(
            Method::POST,
            "/circles",
            json!({
                "name": "Lagos Traders Club",
                "description": "Weekly savings for marketplace vendors",
                "category": "trading",
                "visibility": "public",
                "token": token,
                "threshold": 1,
                "approvers": []
            }),
            admin_a,
        ))
        .await
        .unwrap();

    // 2. Create private circle: "Secret Wealth Group"
    let _ = app
        .clone()
        .oneshot(authed(
            Method::POST,
            "/circles",
            json!({
                "name": "Secret Wealth Group",
                "description": "Private syndicate",
                "category": "wealth",
                "visibility": "private",
                "token": token,
                "threshold": 1,
                "approvers": []
            }),
            admin_a,
        ))
        .await
        .unwrap();

    // 3. User B searches for partial match "traders" -> should find "Lagos Traders Club"
    let resp = app
        .clone()
        .oneshot(authed(
            Method::GET,
            "/circles/search?q=traders",
            json!(null),
            user_b,
        ))
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body = json_body(resp).await;
    assert_eq!(body["total"].as_i64().unwrap(), 1);
    assert_eq!(body["circles"][0]["name"].as_str().unwrap(), "Lagos Traders Club");

    // 4. User B searches for "secret" -> non-member should see 0 results (privacy enforced)
    let resp = app
        .clone()
        .oneshot(authed(
            Method::GET,
            "/circles/search?q=secret",
            json!(null),
            user_b,
        ))
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body = json_body(resp).await;
    assert_eq!(body["total"].as_i64().unwrap(), 0);

    // 5. Admin A searches for "secret" -> member sees 1 result
    let resp = app
        .oneshot(authed(
            Method::GET,
            "/circles/search?q=secret",
            json!(null),
            admin_a,
        ))
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body = json_body(resp).await;
    assert_eq!(body["total"].as_i64().unwrap(), 1);
    assert_eq!(body["circles"][0]["name"].as_str().unwrap(), "Secret Wealth Group");
}

#[sqlx::test(migrations = "./migrations")]
async fn search_circles_empty_and_pagination(pool: sqlx::PgPool) {
    let app = circles_router(pool);
    let user = "GUSER1111111111111111111111111111111111111111111111111111";

    let resp = app
        .oneshot(authed(
            Method::GET,
            "/circles/search?q=nonexistentpatternxyz123",
            json!(null),
            user,
        ))
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body = json_body(resp).await;
    assert_eq!(body["total"].as_i64().unwrap(), 0);
    assert!(body["circles"].as_array().unwrap().is_empty());
}

#[sqlx::test(migrations = "./migrations")]
async fn membership_service_join_limits_and_duplicates(pool: sqlx::PgPool) {
    let app = circles_router(pool.clone());
    let token = "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC";
    let owner = "GOWNER11111111111111111111111111111111111111111111111111";
    let user1 = "GUSER111111111111111111111111111111111111111111111111111";
    let user2 = "GUSER222222222222222222222222222222222222222222222222222";

    // Create circle with member_limit = 2 (owner + 1 other member)
    let create_resp = app
        .clone()
        .oneshot(authed(
            Method::POST,
            "/circles",
            json!({
                "name": "Limited Circle",
                "visibility": "public",
                "token": token,
                "threshold": 1,
                "approvers": []
            }),
            owner,
        ))
        .await
        .unwrap();
    let body = json_body(create_resp).await;
    let circle_id = body["id"].as_str().unwrap().to_string();

    // Set member_limit to 2 directly on the circle
    let _ = sqlx::query!(
        "UPDATE circles SET member_limit = 2 WHERE id = $1::uuid",
        Uuid::parse_str(&circle_id).unwrap()
    )
    .execute(&pool)
    .await;

    // Check eligibility for user1
    let elig_resp = app
        .clone()
        .oneshot(authed(
            Method::GET,
            &format!("/circles/{circle_id}/eligibility"),
            json!(null),
            user1,
        ))
        .await
        .unwrap();
    assert_eq!(elig_resp.status(), StatusCode::OK);
    let elig_body = json_body(elig_resp).await;
    assert_eq!(elig_body["eligible"].as_bool().unwrap(), true);

    // User 1 joins -> 200 OK
    let join_resp = app
        .clone()
        .oneshot(authed(
            Method::POST,
            &format!("/circles/{circle_id}/join"),
            json!(null),
            user1,
        ))
        .await
        .unwrap();
    assert_eq!(join_resp.status(), StatusCode::OK);

    // User 1 attempts to join again -> 409 Conflict (cannot join twice)
    let dup_resp = app
        .clone()
        .oneshot(authed(
            Method::POST,
            &format!("/circles/{circle_id}/join"),
            json!(null),
            user1,
        ))
        .await
        .unwrap();
    assert_eq!(dup_resp.status(), StatusCode::CONFLICT);

    // User 2 attempts to join -> 400 Bad Request (member limit reached)
    let limit_resp = app
        .clone()
        .oneshot(authed(
            Method::POST,
            &format!("/circles/{circle_id}/join"),
            json!(null),
            user2,
        ))
        .await
        .unwrap();
    assert_eq!(limit_resp.status(), StatusCode::BAD_REQUEST);

    // User 1 leaves circle -> 200 OK
    let leave_resp = app
        .clone()
        .oneshot(authed(
            Method::POST,
            &format!("/circles/{circle_id}/leave"),
            json!(null),
            user1,
        ))
        .await
        .unwrap();
    assert_eq!(leave_resp.status(), StatusCode::OK);
}

#[sqlx::test(migrations = "./migrations")]
async fn roles_and_permissions_rbac_enforcement(pool: sqlx::PgPool) {
    let app = circles_router(pool);
    let token = "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC";
    let owner = "GOWNER11111111111111111111111111111111111111111111111111";
    let member_a = "GMEMBERA111111111111111111111111111111111111111111111111";
    let member_b = "GMEMBERB111111111111111111111111111111111111111111111111";

    let circle_id = json_body(
        app.clone()
            .oneshot(authed(
                Method::POST,
                "/circles",
                json!({ "name": "RBAC Test", "token": token, "threshold": 1, "approvers": [] }),
                owner,
            ))
            .await
            .unwrap(),
    )
    .await["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Owner adds member_a and member_b
    let _ = app
        .clone()
        .oneshot(authed(
            Method::POST,
            &format!("/circles/{circle_id}/members"),
            json!({ "address": member_a }),
            owner,
        ))
        .await
        .unwrap();

    let _ = app
        .clone()
        .oneshot(authed(
            Method::POST,
            &format!("/circles/{circle_id}/members"),
            json!({ "address": member_b }),
            owner,
        ))
        .await
        .unwrap();

    // 1. Owner promotes member_a to admin -> 200 OK
    let promote_resp = app
        .clone()
        .oneshot(authed(
            Method::PUT,
            &format!("/circles/{circle_id}/members/{member_a}/role"),
            json!({ "role": "admin" }),
            owner,
        ))
        .await
        .unwrap();
    assert_eq!(promote_resp.status(), StatusCode::OK);

    // 2. Member tries to modify their own role -> 403 Forbidden (cannot self-elevate)
    let self_elevate = app
        .clone()
        .oneshot(authed(
            Method::PUT,
            &format!("/circles/{circle_id}/members/{member_b}/role"),
            json!({ "role": "admin" }),
            member_b,
        ))
        .await
        .unwrap();
    assert_eq!(self_elevate.status(), StatusCode::FORBIDDEN);

    // 3. Admin tries to make member_b an Owner -> 403 Forbidden (only owner can transfer ownership)
    let admin_make_owner = app
        .clone()
        .oneshot(authed(
            Method::PUT,
            &format!("/circles/{circle_id}/members/{member_b}/role"),
            json!({ "role": "owner" }),
            member_a,
        ))
        .await
        .unwrap();
    assert_eq!(admin_make_owner.status(), StatusCode::FORBIDDEN);

    // 4. Admin appoints member_b to moderator -> 200 OK
    let admin_appoint_mod = app
        .clone()
        .oneshot(authed(
            Method::PUT,
            &format!("/circles/{circle_id}/members/{member_b}/role"),
            json!({ "role": "moderator" }),
            member_a,
        ))
        .await
        .unwrap();
    assert_eq!(admin_appoint_mod.status(), StatusCode::OK);

    // 5. Verify member list returns roles
    let roles_resp = app
        .oneshot(authed(
            Method::GET,
            &format!("/circles/{circle_id}/roles"),
            json!(null),
            member_a,
        ))
        .await
        .unwrap();
    assert_eq!(roles_resp.status(), StatusCode::OK);
    let roles_body = json_body(roles_resp).await;
    assert_eq!(roles_body["roles"].as_array().unwrap().len(), 3);
}

#[sqlx::test(migrations = "./migrations")]
async fn reminder_preferences_and_scheduler_lifecycle(pool: sqlx::PgPool) {
    let app = circles_router(pool);
    let token = "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC";
    let owner = "GOWNER11111111111111111111111111111111111111111111111111";

    let circle_id = json_body(
        app.clone()
            .oneshot(authed(
                Method::POST,
                "/circles",
                json!({ "name": "Reminder Test", "token": token, "threshold": 1, "approvers": [] }),
                owner,
            ))
            .await
            .unwrap(),
    )
    .await["id"]
        .as_str()
        .unwrap()
        .to_string();

    // 1. Get default reminder preferences
    let prefs_resp = app
        .clone()
        .oneshot(authed(
            Method::GET,
            &format!("/circles/{circle_id}/reminders/preferences"),
            json!(null),
            owner,
        ))
        .await
        .unwrap();
    assert_eq!(prefs_resp.status(), StatusCode::OK);
    let prefs_body = json_body(prefs_resp).await;
    assert_eq!(prefs_body["enabled"].as_bool().unwrap(), true);
    assert_eq!(prefs_body["lead_time_hours"].as_i64().unwrap(), 24);

    // 2. Update preferences: 48h lead time
    let update_resp = app
        .clone()
        .oneshot(authed(
            Method::PUT,
            &format!("/circles/{circle_id}/reminders/preferences"),
            json!({
                "enabled": true,
                "lead_time_hours": 48,
                "channels": ["in_app", "push"]
            }),
            owner,
        ))
        .await
        .unwrap();
    assert_eq!(update_resp.status(), StatusCode::OK);
    let updated_body = json_body(update_resp).await;
    assert_eq!(updated_body["lead_time_hours"].as_i64().unwrap(), 48);

    // 3. Trigger reminder check
    let trigger_resp = app
        .oneshot(authed(
            Method::POST,
            &format!("/circles/{circle_id}/reminders/trigger"),
            json!(null),
            owner,
        ))
        .await
        .unwrap();
    assert_eq!(trigger_resp.status(), StatusCode::OK);
}
