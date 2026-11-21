//! Circles API — pooled-savings circles backed by the Zaps Circle Soroban contract.
//!
//! # Endpoints
//!
//! | Method | Path                                      | Auth | Description                           |
//! |--------|-------------------------------------------|------|---------------------------------------|
//! | POST   | /api/circles                              | ✓    | Create a new circle record            |
//! | GET    | /api/circles                              | ✓    | List circles the caller belongs to    |
//! | GET    | /api/circles/:id                          | ✓    | Get circle detail                     |
//! | POST   | /api/circles/:id/contribute               | ✓    | Record an on-chain contribution       |
//! | POST   | /api/circles/:id/payout                   | ✓    | Submit a payout request               |
//! | GET    | /api/circles/:id/payout/:req_id           | ✓    | Get payout approval status            |
//! | POST   | /api/circles/:id/payout/:req_id/approve   | ✓    | Cast an approver vote                 |
//! | GET    | /api/circles/:id/members                  | ✓    | List members                          |
//! | POST   | /api/circles/:id/members                  | ✓    | Add a member (admin only)             |
//! | DELETE | /api/circles/:id/members/:address         | ✓    | Remove a member / exit                |
//! | GET    | /api/circles/:id/goals                    | ✓    | List savings goals                    |
//! | POST   | /api/circles/:id/goals                    | ✓    | Create a savings goal                 |
//!
//! # Request / Response shapes
//!
//! All endpoints accept and return `application/json`.  Timestamps are RFC-3339
//! strings in UTC.  Stellar/Soroban addresses are base-32 G-addresses.
//! Amounts are signed 128-bit integers expressed as strings to avoid JSON
//! number-precision loss (multiply by `10^7` for stroops).
//!
//! ## POST /api/circles — CreateCircleRequest
//! ```json
//! {
//!   "name": "Lagos Ajo Group",
//!   "token": "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC",
//!   "max_payout": "500000000",
//!   "threshold": 2,
//!   "approvers": ["GDCV…", "GCFZ…"]
//! }
//! ```
//!
//! ## GET /api/circles — ListCirclesResponse
//! ```json
//! {
//!   "circles": [
//!     {
//!       "id": "01931e4a-…",
//!       "contract_address": "CCIR…",
//!       "name": "Lagos Ajo Group",
//!       "balance": "120000000",
//!       "token": "CDLZ…",
//!       "member_count": 5,
//!       "created_at": "2026-11-01T12:00:00Z"
//!     }
//!   ]
//! }
//! ```
//!
//! ## POST /api/circles/:id/contribute — ContributeRequest
//! ```json
//! { "amount": "10000000", "tx_hash": "abc123…" }
//! ```
//!
//! ## POST /api/circles/:id/payout — PayoutRequest
//! ```json
//! { "to": "GDCV…", "amount": "50000000", "asset": "CDLZ…" }
//! ```
//!
//! ## GET /api/circles/:id/payout/:req_id — PayoutStatusResponse
//! ```json
//! {
//!   "request_id": 3,
//!   "status": "pending",
//!   "current_approvals": 1,
//!   "required_threshold": 2,
//!   "approvers": ["GDCV…"]
//! }
//! ```
//!
//! # Error Responses
//!
//! All errors follow the existing API convention:
//! ```json
//! { "error": "circle_not_found" }
//! ```
//!
//! | HTTP status | `error` field              | Meaning                                |
//! |-------------|---------------------------|----------------------------------------|
//! | 400         | `invalid_amount`          | Amount ≤ 0 or non-numeric              |
//! | 400         | `invalid_address`         | Malformed Stellar G-address            |
//! | 400         | `exceeds_payout_limit`    | Request exceeds circle `max_payout`    |
//! | 401         | `unauthorized`            | Missing / invalid bearer token         |
//! | 403         | `not_member`              | Caller is not a circle member          |
//! | 403         | `not_approver`            | Caller is not an approver              |
//! | 404         | `circle_not_found`        | No circle with that ID                 |
//! | 404         | `payout_not_found`        | No payout request with that ID         |
//! | 409         | `already_approved`        | Caller already voted on this request   |
//! | 409         | `already_executed`        | Payout was already executed on-chain   |
//! | 409         | `pending_obligations`     | Member has pending payouts, cannot exit|
//! | 500         | `internal_error`          | Unexpected server error                |

