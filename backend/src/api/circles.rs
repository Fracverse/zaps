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
use crate::services::circle_audit;
use crate::services::circle_membership::{self, CirclePermission, CircleRole, MemberRoleEntry};
use crate::services::circle_notifications as circle_notifier;
use crate::services::circle_scheduler::{self, UpdateReminderPreferencesRequest};
use crate::services::circle_sharing;
use axum::http::HeaderMap;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use uuid::Uuid;

// ── Constants ─────────────────────────────────────────────────────────────────

const VALID_VISIBILITIES: &[&str] = &["public", "private", "invite_only"];
const DEFAULT_PER_PAGE: i64 = 20;
const MAX_PER_PAGE: i64 = 100;

// ── State ─────────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct CirclesState {
    pub pool: PgPool,
    /// #1225: fan-out point for real-time circle events.
    pub hub: crate::api::circle_events::CircleEventHub,
}

impl CirclesState {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            hub: crate::api::circle_events::CircleEventHub::default(),
        }
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

/// #1197 — Query parameters for circle search.
#[derive(Debug, Deserialize)]
pub struct SearchCirclesQuery {
    pub q: Option<String>,
    pub category: Option<String>,
    pub visibility: Option<String>,
    pub min_target: Option<i64>,
    pub max_target: Option<i64>,
    pub has_space: Option<bool>,
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

/// #1201 — Request body to update a member's role.
#[derive(Debug, Deserialize)]
pub struct UpdateRoleRequest {
    pub role: String,
}

/// #1198 — Review membership request body.
#[derive(Debug, Deserialize)]
pub struct ReviewMembershipRequest {
    pub approve: bool,
    pub reason: Option<String>,
}

/// Pagination query parameters.
#[derive(Debug, Deserialize)]
pub struct PaginationQuery {
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn escape_like_pattern(term: &str) -> String {
    term.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

fn err(msg: &'static str) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({ "error": msg })),
    )
}

fn not_found(msg: &'static str) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({ "error": msg })),
    )
}

fn forbidden(msg: &'static str) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::FORBIDDEN,
        Json(serde_json::json!({ "error": msg })),
    )
}

fn internal() -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({ "error": "internal_error" })),
    )
}

/// Record an immutable audit entry. A failed audit write is logged loudly but
/// does not roll back an already-committed business operation.
async fn audit(
    pool: &PgPool,
    circle_id: Uuid,
    actor: &str,
    action: &str,
    target: &str,
    details: serde_json::Value,
) {
    if let Err(e) = circle_audit::append(
        pool,
        circle_audit::AuditEvent {
            circle_id,
            actor,
            action,
            target,
            details,
        },
    )
    .await
    {
        tracing::error!(circle_id = %circle_id, action, error = %e, "Failed to append circle audit entry");
    }
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

    let visibility = req.visibility.as_deref().unwrap_or("public").to_lowercase();
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
        "INSERT INTO circle_members (circle_id, address, role) VALUES ($1, $2, 'owner') ON CONFLICT (circle_id, address) DO UPDATE SET role = 'owner'",
        id,
        auth.address
    )
    .execute(&state.pool)
    .await;

    // Record membership history
    let _ = sqlx::query!(
        r#"
        INSERT INTO circle_membership_history (circle_id, member_address, action, actor_address, new_role)
        VALUES ($1, $2, 'joined', $2, 'owner')
        "#,
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

    audit(
        &state.pool,
        id,
        &auth.address,
        "circle_created",
        &id.to_string(),
        serde_json::json!({ "visibility": visibility, "category": category }),
    )
    .await;

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
    let per_page = q
        .per_page
        .unwrap_or(DEFAULT_PER_PAGE)
        .clamp(1, MAX_PER_PAGE);
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
    let count_sql = format!("SELECT COUNT(*) FROM circles c WHERE {where_sql}");

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

/// GET /api/circles/search — #1197: Search circles by name, description, category.
///
/// Features:
/// - Supports partial matching across `name`, `description`, and `category` (using ILIKE with trigram indexes).
/// - Sanitizes and validates queries (trimming, max 100 chars, escaping LIKE meta-characters).
/// - Privacy permissions enforced:
///     * Public circles: visible to all users.
///     * Invite-only circles: visible if `is_discoverable` or caller is a member.
///     * Private circles: visible ONLY if caller is a member.
/// - Pagination: page, per_page (default 20, max 100).
/// - Empty results: returns HTTP 200 with `{ "circles": [], "total": 0, "page": page, "per_page": per_page }`.
pub async fn search_circles(
    State(state): State<CirclesState>,
    auth: AuthenticatedUser,
    Query(q): Query<SearchCirclesQuery>,
) -> impl IntoResponse {
    let per_page = q
        .per_page
        .unwrap_or(DEFAULT_PER_PAGE)
        .clamp(1, MAX_PER_PAGE);
    let page = q.page.unwrap_or(1);
    if page < 1 {
        return err("invalid_page").into_response();
    }
    let offset = (page - 1) * per_page;

    if let Some(ref vis) = q.visibility {
        if !VALID_VISIBILITIES.contains(&vis.as_str()) {
            return err("invalid_visibility").into_response();
        }
    }

    let term = q.q.as_deref().unwrap_or("").trim();
    if term.chars().count() > 100 {
        return err("query_too_long").into_response();
    }

    let escaped_term = escape_like_pattern(term).to_lowercase();
    let has_search_term = !escaped_term.is_empty();
    let pattern = if has_search_term {
        Some(format!("%{escaped_term}%"))
    } else {
        None
    };

    // Build privacy and search clauses
    // $1 = auth.address
    let mut where_clauses: Vec<String> = vec![
        r#"(
            c.visibility = 'public'
            OR (c.visibility = 'invite_only' AND c.is_discoverable = TRUE)
            OR EXISTS (SELECT 1 FROM circle_members cm WHERE cm.circle_id = c.id AND cm.address = $1)
        )"#.into()
    ];
    let mut bind_index: i32 = 1;

    let mut pattern_val: Option<String> = None;
    let mut category_val: Option<String> = None;
    let mut visibility_val: Option<String> = None;
    let mut min_target_val: Option<i64> = None;
    let mut max_target_val: Option<i64> = None;

    if let Some(pat) = pattern {
        bind_index += 1;
        where_clauses.push(format!(
            "(LOWER(c.name) LIKE ${bind_index} ESCAPE '\\' OR LOWER(COALESCE(c.description, '')) LIKE ${bind_index} ESCAPE '\\' OR LOWER(c.category) LIKE ${bind_index} ESCAPE '\\')"
        ));
        pattern_val = Some(pat);
    }

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

    if q.has_space.unwrap_or(false) {
        where_clauses.push(
            "(c.member_limit IS NULL OR (SELECT COUNT(*) FROM circle_members WHERE circle_id = c.id) < c.member_limit)".into()
        );
    }

    let where_sql = where_clauses.join(" AND ");

    let count_sql = format!("SELECT COUNT(*) FROM circles c WHERE {where_sql}");

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

    // Bind count query
    let mut count_q = sqlx::query_scalar::<_, i64>(&count_sql).bind(&auth.address);
    if let Some(ref p) = pattern_val {
        count_q = count_q.bind(p.clone());
    }
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

    // If total == 0, return valid empty result right away (O(1))
    if total == 0 {
        return Json(serde_json::json!({
            "circles": Vec::<CircleSummary>::new(),
            "total": 0,
            "page": page,
            "per_page": per_page,
        }))
        .into_response();
    }

    // Bind data query
    let mut data_q = sqlx::query_as::<_, DiscoverRow>(&data_sql).bind(&auth.address);
    if let Some(ref p) = pattern_val {
        data_q = data_q.bind(p.clone());
    }
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

    // #1225: notify authorised subscribers after the DB commit.
    state.hub.publish(
        id,
        "contribution.created",
        serde_json::json!({ "circle_id": id, "contribution_id": contribution_id, "member": auth.address }),
        false,
    );

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

    // #1225: payout amounts are sensitive; only privileged members receive it.
    state.hub.publish(
        id,
        "payout.requested",
        serde_json::json!({ "circle_id": id, "request_id": next_req_id, "amount": amount, "status": "pending" }),
        true,
    );

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

    audit(
        &state.pool,
        circle_id,
        &auth.address,
        if approved {
            "payout_approved"
        } else {
            "payout_approval_cast"
        },
        &format!("payout:{req_id}"),
        serde_json::json!({ "approvals": approval_count, "threshold": threshold }),
    )
    .await;

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

/// POST /api/circles/:id/join — #1198: Join a circle.
pub async fn join_circle(
    State(state): State<CirclesState>,
    auth: AuthenticatedUser,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    match circle_membership::join_circle(&state.pool, id, &auth.address).await {
        Ok(()) => {
            audit(
                &state.pool,
                id,
                &auth.address,
                "member_joined",
                &auth.address,
                serde_json::json!({ "role": "member" }),
            )
            .await;
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "status": "joined",
                    "circle_id": id,
                    "address": auth.address,
                    "role": "member",
                })),
            )
                .into_response()
        }
        Err("already_member") => (
            StatusCode::CONFLICT,
            Json(serde_json::json!({ "error": "already_member" })),
        )
            .into_response(),
        Err("member_limit_reached") => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "member_limit_reached" })),
        )
            .into_response(),
        Err("private_circle_direct_join_disabled") => (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "error": "private_circle_direct_join_disabled" })),
        )
            .into_response(),
        Err("invitation_required") => (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "error": "invitation_required" })),
        )
            .into_response(),
        Err("circle_not_found") => not_found("circle_not_found").into_response(),
        Err(_) => internal().into_response(),
    }
}

