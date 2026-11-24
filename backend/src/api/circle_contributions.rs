//! Circle contribution endpoints: rules (#1203), initiation (#1205),
//! on-chain verification (#1207) and history (#1208).
//!
//! | Method | Path                                          | Who                          |
//! |--------|-----------------------------------------------|------------------------------|
//! | GET    | /api/circles/:id/contribution-rules           | members                      |
//! | PUT    | /api/circles/:id/contribution-rules           | owner / admin (new version)  |
//! | GET    | /api/circles/:id/contribution-rules/versions  | members                      |
//! | POST   | /api/circles/:id/contributions                | members                      |
//! | GET    | /api/circles/:id/contributions                | members (see below)          |
//! | POST   | /api/circles/:id/contributions/:cid/verify    | the contributor, owner/admin |
//!
//! Flow: `POST /contributions` returns the payment instructions (destination,
//! asset, amount and a unique text memo). After paying on Stellar the member
//! calls `POST /contributions/:cid/verify` with the transaction hash; only a
//! verified payment is credited to the circle. Errors use the circles
//! convention: `{ "error": "<code>" }`.

use crate::api::auth_middleware::AuthenticatedUser;
use crate::api::circles::{audit, CirclesState};
use crate::services::circle_contribution_rules::{self as rules, RuleSpec};
use crate::services::circle_contribution_verification as verification;
use crate::services::circle_contributions::{
    self as contributions, HistoryFilter, HistoryQuery, InitiateError, InitiateRequest,
};
use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use chrono::Utc;
use serde::Deserialize;
use uuid::Uuid;

fn error_response(code: &'static str) -> Response {
    let status = match code {
        "circle_not_found" | "contribution_not_found" => StatusCode::NOT_FOUND,
        "not_member" | "permission_denied" => StatusCode::FORBIDDEN,
        "duplicate_contribution"
        | "tx_already_used"
        | "contribution_closed"
        | "circle_not_accepting_contributions" => StatusCode::CONFLICT,
        "internal_error" => StatusCode::INTERNAL_SERVER_ERROR,
        _ => StatusCode::BAD_REQUEST,
    };
    (status, Json(serde_json::json!({ "error": code }))).into_response()
}

// ── Rules (#1203) ─────────────────────────────────────────────────────────────

/// GET /api/circles/:id/contribution-rules — the current rule and what it
/// means for the caller. Other members' overrides are visible to owners and
/// admins only.
pub async fn get_contribution_rules(
    State(state): State<CirclesState>,
    Extension(auth): Extension<AuthenticatedUser>,
    Path(id): Path<Uuid>,
) -> Response {
    match rules::get_rules(&state.pool, id, &auth.address, Utc::now()).await {
        Ok(view) => Json(view).into_response(),
        Err(code) => error_response(code),
    }
}

/// PUT /api/circles/:id/contribution-rules — owner / admin. Stores the rule as
/// a new version; existing contributions keep the version they were created
/// under. Sending the current rule again changes nothing.
pub async fn update_contribution_rules(
    State(state): State<CirclesState>,
    Extension(auth): Extension<AuthenticatedUser>,
    Path(id): Path<Uuid>,
    Json(spec): Json<RuleSpec>,
) -> Response {
    match rules::update_rules(&state.pool, id, &auth.address, spec).await {
        Ok((stored, changed)) => {
            if changed {
                audit(
                    &state.pool,
                    id,
                    &auth.address,
                    "contribution_rule_changed",
                    &id.to_string(),
                    serde_json::json!({ "version": stored.version }),
                )
                .await;
            }
            Json(serde_json::json!({
                "status": if changed { "updated" } else { "unchanged" },
                "version": stored.version,
                "rule": stored.rule,
            }))
            .into_response()
        }
        Err(code) => error_response(code),
    }
}

/// GET /api/circles/:id/contribution-rules/versions — every version, newest first.
pub async fn list_contribution_rule_versions(
    State(state): State<CirclesState>,
    Extension(auth): Extension<AuthenticatedUser>,
    Path(id): Path<Uuid>,
) -> Response {
    match rules::list_versions(&state.pool, id, &auth.address).await {
        Ok(versions) => Json(serde_json::json!({ "versions": versions })).into_response(),
        Err(code) => error_response(code),
    }
}

// ── Initiation (#1205) ────────────────────────────────────────────────────────