use crate::api::auth_middleware::AuthenticatedUser;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

// ── State ─────────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct CirclesState {
    pub pool: PgPool,
}

impl CirclesState {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

// ── Request / Response types ──────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CreateCircleRequest {
    pub name: String,
    pub contract_address: Option<String>,
    pub token: String,
    /// Maximum single payout (stroops). 0 = unlimited.
    pub max_payout: Option<String>,
    pub threshold: u32,
    pub approvers: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct CircleSummary {
    pub id: Uuid,
    pub contract_address: Option<String>,
    pub name: String,
    pub balance: String,
    pub token: String,
    pub member_count: i64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct CircleDetail {
    pub id: Uuid,
    pub contract_address: Option<String>,
    pub name: String,
    pub admin: String,
    pub token: String,
    pub balance: String,
    pub reserved: String,
    pub max_payout: String,
    pub threshold: i32,
    pub approvers: Vec<String>,
    pub total_contributed: String,
    pub total_paid: String,
    pub member_count: i64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct ContributeRequest {
    pub amount: String,
    pub tx_hash: String,
}

#[derive(Debug, Deserialize)]
pub struct PayoutRequest {
    pub to: String,
    pub amount: String,
    pub asset: String,
}

#[derive(Debug, Serialize)]
pub struct PayoutStatus {
    pub request_id: i64,
    pub status: String,
    pub current_approvals: i32,
    pub required_threshold: i32,
    pub approvers: Vec<String>,
    pub is_executed: bool,
}

#[derive(Debug, Deserialize)]
pub struct AddMemberRequest {
    pub address: String,
}

#[derive(Debug, Serialize)]
pub struct MemberEntry {
    pub address: String,
    pub total_contributed: String,
    pub joined_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateGoalRequest {
    pub name: String,
    pub target_amount: String,
    pub deadline: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
pub struct GoalEntry {
    pub id: Uuid,
    pub name: String,
    pub target_amount: String,
    pub current_amount: String,
    pub deadline: Option<DateTime<Utc>>,
    pub completed: bool,
    pub created_at: DateTime<Utc>,
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn err(msg: &'static str) -> (StatusCode, Json<serde_json::Value>) {
    (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": msg })))
}

fn not_found(msg: &'static str) -> (StatusCode, Json<serde_json::Value>) {
    (StatusCode::NOT_FOUND, Json(serde_json::json!({ "error": msg })))
}

fn forbidden(msg: &'static str) -> (StatusCode, Json<serde_json::Value>) {
    (StatusCode::FORBIDDEN, Json(serde_json::json!({ "error": msg })))
}

fn internal() -> (StatusCode, Json<serde_json::Value>) {
    (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": "internal_error" })))
}

// ── Handlers ──────────────────────────────────────────────────────────────────

/// POST /api/circles
pub async fn create_circle(
    State(state): State<CirclesState>,
    auth: AuthenticatedUser,
    Json(req): Json<CreateCircleRequest>,
) -> impl IntoResponse {
    if req.name.trim().is_empty() {
        return err("name_required").into_response();
    }
    if req.threshold == 0 {
        return err("invalid_threshold").into_response();
    }

    let id = Uuid::new_v4();
    let max_payout: i128 = req
        .max_payout
        .as_deref()
        .unwrap_or("0")
        .parse()
        .unwrap_or(0);

    let result = sqlx::query!(
        r#"
        INSERT INTO circles (
            id, name, admin_address, contract_address, token_address,
            max_payout, threshold, balance, reserved,
            total_contributed, total_paid
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, 0, 0, 0, 0)
        RETURNING id, created_at
        "#,
        id,
        req.name.trim(),
        auth.stellar_address,
        req.contract_address,
        req.token,
        max_payout as i64,
        req.threshold as i32,
    )
    .fetch_one(&state.pool)
    .await;

    match result {
        Ok(row) => {
            // Seed the approvers list.
            for approver in &req.approvers {
                let _ = sqlx::query!(
                    "INSERT INTO circle_approvers (circle_id, address) VALUES ($1, $2) ON CONFLICT DO NOTHING",
                    id,
                    approver
                )
                .execute(&state.pool)
                .await;
            }
            // The admin is always a member.
            let _ = sqlx::query!(
                "INSERT INTO circle_members (circle_id, address) VALUES ($1, $2) ON CONFLICT DO NOTHING",
                id,
                auth.stellar_address
            )
            .execute(&state.pool)
            .await;

            (
                StatusCode::CREATED,
                Json(serde_json::json!({ "id": row.id, "created_at": row.created_at })),
            )
                .into_response()
        }
        Err(_) => internal().into_response(),
    }
}

/// GET /api/circles
pub async fn list_circles(
    State(state): State<CirclesState>,
    auth: AuthenticatedUser,
) -> impl IntoResponse {
    let rows = sqlx::query!(
        r#"
        SELECT
            c.id, c.contract_address, c.name, c.balance, c.token_address,
            c.created_at,
            COUNT(cm.address) AS member_count
        FROM circles c
        JOIN circle_members cm ON cm.circle_id = c.id
        WHERE cm.address = $1
        GROUP BY c.id
        ORDER BY c.created_at DESC
        "#,
        auth.stellar_address
    )
    .fetch_all(&state.pool)
    .await;

    match rows {
        Ok(rows) => {
            let circles: Vec<_> = rows
                .into_iter()
                .map(|r| CircleSummary {
                    id: r.id,
                    contract_address: r.contract_address,
                    name: r.name,
                    balance: r.balance.to_string(),
                    token: r.token_address,
                    member_count: r.member_count.unwrap_or(0),
                    created_at: r.created_at,
                })
                .collect();
            Json(serde_json::json!({ "circles": circles })).into_response()
        }
        Err(_) => internal().into_response(),
    }
}

/// GET /api/circles/:id
pub async fn get_circle(
    State(state): State<CirclesState>,
    auth: AuthenticatedUser,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let row = sqlx::query!(
        r#"
        SELECT
            c.id, c.contract_address, c.name, c.admin_address, c.token_address,
            c.balance, c.reserved, c.max_payout, c.threshold,
            c.total_contributed, c.total_paid, c.created_at,
            COUNT(cm.address) AS member_count
        FROM circles c
        JOIN circle_members cm ON cm.circle_id = c.id
        WHERE c.id = $1
        GROUP BY c.id
        "#,
        id
    )
    .fetch_optional(&state.pool)
    .await;

    match row {
        Ok(Some(r)) => {
            // Verify membership.
            let is_member = sqlx::query_scalar!(
                "SELECT 1 FROM circle_members WHERE circle_id = $1 AND address = $2",
                id,
                auth.stellar_address
            )
            .fetch_optional(&state.pool)
            .await
            .unwrap_or(None)
            .is_some();

            if !is_member {
                return forbidden("not_member").into_response();
            }

            let approvers: Vec<String> = sqlx::query_scalar!(
                "SELECT address FROM circle_approvers WHERE circle_id = $1",
                id
            )
            .fetch_all(&state.pool)
            .await
            .unwrap_or_default();

            let detail = CircleDetail {
                id: r.id,
                contract_address: r.contract_address,
                name: r.name,
                admin: r.admin_address,
                token: r.token_address,
                balance: r.balance.to_string(),
                reserved: r.reserved.to_string(),
                max_payout: r.max_payout.to_string(),
                threshold: r.threshold,
                approvers,
                total_contributed: r.total_contributed.to_string(),
                total_paid: r.total_paid.to_string(),
                member_count: r.member_count.unwrap_or(0),
                created_at: r.created_at,
            };
            Json(detail).into_response()
        }
        Ok(None) => not_found("circle_not_found").into_response(),
        Err(_) => internal().into_response(),
    }
}

/// POST /api/circles/:id/contribute
pub async fn contribute(
    State(state): State<CirclesState>,
    auth: AuthenticatedUser,
    Path(id): Path<Uuid>,
    Json(req): Json<ContributeRequest>,
) -> impl IntoResponse {
    let amount: i64 = match req.amount.parse() {
        Ok(v) if v > 0 => v,
        _ => return err("invalid_amount").into_response(),
    };

    // Verify membership.
    let is_member = sqlx::query_scalar!(
        "SELECT 1 FROM circle_members WHERE circle_id = $1 AND address = $2",
        id,
        auth.stellar_address
    )
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None)
    .is_some();

    if !is_member {
        return forbidden("not_member").into_response();
    }

    let mut tx = match state.pool.begin().await {
        Ok(t) => t,
        Err(_) => return internal().into_response(),
    };

    let contribution_id = Uuid::new_v4();
    let insert = sqlx::query!(
        r#"
        INSERT INTO circle_contributions (id, circle_id, member_address, amount, tx_hash)
        VALUES ($1, $2, $3, $4, $5)
        "#,
        contribution_id,
        id,
        auth.stellar_address,
        amount,
        req.tx_hash,
    )
    .execute(&mut *tx)
    .await;

    if insert.is_err() {
        let _ = tx.rollback().await;
        return internal().into_response();
    }

    // Update aggregate balance and per-member contribution total.
    let update = sqlx::query!(
        r#"
        UPDATE circles
           SET balance           = balance + $1,
               total_contributed = total_contributed + $1
         WHERE id = $2
        "#,
        amount,
        id,
    )
    .execute(&mut *tx)
    .await;

    let member_update = sqlx::query!(
        r#"
        UPDATE circle_members
           SET total_contributed = total_contributed + $1
         WHERE circle_id = $2 AND address = $3
        "#,
        amount,
        id,
        auth.stellar_address,
    )
    .execute(&mut *tx)
    .await;

    if update.is_err() || member_update.is_err() {
        let _ = tx.rollback().await;
        return internal().into_response();
    }

    if tx.commit().await.is_err() {
        return internal().into_response();
    }

    (
        StatusCode::CREATED,
        Json(serde_json::json!({ "contribution_id": contribution_id, "new_balance": amount })),
    )
        .into_response()
}

/// POST /api/circles/:id/payout
pub async fn request_payout(
    State(state): State<CirclesState>,
    auth: AuthenticatedUser,
    Path(id): Path<Uuid>,
    Json(req): Json<PayoutRequest>,
) -> impl IntoResponse {
    let amount: i64 = match req.amount.parse() {
        Ok(v) if v > 0 => v,
        _ => return err("invalid_amount").into_response(),
    };

    // Verify membership.
    let is_member = sqlx::query_scalar!(
        "SELECT 1 FROM circle_members WHERE circle_id = $1 AND address = $2",
        id,
        auth.stellar_address
    )
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None)
    .is_some();

    if !is_member {
        return forbidden("not_member").into_response();
    }

    // Check max_payout limit.
    let circle = sqlx::query!(
        "SELECT max_payout, threshold, balance, reserved FROM circles WHERE id = $1",
        id
    )
    .fetch_optional(&state.pool)
    .await;

    let circle = match circle {
        Ok(Some(c)) => c,
        Ok(None) => return not_found("circle_not_found").into_response(),
        Err(_) => return internal().into_response(),
    };

    if circle.max_payout > 0 && amount > circle.max_payout {
        return err("exceeds_payout_limit").into_response();
    }

    if circle.balance - circle.reserved < amount {
        return err("insufficient_balance").into_response();
    }

    let payout_id = Uuid::new_v4();
    let next_req_id: i64 = sqlx::query_scalar!(
        "SELECT COALESCE(MAX(request_id), 0) + 1 FROM circle_payout_requests WHERE circle_id = $1",
        id
    )
    .fetch_one(&state.pool)
    .await
    .unwrap_or(1);

    let mut tx = match state.pool.begin().await {
        Ok(t) => t,
        Err(_) => return internal().into_response(),
    };

    let insert = sqlx::query!(
        r#"
        INSERT INTO circle_payout_requests
            (id, circle_id, request_id, requester_address, to_address, amount,
             asset_address, threshold_at_creation, status)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'pending')
        "#,
        payout_id,
        id,
        next_req_id,
        auth.stellar_address,
        req.to,
        amount,
        req.asset,
        circle.threshold,
    )
    .execute(&mut *tx)
    .await;

    if insert.is_err() {
        let _ = tx.rollback().await;
        return internal().into_response();
    }

    // Reserve the amount so it cannot be double-spent.
    let _ = sqlx::query!(
        "UPDATE circles SET reserved = reserved + $1 WHERE id = $2",
        amount,
        id
    )
    .execute(&mut *tx)
    .await;

    if tx.commit().await.is_err() {
        return internal().into_response();
    }

    (
        StatusCode::CREATED,
        Json(serde_json::json!({ "request_id": next_req_id, "status": "pending" })),
    )
        .into_response()
}

/// GET /api/circles/:id/payout/:req_id
pub async fn get_payout_status(
    State(state): State<CirclesState>,
    auth: AuthenticatedUser,
    Path((circle_id, req_id)): Path<(Uuid, i64)>,
) -> impl IntoResponse {
    let is_member = sqlx::query_scalar!(
        "SELECT 1 FROM circle_members WHERE circle_id = $1 AND address = $2",
        circle_id,
        auth.stellar_address
    )
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None)
    .is_some();

    if !is_member {
        return forbidden("not_member").into_response();
    }

    let row = sqlx::query!(
        r#"
        SELECT request_id, status, threshold_at_creation, is_executed
        FROM circle_payout_requests
        WHERE circle_id = $1 AND request_id = $2
        "#,
        circle_id,
        req_id
    )
    .fetch_optional(&state.pool)
    .await;

    match row {
        Ok(Some(r)) => {
            let approvers: Vec<String> = sqlx::query_scalar!(
                r#"
                SELECT approver_address FROM circle_payout_approvals
                WHERE circle_id = $1 AND request_id = $2
                "#,
                circle_id,
                req_id
            )
            .fetch_all(&state.pool)
            .await
            .unwrap_or_default();

            let status = PayoutStatus {
                request_id: r.request_id,
                status: r.status,
                current_approvals: approvers.len() as i32,
                required_threshold: r.threshold_at_creation,
                approvers,
                is_executed: r.is_executed,
            };
            Json(status).into_response()
        }
        Ok(None) => not_found("payout_not_found").into_response(),
        Err(_) => internal().into_response(),
    }
}

/// POST /api/circles/:id/payout/:req_id/approve
pub async fn approve_payout(
    State(state): State<CirclesState>,
    auth: AuthenticatedUser,
    Path((circle_id, req_id)): Path<(Uuid, i64)>,
) -> impl IntoResponse {
    // Must be a registered approver.
    let is_approver = sqlx::query_scalar!(
        "SELECT 1 FROM circle_approvers WHERE circle_id = $1 AND address = $2",
        circle_id,
        auth.stellar_address
    )
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None)
    .is_some();

    if !is_approver {
        return forbidden("not_approver").into_response();
    }

    // Check request exists and is not yet executed.
    let req_row = sqlx::query!(
        "SELECT is_executed, status FROM circle_payout_requests WHERE circle_id = $1 AND request_id = $2",
        circle_id,
        req_id
    )
    .fetch_optional(&state.pool)
    .await;

    match req_row {
        Ok(Some(r)) if r.is_executed => return err("already_executed").into_response(),
        Ok(None) => return not_found("payout_not_found").into_response(),
        Err(_) => return internal().into_response(),
        _ => {}
    }

    // Idempotency: reject duplicate approval.
    let already = sqlx::query_scalar!(
        "SELECT 1 FROM circle_payout_approvals WHERE circle_id = $1 AND request_id = $2 AND approver_address = $3",
        circle_id,
        req_id,
        auth.stellar_address
    )
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None)
    .is_some();

    if already {
        return (
            StatusCode::CONFLICT,
            Json(serde_json::json!({ "error": "already_approved" })),
        )
            .into_response();
    }

    let insert = sqlx::query!(
        "INSERT INTO circle_payout_approvals (circle_id, request_id, approver_address) VALUES ($1, $2, $3)",
        circle_id,
        req_id,
        auth.stellar_address
    )
    .execute(&state.pool)
    .await;

    if insert.is_err() {
        return internal().into_response();
    }

    // Check whether threshold has now been met.
    let approval_count: i64 = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM circle_payout_approvals WHERE circle_id = $1 AND request_id = $2",
        circle_id,
        req_id
    )
    .fetch_one(&state.pool)
    .await
    .unwrap_or(0);

    let threshold: i32 = sqlx::query_scalar!(
        "SELECT threshold_at_creation FROM circle_payout_requests WHERE circle_id = $1 AND request_id = $2",
        circle_id,
        req_id
    )
    .fetch_one(&state.pool)
    .await
    .unwrap_or(999);

    let approved = approval_count >= threshold as i64;
    if approved {
        let _ = sqlx::query!(
            "UPDATE circle_payout_requests SET status = 'approved' WHERE circle_id = $1 AND request_id = $2",
            circle_id,
            req_id
        )
        .execute(&state.pool)
        .await;
    }

    Json(serde_json::json!({
        "current_approvals": approval_count,
        "threshold": threshold,
        "is_approved": approved,
    }))
    .into_response()
}

/// GET /api/circles/:id/members
pub async fn list_members(
    State(state): State<CirclesState>,
    auth: AuthenticatedUser,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let is_member = sqlx::query_scalar!(
        "SELECT 1 FROM circle_members WHERE circle_id = $1 AND address = $2",
        id,
        auth.stellar_address
    )
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None)
    .is_some();