/// POST /api/circles/:id/leave — #1198: Leave a circle.
pub async fn leave_circle(
    State(state): State<CirclesState>,
    auth: AuthenticatedUser,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    match circle_membership::leave_circle(&state.pool, id, &auth.address).await {
        Ok(()) => {
            audit(
                &state.pool,
                id,
                &auth.address,
                "member_left",
                &auth.address,
                serde_json::json!({}),
            )
            .await;
            (
                StatusCode::OK,
                Json(serde_json::json!({ "status": "left", "circle_id": id })),
            )
                .into_response()
        }
        Err("not_member") => forbidden("not_member").into_response(),
        Err("owner_cannot_leave_must_transfer") => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "owner_cannot_leave_must_transfer" })),
        )
            .into_response(),
        Err("pending_obligations") => (
            StatusCode::CONFLICT,
            Json(serde_json::json!({ "error": "pending_obligations" })),
        )
            .into_response(),
        Err(_) => internal().into_response(),
    }
}

/// GET /api/circles/:id/eligibility — #1198: Check if caller is eligible to join.
pub async fn check_eligibility(
    State(state): State<CirclesState>,
    auth: AuthenticatedUser,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    match circle_membership::check_eligibility(&state.pool, id, &auth.address).await {
        Ok(eligibility) => Json(eligibility).into_response(),
        Err("circle_not_found") => not_found("circle_not_found").into_response(),
        Err(_) => internal().into_response(),
    }
}

/// GET /api/circles/:id/membership/history — #1198: View circle membership history.
pub async fn get_membership_history(
    State(state): State<CirclesState>,
    auth: AuthenticatedUser,
    Path(id): Path<Uuid>,
    Query(q): Query<PaginationQuery>,
) -> impl IntoResponse {
    if let Err(e) = circle_membership::require_circle_permission(
        &state.pool,
        id,
        &auth.address,
        CirclePermission::ViewFinancialInfo,
    )
    .await
    {
        return match e {
            "not_member" => forbidden("not_member").into_response(),
            _ => forbidden("permission_denied").into_response(),
        };
    }

    let page = q.page.unwrap_or(1).max(1);
    let per_page = q
        .per_page
        .unwrap_or(DEFAULT_PER_PAGE)
        .clamp(1, MAX_PER_PAGE);

    match circle_membership::get_membership_history(&state.pool, id, page, per_page).await {
        Ok((history, total)) => Json(serde_json::json!({
            "history": history,
            "total": total,
            "page": page,
            "per_page": per_page,
        }))
        .into_response(),
        Err(_) => internal().into_response(),
    }
}

