//! Circles API — pooled-savings circles backed by the Zaps Circle Soroban contract.
//!
//! # Endpoints
//!
//! | Method | Path                                      | Auth | Description                           |
//! |--------|-------------------------------------------|------|---------------------------------------|
//! | POST   | /api/circles                              | ✓    | Create a new circle (#1194)           |
//! | GET    | /api/circles                              | ✓    | List circles the caller belongs to    |
//! | GET    | /api/circles/discover                     | ✓    | Discover public circles (#1196)       |
//! | GET    | /api/circles/:id                          | ✓    | Get circle detail (#1195)             |
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
//! strings in UTC.  Amounts are `i64` values in stroops (multiply by `10^7`).
//!
//! ## POST /api/circles — CreateCircleRequest (#1194)
//! ```json
//! {
//!   "name": "Lagos Ajo Group",
//!   "description": "A weekly savings circle for the Lagos crew",
//!   "category": "savings",
//!   "visibility": "public",
//!   "image_url": "https://cdn.example.com/image.png",
//!   "target_amount": "5000000000",
//!   "token": "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC",
//!   "max_payout": "500000000",
//!   "threshold": 2,
//!   "approvers": ["GDCV…", "GCFZ…"]
//! }
//! ```
//!
//! ## GET /api/circles/discover — DiscoverQuery (#1196)
//! Query params: `category`, `visibility`, `min_target`, `max_target`,
//!               `has_space` (bool), `page` (default 1), `per_page` (default 20, max 100)
//!
//! ```json
//! {
//!   "circles": [ { "id": "…", "name": "…", … } ],
//!   "total": 42,
//!   "page": 1,
//!   "per_page": 20
//! }
//! ```
//!
//! ## GET /api/circles/:id — CircleDetailResponse (#1195)
//! Returns full circle metadata, membership info, contribution progress, active
//! rules, and statistics.  Private circles are only accessible to members.
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
//! | 400         | `name_required`           | Empty name                             |
//! | 400         | `invalid_visibility`      | Unrecognised visibility value          |
//! | 400         | `invalid_amount`          | Amount ≤ 0 or non-numeric              |
//! | 400         | `invalid_threshold`       | Threshold is 0                         |
//! | 400         | `invalid_page`            | page / per_page out of range           |
//! | 400         | `exceeds_payout_limit`    | Request exceeds circle `max_payout`    |
//! | 400         | `insufficient_balance`    | Not enough unreserved balance          |
//! | 401         | `unauthorized`            | Missing / invalid bearer token         |
//! | 403         | `not_member`              | Caller is not a circle member          |
//! | 403         | `not_admin`               | Caller is not the circle admin         |
//! | 403         | `not_approver`            | Caller is not an approver              |
//! | 404         | `circle_not_found`        | No circle with that ID                 |
//! | 404         | `payout_not_found`        | No payout request with that ID         |
//! | 404         | `member_not_found`        | No such member                         |
//! | 409         | `already_approved`        | Caller already voted on this request   |
//! | 409         | `already_executed`        | Payout was already executed on-chain   |
//! | 409         | `pending_obligations`     | Member has pending payouts, cannot exit|
//! | 500         | `internal_error`          | Unexpected server error                |

use crate::api::auth_middleware::AuthenticatedUser;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

// ── Constants ─────────────────────────────────────────────────────────────────

const VALID_VISIBILITIES: &[&str] = &["public", "private", "invite_only"];
const DEFAULT_PER_PAGE: i64 = 20;
const MAX_PER_PAGE: i64 = 100;

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

/// #1194 — Create Circle request body.
#[derive(Debug, Deserialize)]
pub struct CreateCircleRequest {
    /// Display name, required.
    pub name: String,
    /// Optional human-readable description.
    pub description: Option<String>,
    /// Category tag (e.g. "savings", "investment", "general").
    pub category: Option<String>,
    /// One of: "public", "private", "invite_only". Defaults to "public".
    pub visibility: Option<String>,
    /// URL to a cover image stored in object storage.
    pub image_url: Option<String>,
    /// Savings goal amount in stroops (0 = no goal set).
    pub target_amount: Option<String>,
    /// Soroban contract address, populated after on-chain deployment.
    pub contract_address: Option<String>,
    /// Token contract address (required).
    pub token: String,
    /// Maximum single payout in stroops. 0 = unlimited.
    pub max_payout: Option<String>,
    /// Number of approver signatures required for a payout.
    pub threshold: u32,
    /// Addresses that can approve payouts.
    pub approvers: Vec<String>,
}