/// POST /api/circles/:id/contributions — validate and record a pending
/// contribution and return the payment instructions. Send an
/// `Idempotency-Key` header to make client retries return the original answer.
pub async fn initiate_contribution(
    State(state): State<CirclesState>,
    Extension(auth): Extension<AuthenticatedUser>,
    Path(id): Path<Uuid>,
    Json(req): Json<InitiateRequest>,
) -> Response {
    match contributions::initiate(&state.pool, id, &auth.address, req, Utc::now()).await {
        Ok(initiated) => {
            audit(
                &state.pool,
                id,
                &auth.address,
                "contribution_initiated",
                &initiated.contribution.id.to_string(),
                serde_json::json!({
                    "amount": initiated.contribution.amount,
                    "asset": initiated.contribution.asset,
                    "rule_version": initiated.contribution.rule_version,
                }),
            )
            .await;
            (StatusCode::CREATED, Json(initiated)).into_response()
        }
        Err(InitiateError::Duplicate(existing)) => (
            StatusCode::CONFLICT,
            Json(serde_json::json!({
                "error": "duplicate_contribution",
                "contribution": existing,
            })),
        )
            .into_response(),
        Err(InitiateError::Rejected(code)) => error_response(code),
    }
}

// ── Verification (#1207) ──────────────────────────────────────────────────────

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerifyRequest {
    /// Hash of the Stellar transaction that paid the contribution. Optional
    /// when re-checking a contribution whose hash was already submitted.
    #[serde(default)]
    pub tx_hash: Option<String>,
}

/// POST /api/circles/:id/contributions/:cid/verify — check the payment on
/// Stellar. Only the contributing member can send a `tx_hash`; owners and
/// admins can re-check the hash on file. The response `outcome` is `confirmed`,
/// `failed` (the hash on file does not pay the contribution, which is now
/// closed), `mismatch` (the supplied hash does not pay it: nothing is stored
/// and the contribution stays open), `not_found` (the network does not know it
/// yet: try again later) or `unavailable`.
pub async fn verify_contribution(
    State(state): State<CirclesState>,
    Extension(auth): Extension<AuthenticatedUser>,
    Path((id, contribution_id)): Path<(Uuid, Uuid)>,
    Json(req): Json<VerifyRequest>,
) -> Response {
    let result = verification::verify_contribution(
        &state.pool,
        state.chain.as_ref(),
        id,
        contribution_id,
        &auth.address,
        req.tx_hash.as_deref(),
    )
    .await;
    match result {
        Ok(result) => {
            match result.outcome {
                "confirmed" if result.newly_confirmed => {
                    audit(
                        &state.pool,
                        id,
                        &auth.address,
                        "contribution_confirmed",
                        &contribution_id.to_string(),
                        serde_json::json!({ "amount": result.contribution.amount }),
                    )
                    .await;
                    // Authorised subscribers learn about the credit only now.
                    state.hub.publish(
                        id,
                        "contribution.confirmed",
                        serde_json::json!({
                            "circle_id": id,
                            "contribution_id": contribution_id,
                            "member": result.contribution.member_address,
                        }),
                        false,
                    );
                }
                "failed" => {
                    audit(
                        &state.pool,
                        id,
                        &auth.address,
                        "contribution_failed",
                        &contribution_id.to_string(),
                        serde_json::json!({ "reason": result.reason }),
                    )
                    .await;
                }
                _ => {}
            }
            Json(result).into_response()
        }
        Err(code) => error_response(code),
    }
}

// ── History (#1208) ───────────────────────────────────────────────────────────

/// GET /api/circles/:id/contributions — filters: `member`, `from`, `to`
/// (RFC 3339), `asset`, `status`, plus `page` / `per_page`. Members see all
/// confirmed contributions and their own records of any status; owners and
/// admins see everything.
pub async fn list_contributions(
    State(state): State<CirclesState>,
    Extension(auth): Extension<AuthenticatedUser>,
    Path(id): Path<Uuid>,
    Query(query): Query<HistoryQuery>,
) -> Response {
    let filter = match HistoryFilter::from_query(query) {
        Ok(filter) => filter,
        Err(code) => return error_response(code),
    };
    match contributions::list_history(&state.pool, id, &auth.address, &filter).await {
        Ok(page) => Json(page).into_response(),
        Err(code) => error_response(code),
    }
}