/// POST /api/circles/:id/members/requests/:req_id/review — #1198: Approve or reject join request.
pub async fn review_membership_request(
    State(state): State<CirclesState>,
    auth: AuthenticatedUser,
    Path((circle_id, req_id)): Path<(Uuid, Uuid)>,
    Json(req): Json<ReviewMembershipRequest>,
) -> impl IntoResponse {
    if let Err(e) = circle_membership::require_circle_permission(
        &state.pool,
        circle_id,
        &auth.address,
        CirclePermission::ManageMembers,
    )
    .await
    {
        return match e {
            "not_member" => forbidden("not_member").into_response(),
            _ => forbidden("permission_denied").into_response(),
        };
    }

    let request = sqlx::query!(
        "SELECT requester_address, status FROM circle_membership_requests WHERE id = $1 AND circle_id = $2",
        req_id,
        circle_id
    )
    .fetch_optional(&state.pool)
    .await;

    let r = match request {
        Ok(Some(r)) => r,
        Ok(None) => return not_found("request_not_found").into_response(),
        Err(_) => return internal().into_response(),
    };

    if r.status != "pending" {
        return (
            StatusCode::CONFLICT,
            Json(serde_json::json!({ "error": "request_already_reviewed" })),
        )
            .into_response();
    }

    let mut tx = match state.pool.begin().await {
        Ok(t) => t,
        Err(_) => return internal().into_response(),
    };

    let new_status = if req.approve { "approved" } else { "rejected" };

    if req.approve {
        // Enforce membership limit
        let count: i64 = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM circle_members WHERE circle_id = $1",
            circle_id
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap_or(0);

        let limit =
            sqlx::query_scalar!("SELECT member_limit FROM circles WHERE id = $1", circle_id)
                .fetch_one(&mut *tx)
                .await
                .ok()
                .flatten();

        if let Some(limit) = limit {
            if count >= limit as i64 {
                let _ = tx.rollback().await;
                return err("member_limit_reached").into_response();
            }
        }

        let insert = sqlx::query!(
            "INSERT INTO circle_members (circle_id, address, role) VALUES ($1, $2, 'member') ON CONFLICT DO NOTHING",
            circle_id,
            r.requester_address
        )
        .execute(&mut *tx)
        .await;

        if insert.is_err() {
            let _ = tx.rollback().await;
            return internal().into_response();
        }

        let _ = sqlx::query!(
            r#"
            INSERT INTO circle_membership_history (circle_id, member_address, action, actor_address, new_role, reason)
            VALUES ($1, $2, 'approved', $3, 'member', $4)
            "#,
            circle_id,
            r.requester_address,
            auth.address,
            req.reason
        )
        .execute(&mut *tx)
        .await;
    } else {
        let _ = sqlx::query!(
            r#"
            INSERT INTO circle_membership_history (circle_id, member_address, action, actor_address, reason)
            VALUES ($1, $2, 'rejected', $3, $4)
            "#,
            circle_id,
            r.requester_address,
            auth.address,
            req.reason
        )
        .execute(&mut *tx)
        .await;
    }

    let _ = sqlx::query!(
        "UPDATE circle_membership_requests SET status = $1, reviewed_by = $2, reviewed_at = NOW() WHERE id = $3",
        new_status,
        auth.address,
        req_id
    )
    .execute(&mut *tx)
    .await;

    if tx.commit().await.is_err() {
        return internal().into_response();
    }

    audit(
        &state.pool,
        circle_id,
        &auth.address,
        if req.approve {
            "membership_approved"
        } else {
            "membership_rejected"
        },
        &format!("membership_request:{req_id}"),
        serde_json::json!({ "status": new_status }),
    )
    .await;

    Json(serde_json::json!({ "request_id": req_id, "status": new_status })).into_response()
}

/// GET /api/circles/:id/roles — #1201: List member roles.
pub async fn list_roles(
    State(state): State<CirclesState>,
    auth: AuthenticatedUser,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    if let Err(e) = circle_membership::require_circle_permission(
        &state.pool,
        id,
        &auth.address,
        CirclePermission::ViewFinancialInfo,
    )
    .await
    {
        return match e {
            "not_member" => forbidden("not_member").into_response(),
            _ => forbidden("permission_denied").into_response(),
        };
    }

    let rows = sqlx::query!(
        "SELECT address, role, joined_at, total_contributed FROM circle_members WHERE circle_id = $1 ORDER BY joined_at",
        id
    )
    .fetch_all(&state.pool)
    .await;

    match rows {
        Ok(rows) => {
            let roles: Vec<_> = rows
                .into_iter()
                .map(|r| MemberRoleEntry {
                    address: r.address,
                    role: r.role,
                    joined_at: r.joined_at,
                    total_contributed: r.total_contributed.to_string(),
                })
                .collect();
            Json(serde_json::json!({ "roles": roles })).into_response()
        }
        Err(_) => internal().into_response(),
    }
}