/// Lightweight circle row returned in list / discovery endpoints.
#[derive(Debug, Serialize)]
pub struct CircleSummary {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub category: String,
    pub visibility: String,
    pub image_url: Option<String>,
    pub target_amount: String,
    pub balance: String,
    pub token: String,
    pub member_count: i64,
    pub is_discoverable: bool,
    pub created_at: DateTime<Utc>,
}

/// Full circle response returned by GET /api/circles/:id (#1195).
#[derive(Debug, Serialize)]
pub struct CircleDetailResponse {
    // ── Identity ──────────────────────────────────────────────────────────────
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub category: String,
    pub visibility: String,
    pub image_url: Option<String>,
    pub contract_address: Option<String>,
    // ── Membership ────────────────────────────────────────────────────────────
    pub admin: String,
    pub member_count: i64,
    pub caller_is_member: bool,
    pub caller_is_admin: bool,
    // ── Financial ─────────────────────────────────────────────────────────────
    pub token: String,
    pub balance: String,
    pub reserved: String,
    pub max_payout: String,
    pub threshold: i32,
    pub approvers: Vec<String>,
    /// Total contributions ever deposited into this circle.
    pub total_contributed: String,
    /// Total payouts ever disbursed from this circle.
    pub total_paid: String,
    // ── Savings goal progress ─────────────────────────────────────────────────
    /// Configured target amount (0 when no goal is set).
    pub target_amount: String,
    /// Percentage of target reached (0–100). Null when target_amount is 0.
    pub progress_pct: Option<f64>,
    // ── Active contribution rule ───────────────────────────────────────────────
    pub contribution_rule: Option<ContributionRuleEntry>,
    // ── Timestamps ────────────────────────────────────────────────────────────
    pub created_at: DateTime<Utc>,
}

/// Single row from `circle_contribution_rules`.
#[derive(Debug, Serialize)]
pub struct ContributionRuleEntry {
    pub id: Uuid,
    pub min_amount: String,
    pub max_amount: Option<String>,
    pub frequency: String,
    pub is_mandatory: bool,
}

/// #1196 — Query parameters for the discovery endpoint.
#[derive(Debug, Deserialize)]
pub struct DiscoverQuery {
    /// Filter by category tag (exact match).
    pub category: Option<String>,
    /// Filter by visibility value. Defaults to "public" only when unset.
    pub visibility: Option<String>,
    /// Minimum target_amount filter (inclusive).
    pub min_target: Option<i64>,
    /// Maximum target_amount filter (inclusive).
    pub max_target: Option<i64>,
    /// When true, only return circles with room for new members (member_limit not reached).
    pub has_space: Option<bool>,
    /// 1-indexed page number (default: 1).
    pub page: Option<i64>,
    /// Results per page (default: 20, max: 100).
    pub per_page: Option<i64>,
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
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({ "error": "internal_error" })),
    )
}

// ── Handlers ──────────────────────────────────────────────────────────────────