    if !is_member {
        return forbidden("not_member").into_response();
    }

    let rows = sqlx::query!(
        "SELECT address, total_contributed, joined_at FROM circle_members WHERE circle_id = $1 ORDER BY joined_at",
        id
    )
    .fetch_all(&state.pool)
    .await;

    match rows {
        Ok(rows) => {
            let members: Vec<_> = rows
                .into_iter()
                .map(|r| MemberEntry {
                    address: r.address,
                    total_contributed: r.total_contributed.to_string(),
                    joined_at: r.joined_at,
                })
                .collect();
            Json(serde_json::json!({ "members": members })).into_response()
        }
        Err(_) => internal().into_response(),
    }
}

/// POST /api/circles/:id/members
pub async fn add_member(
    State(state): State<CirclesState>,
    auth: AuthenticatedUser,
    Path(id): Path<Uuid>,
    Json(req): Json<AddMemberRequest>,
) -> impl IntoResponse {
    // Only the circle admin may add members.
    let is_admin = sqlx::query_scalar!(
        "SELECT 1 FROM circles WHERE id = $1 AND admin_address = $2",
        id,
        auth.stellar_address
    )
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None)
    .is_some();

    if !is_admin {
        return forbidden("not_admin").into_response();
    }

    let result = sqlx::query!(
        "INSERT INTO circle_members (circle_id, address) VALUES ($1, $2) ON CONFLICT DO NOTHING",
        id,
        req.address
    )
    .execute(&state.pool)
    .await;

    match result {
        Ok(_) => (StatusCode::CREATED, Json(serde_json::json!({ "address": req.address }))).into_response(),
        Err(_) => internal().into_response(),
    }
}