/// PUT /api/circles/:id/members/:address/role — #1201: Update a member's role with RBAC enforcement.
pub async fn update_member_role(
    State(state): State<CirclesState>,
    auth: AuthenticatedUser,
    Path((circle_id, address)): Path<(Uuid, String)>,
    Json(req): Json<UpdateRoleRequest>,
) -> impl IntoResponse {
    let new_role = match CircleRole::from_str(&req.role) {
        Some(r) => r,
        None => return err("invalid_role").into_response(),
    };

    match circle_membership::update_member_role(
        &state.pool,
        circle_id,
        &auth.address,
        &address,
        new_role,
    )
    .await
    {
        Ok(()) => {
            audit(
                &state.pool,
                circle_id,
                &auth.address,
                "role_changed",
                &address,
                serde_json::json!({ "new_role": new_role.as_str() }),
            )
            .await;
            Json(serde_json::json!({
                "address": address,
                "new_role": new_role.as_str(),
            }))
            .into_response()
        }
        Err("not_member") => forbidden("not_member").into_response(),
        Err("member_not_found") => not_found("member_not_found").into_response(),
        Err("cannot_modify_own_role") => (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "error": "cannot_modify_own_role" })),
        )
            .into_response(),
        Err("insufficient_permission") => forbidden("permission_denied").into_response(),
        Err("cannot_modify_owner") => (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "error": "cannot_modify_owner" })),
        )
            .into_response(),
        Err("only_owner_can_transfer_ownership") => (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "error": "only_owner_can_transfer_ownership" })),
        )
            .into_response(),
        Err("cannot_grant_role_equal_or_higher_than_own") => (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "error": "cannot_grant_role_equal_or_higher_than_own" })),
        )
            .into_response(),
        Err("cannot_modify_equal_or_higher_role") => (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "error": "cannot_modify_equal_or_higher_role" })),
        )
            .into_response(),
        Err(_) => internal().into_response(),
    }
}

/// GET /api/circles/:id/reminders/preferences — #1219: Get reminder preferences.
pub async fn get_reminder_preferences(
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

    match circle_scheduler::get_reminder_preferences(&state.pool, id, &auth.address).await {
        Ok(prefs) => Json(prefs).into_response(),
        Err(_) => internal().into_response(),
    }
}

/// PUT /api/circles/:id/reminders/preferences — #1219: Configure reminder preferences.
pub async fn update_reminder_preferences(
    State(state): State<CirclesState>,
    auth: AuthenticatedUser,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateReminderPreferencesRequest>,
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

    match circle_scheduler::update_reminder_preferences(&state.pool, id, &auth.address, req).await {
        Ok(prefs) => Json(prefs).into_response(),
        Err(_) => internal().into_response(),
    }
}

/// POST /api/circles/:id/reminders/trigger — #1219: Trigger on-demand contribution reminder evaluation.
pub async fn trigger_contribution_reminders(
    State(state): State<CirclesState>,
    auth: AuthenticatedUser,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    if let Err(e) = circle_membership::require_circle_permission(
        &state.pool,
        id,
        &auth.address,
        CirclePermission::ManageSettings,
    )
    .await
    {
        return match e {
            "not_member" => forbidden("not_member").into_response(),
            _ => forbidden("permission_denied").into_response(),
        };
    }

    match circle_scheduler::process_contribution_reminders(&state.pool, Utc::now()).await {
        Ok(metrics) => Json(metrics).into_response(),
        Err(_) => internal().into_response(),
    }
}

/// POST /api/circles/:id/members — #1198 / #1201: Add a member (permission checked).
pub async fn add_member(
    State(state): State<CirclesState>,
    auth: AuthenticatedUser,
    Path(id): Path<Uuid>,
    Json(req): Json<AddMemberRequest>,
) -> impl IntoResponse {
    if let Err(e) = circle_membership::require_circle_permission(
        &state.pool,
        id,
        &auth.address,
        CirclePermission::ManageMembers,
    )
    .await
    {
        return match e {
            "not_member" => forbidden("not_member").into_response(),
            _ => forbidden("permission_denied").into_response(),
        };
    }

    let is_already = sqlx::query_scalar!(
        "SELECT 1 FROM circle_members WHERE circle_id = $1 AND address = $2",
        id,
        req.address
    )
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None)
    .is_some();

    if is_already {
        return (
            StatusCode::CONFLICT,
            Json(serde_json::json!({ "error": "already_member" })),
        )
            .into_response();
    }

    let limit = sqlx::query_scalar!("SELECT member_limit FROM circles WHERE id = $1", id)
        .fetch_one(&state.pool)
        .await
        .ok()
        .flatten();

    if let Some(limit) = limit {
        let count: i64 = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM circle_members WHERE circle_id = $1",
            id
        )
        .fetch_one(&state.pool)
        .await
        .unwrap_or(0);

        if count >= limit as i64 {
            return err("member_limit_reached").into_response();
        }
    }

    let mut tx = match state.pool.begin().await {
        Ok(t) => t,
        Err(_) => return internal().into_response(),
    };

    let result = sqlx::query!(
        "INSERT INTO circle_members (circle_id, address, role) VALUES ($1, $2, 'member') ON CONFLICT DO NOTHING",
        id,
        req.address
    )
    .execute(&mut *tx)
    .await;

    if result.is_err() {
        let _ = tx.rollback().await;
        return internal().into_response();
    }

    let _ = sqlx::query!(
        r#"
        INSERT INTO circle_membership_history (circle_id, member_address, action, actor_address, new_role)
        VALUES ($1, $2, 'joined', $3, 'member')
        "#,
        id,
        req.address,
        auth.address
    )
    .execute(&mut *tx)
    .await;

    let _ = sqlx::query!(
        r#"
        INSERT INTO circle_activities (circle_id, actor_address, activity_type, metadata)
        VALUES ($1, $2, 'member_joined', json_build_object('target', $3::text))
        "#,
        id,
        auth.address,
        req.address
    )
    .execute(&mut *tx)
    .await;

    if tx.commit().await.is_err() {
        return internal().into_response();
    }

    audit(
        &state.pool,
        id,
        &auth.address,
        "member_added",
        &req.address,
        serde_json::json!({ "role": "member" }),
    )
    .await;

    (
        StatusCode::CREATED,
        Json(serde_json::json!({ "address": req.address, "role": "member" })),
    )
        .into_response()
}