/// POST /api/circles — #1194: Create a new circle.
///
/// The authenticated user is automatically assigned the owner/admin role and
/// added to `circle_members`.  A unique UUID is generated for the circle.
/// Required fields: `name`, `token`, `threshold`.
pub async fn create_circle(
    State(state): State<CirclesState>,
    auth: AuthenticatedUser,
    Json(req): Json<CreateCircleRequest>,
) -> impl IntoResponse {
    // ── Validate required fields ──────────────────────────────────────────────
    if req.name.trim().is_empty() {
        return err("name_required").into_response();
    }
    if req.token.trim().is_empty() {
        return err("token_required").into_response();
    }
    if req.threshold == 0 {
        return err("invalid_threshold").into_response();
    }

    let visibility = req
        .visibility
        .as_deref()
        .unwrap_or("public")
        .to_lowercase();
    if !VALID_VISIBILITIES.contains(&visibility.as_str()) {
        return err("invalid_visibility").into_response();
    }

    let category = req
        .category
        .as_deref()
        .unwrap_or("general")
        .trim()
        .to_lowercase();

    let max_payout: i64 = req
        .max_payout
        .as_deref()
        .unwrap_or("0")
        .parse()
        .unwrap_or(0);

    let target_amount: i64 = req
        .target_amount
        .as_deref()
        .unwrap_or("0")
        .parse()
        .unwrap_or(0);

    // Public and invite-only circles are discoverable; private ones are not.
    let is_discoverable = visibility != "private";

    let id = Uuid::new_v4();

    let result = sqlx::query!(
        r#"
        INSERT INTO circles (
            id, name, description, category, visibility, image_url,
            target_amount, is_discoverable,
            admin_address, contract_address, token_address,
            max_payout, threshold,
            balance, reserved, total_contributed, total_paid
        )
        VALUES (
            $1, $2, $3, $4, $5, $6,
            $7, $8,
            $9, $10, $11,
            $12, $13,
            0, 0, 0, 0
        )
        RETURNING id, created_at
        "#,
        id,
        req.name.trim(),
        req.description,
        category,
        visibility,
        req.image_url,
        target_amount,
        is_discoverable,
        auth.address,
        req.contract_address,
        req.token.trim(),
        max_payout,
        req.threshold as i32,
    )
    .fetch_one(&state.pool)
    .await;

    let row = match result {
        Ok(r) => r,
        Err(_) => return internal().into_response(),
    };

    // Creator is automatically the admin/owner member.
    let _ = sqlx::query!(
        "INSERT INTO circle_members (circle_id, address) VALUES ($1, $2) ON CONFLICT DO NOTHING",
        id,
        auth.address
    )
    .execute(&state.pool)
    .await;

    // Seed approvers (admin is always an implicit approver).
    for approver in &req.approvers {
        let _ = sqlx::query!(
            "INSERT INTO circle_approvers (circle_id, address) VALUES ($1, $2) ON CONFLICT DO NOTHING",
            id,
            approver
        )
        .execute(&state.pool)
        .await;
    }

    (
        StatusCode::CREATED,
        Json(serde_json::json!({
            "id": row.id,
            "created_at": row.created_at,
        })),
    )
        .into_response()
}

/// GET /api/circles — List circles the authenticated user belongs to.
pub async fn list_circles(
    State(state): State<CirclesState>,
    auth: AuthenticatedUser,
) -> impl IntoResponse {
    let rows = sqlx::query!(
        r#"
        SELECT
            c.id, c.name, c.description, c.category, c.visibility,
            c.image_url, c.target_amount, c.balance, c.token_address,
            c.is_discoverable, c.created_at,
            COUNT(cm2.address) AS member_count
        FROM circles c
        JOIN circle_members cm ON cm.circle_id = c.id AND cm.address = $1
        JOIN circle_members cm2 ON cm2.circle_id = c.id
        GROUP BY c.id
        ORDER BY c.created_at DESC
        "#,
        auth.address
    )
    .fetch_all(&state.pool)
    .await;

    match rows {
        Ok(rows) => {
            let circles: Vec<_> = rows
                .into_iter()
                .map(|r| CircleSummary {
                    id: r.id,
                    name: r.name,
                    description: r.description,
                    category: r.category,
                    visibility: r.visibility,
                    image_url: r.image_url,
                    target_amount: r.target_amount.to_string(),
                    balance: r.balance.to_string(),
                    token: r.token_address,
                    member_count: r.member_count.unwrap_or(0),
                    is_discoverable: r.is_discoverable,
                    created_at: r.created_at,
                })
                .collect();
            Json(serde_json::json!({ "circles": circles })).into_response()
        }
        Err(_) => internal().into_response(),
    }
}