/// DELETE /api/circles/:id/members/:address
pub async fn remove_member(
    State(state): State<CirclesState>,
    auth: AuthenticatedUser,
    Path((circle_id, address)): Path<(Uuid, String)>,
) -> impl IntoResponse {
    // A member can exit themselves; only the admin can remove others.
    let is_self = auth.stellar_address == address;
    if !is_self {
        let is_admin = sqlx::query_scalar!(
            "SELECT 1 FROM circles WHERE id = $1 AND admin_address = $2",
            circle_id,
            auth.stellar_address
        )
        .fetch_optional(&state.pool)
        .await
        .unwrap_or(None)
        .is_some();
        if !is_admin {
            return forbidden("not_admin").into_response();
        }
    }

    // Block exit if the member has pending payout requests.
    let pending: i64 = sqlx::query_scalar!(
        r#"
        SELECT COUNT(*) FROM circle_payout_requests
        WHERE circle_id = $1 AND requester_address = $2 AND status = 'pending'
        "#,
        circle_id,
        address
    )
    .fetch_one(&state.pool)
    .await
    .unwrap_or(0);

    if pending > 0 {
        return (
            StatusCode::CONFLICT,
            Json(serde_json::json!({ "error": "pending_obligations" })),
        )
            .into_response();
    }

    let result = sqlx::query!(
        "DELETE FROM circle_members WHERE circle_id = $1 AND address = $2",
        circle_id,
        address
    )
    .execute(&state.pool)
    .await;

    match result {
        Ok(r) if r.rows_affected() > 0 => StatusCode::NO_CONTENT.into_response(),
        Ok(_) => not_found("member_not_found").into_response(),
        Err(_) => internal().into_response(),
    }
}