/// DELETE /api/circles/:id/members/:address — #1198 / #1201: Remove a member or leave circle.
pub async fn remove_member(
    State(state): State<CirclesState>,
    auth: AuthenticatedUser,
    Path((circle_id, address)): Path<(Uuid, String)>,
) -> impl IntoResponse {
    let is_self = auth.address == address;
    let res = if is_self {
        circle_membership::leave_circle(&state.pool, circle_id, &address).await
    } else {
        circle_membership::remove_member_authorized(
            &state.pool,
            circle_id,
            &auth.address,
            &address,
            None,
        )
        .await
    };

    match res {
        Ok(()) => {
            audit(
                &state.pool,
                circle_id,
                &auth.address,
                if is_self {
                    "member_left"
                } else {
                    "member_removed"
                },
                &address,
                serde_json::json!({}),
            )
            .await;
            StatusCode::NO_CONTENT.into_response()
        }
        Err("not_member") => forbidden("not_member").into_response(),
        Err("permission_denied") => forbidden("permission_denied").into_response(),
        Err("member_not_found") => not_found("member_not_found").into_response(),
        Err("owner_cannot_leave_must_transfer") => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "owner_cannot_leave_must_transfer" })),
        )
            .into_response(),
        Err("cannot_remove_owner") => (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "error": "cannot_remove_owner" })),
        )
            .into_response(),
        Err("cannot_remove_equal_or_higher_role") => (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "error": "cannot_remove_equal_or_higher_role" })),
        )
            .into_response(),
        Err("pending_obligations") => (
            StatusCode::CONFLICT,
            Json(serde_json::json!({ "error": "pending_obligations" })),
        )
            .into_response(),
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

// ── #1222: Circle audit log ───────────────────────────────────────────────────

async fn require_audit_access(
    pool: &PgPool,
    circle_id: Uuid,
    actor: &str,
) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    circle_membership::require_circle_permission(
        pool,
        circle_id,
        actor,
        CirclePermission::ManageSettings,
    )
    .await
    .map(|_| ())
    .map_err(|e| match e {
        "not_member" => forbidden("not_member"),
        _ => forbidden("permission_denied"),
    })
}

/// GET /api/circles/:id/audit — admins/owners only.
pub async fn list_audit_log(
    State(state): State<CirclesState>,
    auth: AuthenticatedUser,
    Path(circle_id): Path<Uuid>,
    Query(q): Query<PaginationQuery>,
) -> impl IntoResponse {
    if let Err(response) = require_audit_access(&state.pool, circle_id, &auth.address).await {
        return response.into_response();
    }
    let per_page = q
        .per_page
        .unwrap_or(DEFAULT_PER_PAGE)
        .clamp(1, MAX_PER_PAGE);
    let page = q.page.unwrap_or(1);
    if page < 1 {
        return err("invalid_page").into_response();
    }
    match circle_audit::list(&state.pool, circle_id, per_page, (page - 1) * per_page).await {
        Ok(entries) => Json(serde_json::json!({
            "entries": entries,
            "page": page,
            "per_page": per_page,
        }))
        .into_response(),
        Err(_) => internal().into_response(),
    }
}

/// GET /api/circles/:id/audit/verify — check the hash chain for tampering.
pub async fn verify_audit_log(
    State(state): State<CirclesState>,
    auth: AuthenticatedUser,
    Path(circle_id): Path<Uuid>,
) -> impl IntoResponse {
    if let Err(response) = require_audit_access(&state.pool, circle_id, &auth.address).await {
        return response.into_response();
    }
    match circle_audit::verify_chain(&state.pool, circle_id).await {
        Ok(valid) => Json(serde_json::json!({ "valid": valid })).into_response(),
        Err(_) => internal().into_response(),
    }
}

#[derive(Debug, Deserialize)]
pub struct UpdateContributionRuleRequest {
    pub min_amount: String,
    pub max_amount: Option<String>,
    pub frequency: String,
    pub is_mandatory: bool,
}

/// PUT /api/circles/:id/contribution-rules — admin-only; writes an audit entry.
pub async fn update_contribution_rule(
    State(state): State<CirclesState>,
    auth: AuthenticatedUser,
    Path(circle_id): Path<Uuid>,
    Json(req): Json<UpdateContributionRuleRequest>,
) -> impl IntoResponse {
    if let Err(response) = require_audit_access(&state.pool, circle_id, &auth.address).await {
        return response.into_response();
    }
    let min_amount: i64 = match req.min_amount.parse() {
        Ok(v) if v >= 0 => v,
        _ => return err("invalid_amount").into_response(),
    };
    let max_amount = match req.max_amount.as_deref() {
        Some(v) => match v.parse::<i64>() {
            Ok(n) if n >= min_amount => Some(n),
            _ => return err("invalid_amount").into_response(),
        },
        None => None,
    };
    if !["none", "daily", "weekly", "monthly"].contains(&req.frequency.as_str()) {
        return err("invalid_frequency").into_response();
    }

    let existing: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM circle_contribution_rules WHERE circle_id = $1 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(circle_id)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None);

    let result = if let Some(rule_id) = existing {
        sqlx::query(
            r#"
            UPDATE circle_contribution_rules
               SET min_amount = $1, max_amount = $2, frequency = $3,
                   is_mandatory = $4, updated_at = NOW()
             WHERE id = $5
            "#,
        )
        .bind(min_amount)
        .bind(max_amount)
        .bind(&req.frequency)
        .bind(req.is_mandatory)
        .bind(rule_id)
        .execute(&state.pool)
        .await
    } else {
        sqlx::query(
            r#"
            INSERT INTO circle_contribution_rules
                (circle_id, min_amount, max_amount, frequency, is_mandatory)
            VALUES ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(circle_id)
        .bind(min_amount)
        .bind(max_amount)
        .bind(&req.frequency)
        .bind(req.is_mandatory)
        .execute(&state.pool)
        .await
    };

    if result.is_err() {
        return internal().into_response();
    }
    audit(
        &state.pool,
        circle_id,
        &auth.address,
        "contribution_rule_changed",
        &circle_id.to_string(),
        serde_json::json!({
            "min_amount": min_amount.to_string(),
            "max_amount": max_amount.map(|v| v.to_string()),
            "frequency": req.frequency,
            "is_mandatory": req.is_mandatory,
        }),
    )
    .await;
    Json(serde_json::json!({ "status": "updated" })).into_response()
}

