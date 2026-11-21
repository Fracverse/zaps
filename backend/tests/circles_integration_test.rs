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
    use axum::routing::{delete, get, post};
    let state = CirclesState::new(pool);
    Router::new()
        .route("/circles", post(circles::create_circle).get(circles::list_circles))
        .route("/circles/:id", get(circles::get_circle))
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