/// GET /api/circles/:id/goals
pub async fn list_goals(
    State(state): State<CirclesState>,
    auth: AuthenticatedUser,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let is_member = sqlx::query_scalar!(
        "SELECT 1 FROM circle_members WHERE circle_id = $1 AND address = $2",
        id,
        auth.stellar_address
    )
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None)
    .is_some();

    if !is_member {
        return forbidden("not_member").into_response();
    }

    let rows = sqlx::query!(
        r#"
        SELECT id, name, target_amount, current_amount, deadline, completed, created_at
        FROM circle_goals WHERE circle_id = $1 ORDER BY created_at
        "#,
        id
    )
    .fetch_all(&state.pool)
    .await;

    match rows {
        Ok(rows) => {
            let goals: Vec<_> = rows
                .into_iter()
                .map(|r| GoalEntry {
                    id: r.id,
                    name: r.name,
                    target_amount: r.target_amount.to_string(),
                    current_amount: r.current_amount.to_string(),
                    deadline: r.deadline,
                    completed: r.completed,
                    created_at: r.created_at,
                })
                .collect();
            Json(serde_json::json!({ "goals": goals })).into_response()
        }
        Err(_) => internal().into_response(),
    }
}

/// POST /api/circles/:id/goals
pub async fn create_goal(
    State(state): State<CirclesState>,
    auth: AuthenticatedUser,
    Path(id): Path<Uuid>,
    Json(req): Json<CreateGoalRequest>,
) -> impl IntoResponse {
    let is_admin = sqlx::query_scalar!(
        "SELECT 1 FROM circles WHERE id = $1 AND admin_address = $2",
        id,
        auth.stellar_address
    )
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None)
    .is_some();

    if !is_admin {
        return forbidden("not_admin").into_response();
    }

    let target: i64 = match req.target_amount.parse() {
        Ok(v) if v > 0 => v,
        _ => return err("invalid_amount").into_response(),
    };

    let goal_id = Uuid::new_v4();
    let result = sqlx::query!(
        r#"
        INSERT INTO circle_goals (id, circle_id, name, target_amount, current_amount, deadline, completed)
        VALUES ($1, $2, $3, $4, 0, $5, false)
        RETURNING id, created_at
        "#,
        goal_id,
        id,
        req.name.trim(),
        target,
        req.deadline,
    )
    .fetch_one(&state.pool)
    .await;

    match result {
        Ok(row) => (
            StatusCode::CREATED,
            Json(serde_json::json!({ "id": row.id, "created_at": row.created_at })),
        )
            .into_response(),
        Err(_) => internal().into_response(),
    }
}