#[derive(Debug, Deserialize)]
pub struct ExecutePayoutRequest {
    /// On-chain transaction hash (public information, safe to audit).
    pub tx_hash: String,
}

/// POST /api/circles/:id/payout/:req_id/execute — admin-only payout execution.
pub async fn execute_payout(
    State(state): State<CirclesState>,
    auth: AuthenticatedUser,
    Path((circle_id, req_id)): Path<(Uuid, i64)>,
    Json(req): Json<ExecutePayoutRequest>,
) -> impl IntoResponse {
    if let Err(response) = require_audit_access(&state.pool, circle_id, &auth.address).await {
        return response.into_response();
    }
    if req.tx_hash.trim().is_empty() || req.tx_hash.len() > 128 {
        return err("invalid_tx_hash").into_response();
    }

    let row = sqlx::query(
        "SELECT status, is_executed FROM circle_payout_requests WHERE circle_id = $1 AND request_id = $2",
    )
    .bind(circle_id)
    .bind(req_id)
    .fetch_optional(&state.pool)
    .await;

    let row = match row {
        Ok(Some(r)) => r,
        Ok(None) => return not_found("payout_not_found").into_response(),
        Err(_) => return internal().into_response(),
    };
    if row.get::<bool, _>("is_executed") {
        return err("already_executed").into_response();
    }

    let updated = sqlx::query(
        r#"
        UPDATE circle_payout_requests
           SET status = 'executed', is_executed = TRUE, tx_hash = $3
         WHERE circle_id = $1 AND request_id = $2 AND is_executed = FALSE
        "#,
    )
    .bind(circle_id)
    .bind(req_id)
    .bind(req.tx_hash.trim())
    .execute(&state.pool)
    .await;

    if updated.is_err() {
        return internal().into_response();
    }
    audit(
        &state.pool,
        circle_id,
        &auth.address,
        "payout_executed",
        &format!("payout:{req_id}"),
        serde_json::json!({ "tx_hash": req.tx_hash.trim() }),
    )
    .await;
    Json(serde_json::json!({ "status": "executed" })).into_response()
}

// ── #1221: Circle sharing and deep links ─────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CreateShareLinkRequest {
    /// `circle` (default) or `invitation`.
    pub scope: Option<String>,
    /// Optional invitation this link is tied to; must belong to the circle.
    pub invitation_id: Option<Uuid>,
    /// Link lifetime in hours (1..=720). Defaults to 168 hours (7 days).
    pub expires_in_hours: Option<i64>,
}

/// POST /api/circles/:id/share — create a secure, expiring share link.
pub async fn create_share_link(
    State(state): State<CirclesState>,
    auth: AuthenticatedUser,
    Path(circle_id): Path<Uuid>,
    Json(req): Json<CreateShareLinkRequest>,
) -> impl IntoResponse {
    let scope = req.scope.as_deref().unwrap_or("circle");
    if scope != "circle" && scope != "invitation" {
        return err("invalid_scope").into_response();
    }
    if scope == "invitation" && req.invitation_id.is_none() {
        return err("invitation_required").into_response();
    }
    let hours = req.expires_in_hours.unwrap_or(168);
    if !(1..=720).contains(&hours) {
        return err("invalid_expiry").into_response();
    }

    if let Err(e) = circle_membership::require_circle_permission(
        &state.pool,
        circle_id,
        &auth.address,
        CirclePermission::InviteMembers,
    )
    .await
    {
        return match e {
            "not_member" => forbidden("not_member").into_response(),
            _ => forbidden("permission_denied").into_response(),
        };
    }

    if let Some(invitation_id) = req.invitation_id {
        let belongs = sqlx::query_scalar!(
            "SELECT 1 FROM circle_invitations WHERE id = $1 AND circle_id = $2",
            invitation_id,
            circle_id
        )
        .fetch_optional(&state.pool)
        .await
        .unwrap_or(None)
        .is_some();
        if !belongs {
            return not_found("invitation_not_found").into_response();
        }
    }

    let token = circle_sharing::generate_token();
    let token_hash = circle_sharing::hash_token(&token);
    let id = Uuid::new_v4();
    let result = sqlx::query(
        r#"
        INSERT INTO circle_share_links
            (id, circle_id, created_by, token_hash, scope, invitation_id, expires_at)
        VALUES ($1, $2, $3, $4, $5, $6, NOW() + ($7::text || ' hours')::interval)
        "#,
    )
    .bind(id)
    .bind(circle_id)
    .bind(&auth.address)
    .bind(&token_hash)
    .bind(scope)
    .bind(req.invitation_id)
    .bind(hours.to_string())
    .execute(&state.pool)
    .await;

    match result {
        Ok(_) => {
            audit(
                &state.pool,
                circle_id,
                &auth.address,
                "share_link_created",
                &id.to_string(),
                serde_json::json!({ "scope": scope, "expires_in_hours": hours }),
            )
            .await;
            (
                StatusCode::CREATED,
                Json(serde_json::json!({
                    "id": id,
                    "token": token,
                    "scope": scope,
                    "expires_in_hours": hours,
                    "deep_link": format!("zaps://circle/{circle_id}?invite={token}"),
                    "web_fallback": format!("https://zaps.app/circle/{circle_id}?invite={token}"),
                })),
            )
                .into_response()
        }
        Err(_) => internal().into_response(),
    }
}

