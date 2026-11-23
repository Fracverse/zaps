//! Circle contribution endpoints: rules (#1203).
//!
//! | Method | Path                                          | Who                          |
//! |--------|-----------------------------------------------|------------------------------|
//! | GET    | /api/circles/:id/contribution-rules           | members                      |
//! | PUT    | /api/circles/:id/contribution-rules           | owner / admin (new version)  |
//! | GET    | /api/circles/:id/contribution-rules/versions  | members                      |
//!
//! Errors use the circles convention: `{ "error": "<code>" }`.

use crate::api::auth_middleware::AuthenticatedUser;
use crate::api::circles::{audit, CirclesState};
use crate::services::circle_contribution_rules::{self as rules, RuleSpec};
use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use chrono::Utc;
use uuid::Uuid;

fn error_response(code: &'static str) -> Response {
    let status = match code {
        "not_member" | "permission_denied" => StatusCode::FORBIDDEN,
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