/// GET /api/circles/discover — #1196: Discover public circles.
///
/// Returns only `is_discoverable = true` circles whose `visibility` is not
/// `'private'`.  Supports optional filters for `category`, `visibility`,
/// `min_target`, `max_target`, and `has_space` (membership availability).
/// Results are paginated; default page size is 20, maximum is 100.
pub async fn discover_circles(
    State(state): State<CirclesState>,
    _auth: AuthenticatedUser,
    Query(q): Query<DiscoverQuery>,
) -> impl IntoResponse {
    // ── Validate pagination params ────────────────────────────────────────────
    let per_page = q.per_page.unwrap_or(DEFAULT_PER_PAGE).clamp(1, MAX_PER_PAGE);
    let page = q.page.unwrap_or(1);
    if page < 1 {
        return err("invalid_page").into_response();
    }
    let offset = (page - 1) * per_page;

    // ── Validate visibility filter ────────────────────────────────────────────
    if let Some(ref vis) = q.visibility {
        if !VALID_VISIBILITIES.contains(&vis.as_str()) {
            return err("invalid_visibility").into_response();
        }
    }

    // Build WHERE conditions as runtime strings because sqlx compile-time macros
    // cannot handle fully dynamic filter sets.  All user-supplied values are
    // passed as bound parameters to prevent SQL injection.
    //
    // We use a fixed-index approach: collect all bind values and track the
    // next parameter index manually.

    let mut where_clauses: Vec<String> = vec![
        "c.is_discoverable = TRUE".into(),
        "c.visibility != 'private'".into(),
    ];
    let mut bind_index: i32 = 1;

    // We'll build the query string with placeholders then execute via raw query.
    // Collect typed bind values as serde_json for the dynamic query builder.
    // Because sqlx dynamic queries can't use the compile-time macro path, we
    // use `sqlx::query` (runtime) with explicit binding.

    let mut category_val: Option<String> = None;
    let mut visibility_val: Option<String> = None;
    let mut min_target_val: Option<i64> = None;
    let mut max_target_val: Option<i64> = None;

    if let Some(cat) = q.category.as_deref() {
        bind_index += 1;
        where_clauses.push(format!("c.category = ${bind_index}"));
        category_val = Some(cat.to_lowercase());
    }
    if let Some(vis) = q.visibility.as_deref() {
        bind_index += 1;
        where_clauses.push(format!("c.visibility = ${bind_index}"));
        visibility_val = Some(vis.to_string());
    }
    if let Some(min) = q.min_target {
        bind_index += 1;
        where_clauses.push(format!("c.target_amount >= ${bind_index}"));
        min_target_val = Some(min);
    }
    if let Some(max) = q.max_target {
        bind_index += 1;
        where_clauses.push(format!("c.target_amount <= ${bind_index}"));
        max_target_val = Some(max);
    }

    let has_space_filter = q.has_space.unwrap_or(false);
    if has_space_filter {
        // member_limit IS NULL means no cap; otherwise must have room.
        where_clauses.push(
            "(c.member_limit IS NULL OR (SELECT COUNT(*) FROM circle_members WHERE circle_id = c.id) < c.member_limit)".into(),
        );
    }

    let where_sql = where_clauses.join(" AND ");

    // Count query (for pagination metadata).
    let count_sql = format!(
        "SELECT COUNT(*) FROM circles c WHERE {where_sql}"
    );

    // Data query — ordered by newest first for consistency.
    let data_sql = format!(
        r#"
        SELECT
            c.id, c.name, c.description, c.category, c.visibility,
            c.image_url, c.target_amount, c.balance, c.token_address,
            c.is_discoverable, c.created_at,
            COUNT(cm.address) AS member_count
        FROM circles c
        LEFT JOIN circle_members cm ON cm.circle_id = c.id
        WHERE {where_sql}
        GROUP BY c.id
        ORDER BY c.created_at DESC
        LIMIT ${limit_idx} OFFSET ${offset_idx}
        "#,
        limit_idx = bind_index + 1,
        offset_idx = bind_index + 2,
    );

    // Build and execute count query.
    let mut count_q = sqlx::query_scalar::<_, i64>(&count_sql);
    if let Some(ref cat) = category_val {
        count_q = count_q.bind(cat.clone());
    }
    if let Some(ref vis) = visibility_val {
        count_q = count_q.bind(vis.clone());
    }
    if let Some(min) = min_target_val {
        count_q = count_q.bind(min);
    }
    if let Some(max) = max_target_val {
        count_q = count_q.bind(max);
    }

    let total: i64 = match count_q.fetch_one(&state.pool).await {
        Ok(n) => n,
        Err(_) => return internal().into_response(),
    };

    // Build and execute data query.
    let mut data_q = sqlx::query_as::<_, DiscoverRow>(&data_sql);
    if let Some(ref cat) = category_val {
        data_q = data_q.bind(cat.clone());
    }
    if let Some(ref vis) = visibility_val {
        data_q = data_q.bind(vis.clone());
    }
    if let Some(min) = min_target_val {
        data_q = data_q.bind(min);
    }
    if let Some(max) = max_target_val {
        data_q = data_q.bind(max);
    }
    data_q = data_q.bind(per_page).bind(offset);

    match data_q.fetch_all(&state.pool).await {
        Ok(rows) => {
            let circles: Vec<CircleSummary> = rows
                .into_iter()
                .map(|r| CircleSummary {
                    id: r.id,
                    name: r.name,
                    description: r.description,
                    category: r.category,
                    visibility: r.visibility,
                    image_url: r.image_url,
                    target_amount: r.target_amount.to_string(),
                    balance: r.balance.to_string(),
                    token: r.token_address,
                    member_count: r.member_count.unwrap_or(0),
                    is_discoverable: r.is_discoverable,
                    created_at: r.created_at,
                })
                .collect();

            Json(serde_json::json!({
                "circles": circles,
                "total": total,
                "page": page,
                "per_page": per_page,
            }))
            .into_response()
        }
        Err(_) => internal().into_response(),
    }
}