/// GET /api/circles/share/:token — resolve a share link into app deep links.
///
/// Private circles and invitation links require the caller to be a member or
/// the named invitee. Every resolution is logged with one of the outcomes
/// below; the raw token is never written to the log.
pub async fn resolve_share_link(
    State(state): State<CirclesState>,
    auth: AuthenticatedUser,
    Path(token): Path<String>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let token = token.trim();
    if token.len() != 64 || !token.bytes().all(|b| b.is_ascii_hexdigit()) {
        return not_found("share_link_not_found").into_response();
    }
    let token_hash = circle_sharing::hash_token(token);

    let row = match sqlx::query(
        r#"
        SELECT l.id, l.circle_id, l.scope, l.invitation_id, l.expires_at, l.revoked_at,
               c.visibility,
               i.status AS invitation_status, i.invitee_address, i.expires_at AS invitation_expires_at
        FROM circle_share_links l
        JOIN circles c ON c.id = l.circle_id
        LEFT JOIN circle_invitations i ON i.id = l.invitation_id
        WHERE l.token_hash = $1
        "#,
    )
    .bind(&token_hash)
    .fetch_optional(&state.pool)
    .await
    {
        Ok(row) => row,
        Err(_) => return internal().into_response(),
    };
    let Some(row) = row else {
        return not_found("share_link_not_found").into_response();
    };

    let link_id: Uuid = row.get("id");
    let user_agent = headers
        .get(axum::http::header::USER_AGENT)
        .and_then(|v| v.to_str().ok());
    let ip_address = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok());

    let deny = |outcome: &'static str, error: &'static str| async {
        let _ = circle_sharing::log_access(
            &state.pool,
            link_id,
            Some(&auth.address),
            user_agent,
            ip_address,
            outcome,
        )
        .await;
        (
            if outcome == "denied_expired" {
                StatusCode::GONE
            } else {
                StatusCode::FORBIDDEN
            },
            Json(serde_json::json!({ "error": error })),
        )
            .into_response()
    };

    if row
        .get::<Option<chrono::DateTime<Utc>>, _>("revoked_at")
        .is_some()
    {
        return deny("denied_revoked", "share_link_revoked").await;
    }
    if row
        .get::<Option<chrono::DateTime<Utc>>, _>("expires_at")
        .map(|exp| exp <= Utc::now())
        .unwrap_or(false)
    {
        return deny("denied_expired", "share_link_expired").await;
    }

    let visibility: String = row.get("visibility");
    let invitation_id: Option<Uuid> = row.get("invitation_id");
    let is_member = sqlx::query_scalar!(
        "SELECT 1 FROM circle_members WHERE circle_id = $1 AND address = $2",
        row.get::<Uuid, _>("circle_id"),
        auth.address
    )
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None)
    .is_some();

    let mut is_invitee = false;
    if let Some(invitation_id) = invitation_id {
        let status: Option<String> = row.get("invitation_status");
        let invitee: Option<String> = row.get("invitee_address");
        let invitation_expires: Option<chrono::DateTime<Utc>> = row.get("invitation_expires_at");
        is_invitee = invitee.as_deref() == Some(auth.address.as_str())
            && status.as_deref() == Some("pending")
            && invitation_expires.map(|e| e > Utc::now()).unwrap_or(true);
        if !is_invitee {
            return deny("denied_unauthorized", "invitation_invalid").await;
        }
    }

    if visibility == "private" && !is_member && !is_invitee {
        return deny("denied_unauthorized", "authorization_required").await;
    }

    if circle_sharing::log_access(
        &state.pool,
        link_id,
        Some(&auth.address),
        user_agent,
        ip_address,
        "allowed",
    )
    .await
    .is_err()
    {
        tracing::warn!(link_id = %link_id, "Failed to log circle share link access");
    }

    let circle_id: Uuid = row.get("circle_id");
    Json(serde_json::json!({
        "circle_id": circle_id,
        "scope": row.get::<String, _>("scope"),
        "deep_link": format!("zaps://circle/{circle_id}?invite={token}"),
        "web_fallback": format!("https://zaps.app/circle/{circle_id}?invite={token}"),
        "install_link": "https://zaps.app/download",
        "expires_at": row.get::<Option<chrono::DateTime<Utc>>, _>("expires_at"),
    }))
    .into_response()
}

/// POST /api/circles/share/:id/revoke — creator or circle admin can revoke.
pub async fn revoke_share_link(
    State(state): State<CirclesState>,
    auth: AuthenticatedUser,
    Path(link_id): Path<Uuid>,
) -> impl IntoResponse {
    let link = sqlx::query("SELECT circle_id, created_by FROM circle_share_links WHERE id = $1")
        .bind(link_id)
        .fetch_optional(&state.pool)
        .await;

    let link = match link {
        Ok(Some(l)) => l,
        Ok(None) => return not_found("share_link_not_found").into_response(),
        Err(_) => return internal().into_response(),
    };

    let is_creator = link.get::<String, _>("created_by") == auth.address;
    if !is_creator {
        if let Err(_) = circle_membership::require_circle_permission(
            &state.pool,
            link.get("circle_id"),
            &auth.address,
            CirclePermission::ManageMembers,
        )
        .await
        {
            return forbidden("permission_denied").into_response();
        }
    }

    let circle_id: Uuid = link.get("circle_id");
    match sqlx::query(
        "UPDATE circle_share_links SET revoked_at = NOW() WHERE id = $1 AND revoked_at IS NULL",
    )
    .bind(link_id)
    .execute(&state.pool)
    .await
    {
        Ok(_) => {
            audit(
                &state.pool,
                circle_id,
                &auth.address,
                "share_link_revoked",
                &link_id.to_string(),
                serde_json::json!({}),
            )
            .await;
            StatusCode::NO_CONTENT.into_response()
        }
        Err(_) => internal().into_response(),
    }
}

// ── #1220: Circle member search ──────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CircleUserSearchQuery {
    /// Circle the caller is inviting into; membership is required.
    pub circle_id: Uuid,
    /// Username prefix, Stellar address prefix, or approved identifier.
    pub q: String,
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct CircleUserSearchResult {
    pub username: String,
    pub address: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
}