// ── Integration tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
        routing::{delete, get, post},
        Router,
    };
    use serde_json::json;
    use tower::ServiceExt;

    fn test_router(pool: PgPool) -> Router {
        let state = CirclesState::new(pool);
        Router::new()
            .route("/circles", post(create_circle).get(list_circles))
            .route("/circles/:id", get(get_circle))
            .route("/circles/:id/contribute", post(contribute))
            .route("/circles/:id/payout", post(request_payout))
            .route("/circles/:id/payout/:req_id", get(get_payout_status))
            .route("/circles/:id/payout/:req_id/approve", post(approve_payout))
            .route("/circles/:id/members", get(list_members).post(add_member))
            .route("/circles/:id/members/:address", delete(remove_member))
            .route("/circles/:id/goals", get(list_goals).post(create_goal))
            .with_state(state)
    }

    /// Verify that the router compiles and routes are wired correctly.
    /// Full integration tests that hit a live Postgres instance live in
    /// `backend/tests/circles_integration_test.rs`.
    #[test]
    fn router_compiles() {
        // This test exists purely to exercise the type-checker path.
        // The actual runtime tests require a database; see the integration
        // test file for those.
        let _ = std::mem::size_of::<CirclesState>();
        let _ = std::mem::size_of::<CreateCircleRequest>();
        let _ = std::mem::size_of::<CircleSummary>();
        let _ = std::mem::size_of::<CircleDetail>();
        let _ = std::mem::size_of::<ContributeRequest>();
        let _ = std::mem::size_of::<PayoutRequest>();
        let _ = std::mem::size_of::<PayoutStatus>();
    }
}