/// Internal row type for the discovery query (runtime sqlx, not compile-time macro).
#[derive(sqlx::FromRow)]
struct DiscoverRow {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub category: String,
    pub visibility: String,
    pub image_url: Option<String>,
    pub target_amount: i64,
    pub balance: i64,
    pub token_address: String,
    pub is_discoverable: bool,
    pub created_at: DateTime<Utc>,
    pub member_count: Option<i64>,
}

/// GET /api/circles/:id — #1195: Retrieve complete circle information.
///
/// Access control:
/// - Public circles: any authenticated user can view.
/// - Invite-only circles: any authenticated user can view the summary
///   (but `caller_is_member` distinguishes the experience).
/// - Private circles: only members may retrieve details (403 for non-members).
///
/// Returns membership info, contribution progress toward the savings goal,
/// the active contribution rule, and financial statistics.
pub async fn get_circle(
    State(state): State<CirclesState>,
    auth: AuthenticatedUser,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let row = sqlx::query!(
        r#"
        SELECT
            c.id, c.name, c.description, c.category, c.visibility,
            c.image_url, c.contract_address, c.target_amount,
            c.admin_address, c.token_address,
            c.balance, c.reserved, c.max_payout, c.threshold,
            c.total_contributed, c.total_paid, c.created_at,
            COUNT(cm.address) AS member_count
        FROM circles c
        LEFT JOIN circle_members cm ON cm.circle_id = c.id
        WHERE c.id = $1
        GROUP BY c.id
        "#,
        id
    )
    .fetch_optional(&state.pool)
    .await;

    let r = match row {
        Ok(Some(r)) => r,
        Ok(None) => return not_found("circle_not_found").into_response(),
        Err(_) => return internal().into_response(),
    };

    let caller_is_member = sqlx::query_scalar!(
        "SELECT 1 FROM circle_members WHERE circle_id = $1 AND address = $2",
        id,
        auth.address
    )
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None)
    .is_some();

    // Private circles are restricted to members only.
    if r.visibility == "private" && !caller_is_member {
        return forbidden("not_member").into_response();
    }

    let caller_is_admin = r.admin_address == auth.address;

    let approvers: Vec<String> = sqlx::query_scalar!(
        "SELECT address FROM circle_approvers WHERE circle_id = $1",
        id
    )
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    // Active contribution rule (most recently created).
    let rule = sqlx::query!(
        r#"
        SELECT id, min_amount, max_amount, frequency, is_mandatory
        FROM circle_contribution_rules
        WHERE circle_id = $1
        ORDER BY created_at DESC
        LIMIT 1
        "#,
        id
    )
    .fetch_optional(&state.pool)
    .await
    .ok()
    .flatten()
    .map(|cr| ContributionRuleEntry {
        id: cr.id,
        min_amount: cr.min_amount.to_string(),
        max_amount: cr.max_amount.map(|v| v.to_string()),
        frequency: cr.frequency,
        is_mandatory: cr.is_mandatory,
    });

    // Progress percentage toward savings goal.
    let progress_pct = if r.target_amount > 0 {
        let pct = (r.total_contributed as f64 / r.target_amount as f64 * 100.0).min(100.0);
        Some((pct * 10.0).round() / 10.0) // round to 1 decimal place
    } else {
        None
    };

    let detail = CircleDetailResponse {
        id: r.id,
        name: r.name,
        description: r.description,
        category: r.category,
        visibility: r.visibility,
        image_url: r.image_url,
        contract_address: r.contract_address,
        admin: r.admin_address,
        member_count: r.member_count.unwrap_or(0),
        caller_is_member,
        caller_is_admin,
        token: r.token_address,
        balance: r.balance.to_string(),
        reserved: r.reserved.to_string(),
        max_payout: r.max_payout.to_string(),
        threshold: r.threshold,
        approvers,
        total_contributed: r.total_contributed.to_string(),
        total_paid: r.total_paid.to_string(),
        target_amount: r.target_amount.to_string(),
        progress_pct,
        contribution_rule: rule,
        created_at: r.created_at,
    };

    Json(detail).into_response()
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

    let is_member = sqlx::query_scalar!(
        "SELECT 1 FROM circle_members WHERE circle_id = $1 AND address = $2",
        id,
        auth.address
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
        auth.address,
        amount,
        req.tx_hash,
    )
    .execute(&mut *tx)
    .await;

    if insert.is_err() {
        let _ = tx.rollback().await;
        return internal().into_response();
    }

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
        auth.address,
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
        Json(serde_json::json!({
            "contribution_id": contribution_id,
            "new_balance": amount,
        })),
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

    let is_member = sqlx::query_scalar!(
        "SELECT 1 FROM circle_members WHERE circle_id = $1 AND address = $2",
        id,
        auth.address
    )
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None)
    .is_some();

    if !is_member {
        return forbidden("not_member").into_response();
    }

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

    let payout_id = Uuid::new_v4();
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
        auth.address,
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
        auth.address
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
    let is_approver = sqlx::query_scalar!(
        "SELECT 1 FROM circle_approvers WHERE circle_id = $1 AND address = $2",
        circle_id,
        auth.address
    )
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None)
    .is_some();

    if !is_approver {
        return forbidden("not_approver").into_response();
    }

    let req_row = sqlx::query!(
        "SELECT is_executed FROM circle_payout_requests WHERE circle_id = $1 AND request_id = $2",
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

    let already = sqlx::query_scalar!(
        "SELECT 1 FROM circle_payout_approvals WHERE circle_id = $1 AND request_id = $2 AND approver_address = $3",
        circle_id,
        req_id,
        auth.address
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
        auth.address
    )
    .execute(&state.pool)
    .await;

    if insert.is_err() {
        return internal().into_response();
    }

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
        auth.address
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
    let is_admin = sqlx::query_scalar!(
        "SELECT 1 FROM circles WHERE id = $1 AND admin_address = $2",
        id,
        auth.address
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
        Ok(_) => (
            StatusCode::CREATED,
            Json(serde_json::json!({ "address": req.address })),
        )
            .into_response(),
        Err(_) => internal().into_response(),
    }
}