/// GET /api/circles/users/search — find users to invite to a circle.
///
/// Only circle members may search. Matches usernames, Stellar addresses and
/// verified identifiers (stored hashed), respects each user's search privacy
/// setting, and returns no account secrets or contact details.
pub async fn search_users_for_circle(
    State(state): State<CirclesState>,
    auth: AuthenticatedUser,
    Query(q): Query<CircleUserSearchQuery>,
) -> impl IntoResponse {
    let per_page = q
        .per_page
        .unwrap_or(DEFAULT_PER_PAGE)
        .clamp(1, MAX_PER_PAGE);
    let page = q.page.unwrap_or(1);
    if page < 1 {
        return err("invalid_page").into_response();
    }
    let term = q.q.trim();
    if term.is_empty() || term.chars().count() > 64 {
        return err("invalid_query").into_response();
    }
    let offset = (page - 1) * per_page;

    let is_member = sqlx::query_scalar!(
        "SELECT 1 FROM circle_members WHERE circle_id = $1 AND address = $2",
        q.circle_id,
        auth.address
    )
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None)
    .is_some();
    if !is_member {
        return forbidden("not_member").into_response();
    }

    // Approved identifiers are looked up by SHA-256 of their normalized
    // value; the raw value is never stored or logged.
    use sha2::{Digest, Sha256};
    let normalized = term.to_lowercase();
    let identifier_hash = hex::encode(Sha256::digest(normalized.as_bytes()));
    let pattern = format!("{}%", escape_like_pattern(&normalized));
    let address_pattern = format!("{}%", escape_like_pattern(term));

    let base = r#"
        FROM users u
        WHERE u.circle_search_enabled = TRUE
          AND u.address <> $4
          AND (
                LOWER(u.username) LIKE $1 ESCAPE '\'
             OR u.address LIKE $2 ESCAPE '\'
             OR EXISTS (
                  SELECT 1 FROM user_verified_identifiers vi
                   WHERE vi.user_id = u.id AND vi.is_verified = TRUE
                     AND vi.identifier_hash = $3
             )
          )
    "#;

    let total: i64 = match sqlx::query_scalar(&format!("SELECT COUNT(*) {base}"))
        .bind(&pattern)
        .bind(&address_pattern)
        .bind(&identifier_hash)
        .bind(&auth.address)
        .fetch_one(&state.pool)
        .await
    {
        Ok(n) => n,
        Err(_) => return internal().into_response(),
    };

    let rows = match sqlx::query(&format!(
        r#"
        SELECT u.username, u.address, u.display_name, u.avatar_url
        {base}
        ORDER BY u.username ASC
        LIMIT $5 OFFSET $6
        "#
    ))
    .bind(&pattern)
    .bind(&address_pattern)
    .bind(&identifier_hash)
    .bind(&auth.address)
    .bind(per_page)
    .bind(offset)
    .fetch_all(&state.pool)
    .await
    {
        Ok(rows) => rows,
        Err(_) => return internal().into_response(),
    };

    let users: Vec<CircleUserSearchResult> = rows
        .iter()
        .map(|r| CircleUserSearchResult {
            username: r.get("username"),
            address: r.get("address"),
            display_name: r.get("display_name"),
            avatar_url: r.get("avatar_url"),
        })
        .collect();

    Json(serde_json::json!({
        "users": users,
        "total": total,
        "page": page,
        "per_page": per_page,
    }))
    .into_response()
}

// ── #1217: Circle notifications ──────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct NotificationListQuery {
    pub unread_only: Option<bool>,
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

/// GET /api/circles/notifications — the authenticated user's notification inbox.
pub async fn list_notifications(
    State(state): State<CirclesState>,
    auth: AuthenticatedUser,
    Query(q): Query<NotificationListQuery>,
) -> impl IntoResponse {
    let per_page = q
        .per_page
        .unwrap_or(DEFAULT_PER_PAGE)
        .clamp(1, MAX_PER_PAGE);
    let page = q.page.unwrap_or(1);
    if page < 1 {
        return err("invalid_page").into_response();
    }
    let offset = (page - 1) * per_page;

    match circle_notifier::list(
        &state.pool,
        &auth.address,
        q.unread_only.unwrap_or(false),
        per_page,
        offset,
    )
    .await
    {
        Ok(notifications) => Json(serde_json::json!({
            "notifications": notifications,
            "page": page,
            "per_page": per_page,
        }))
        .into_response(),
        Err(_) => internal().into_response(),
    }
}

/// GET /api/circles/notifications/unread-count
pub async fn notification_unread_count(
    State(state): State<CirclesState>,
    auth: AuthenticatedUser,
) -> impl IntoResponse {
    match circle_notifier::unread_count(&state.pool, &auth.address).await {
        Ok(count) => Json(serde_json::json!({ "unread_count": count })).into_response(),
        Err(_) => internal().into_response(),
    }
}

/// POST /api/circles/notifications/:id/read — recipients only.
pub async fn mark_notification_read(
    State(state): State<CirclesState>,
    auth: AuthenticatedUser,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    match circle_notifier::mark_read(&state.pool, &auth.address, id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => not_found("notification_not_found").into_response(),
        Err(_) => internal().into_response(),
    }
}

/// POST /api/circles/notifications/read-all
pub async fn mark_all_notifications_read(
    State(state): State<CirclesState>,
    auth: AuthenticatedUser,
) -> impl IntoResponse {
    match circle_notifier::mark_all_read(&state.pool, &auth.address).await {
        Ok(updated) => Json(serde_json::json!({ "updated": updated })).into_response(),
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
        let _ = std::mem::size_of::<SearchCirclesQuery>();
        let _ = std::mem::size_of::<UpdateRoleRequest>();
        let _ = std::mem::size_of::<ReviewMembershipRequest>();
        let _ = std::mem::size_of::<PaginationQuery>();
    }

    #[test]
    fn test_escape_like_pattern() {
        assert_eq!(escape_like_pattern("simple"), "simple");
        assert_eq!(escape_like_pattern("100% savings"), "100\\% savings");
        assert_eq!(escape_like_pattern("user_circle"), "user\\_circle");
        assert_eq!(escape_like_pattern("path\\name"), "path\\\\name");
    }
}