/// DELETE /api/circles/:id/members/:address
pub async fn remove_member(
    State(state): State<CirclesState>,
    auth: AuthenticatedUser,
    Path((circle_id, address)): Path<(Uuid, String)>,
) -> impl IntoResponse {
    let is_self = auth.address == address;
    if !is_self {
        let is_admin = sqlx::query_scalar!(
            "SELECT 1 FROM circles WHERE id = $1 AND admin_address = $2",
            circle_id,
            auth.address
        )
        .fetch_optional(&state.pool)
        .await
        .unwrap_or(None)
        .is_some();
        if !is_admin {
            return forbidden("not_admin").into_response();
        }
    }

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
        auth.address
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
        auth.address
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

    if req.name.trim().is_empty() {
        return err("name_required").into_response();
    }

    let goal_id = Uuid::new_v4();
    let result = sqlx::query!(
        r#"
        INSERT INTO circle_goals (id, circle_id, name, target_amount, deadline)
        VALUES ($1, $2, $3, $4, $5)
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

// ── Compile-time size checks (no runtime cost) ────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn types_are_sized() {
        let _ = std::mem::size_of::<CirclesState>();
        let _ = std::mem::size_of::<CreateCircleRequest>();
        let _ = std::mem::size_of::<CircleSummary>();
        let _ = std::mem::size_of::<CircleDetailResponse>();
        let _ = std::mem::size_of::<DiscoverQuery>();
        let _ = std::mem::size_of::<ContributeRequest>();
        let _ = std::mem::size_of::<PayoutRequest>();
        let _ = std::mem::size_of::<PayoutStatus>();
    }
}
