//! #1199 / #1200 — Circle invitation endpoints.
//!
//! | Method | Path                                        | Description                                  |
//! |--------|---------------------------------------------|----------------------------------------------|
//! | POST   | /api/circles/:id/invitations                | Create a direct or link invitation           |
//! | GET    | /api/circles/:id/invitations                | List a circle's invitations (`?status=`)     |
//! | DELETE | /api/circles/:id/invitations/:inv_id        | Revoke a pending invitation                  |
//! | GET    | /api/circles/invitations/mine               | Caller's pending direct invitations          |
//! | POST   | /api/circles/invitations/:inv_id/accept     | Accept a direct invitation                   |
//! | POST   | /api/circles/invitations/:inv_id/decline    | Decline a direct invitation                  |
//! | POST   | /api/circles/invitations/redeem             | Accept a link invitation (`{"token": "…"}`)  |
//!
//! Link tokens travel in the request body rather than the URL so they do not
//! end up in access logs.

use crate::api::auth_middleware::AuthenticatedUser;
use crate::api::circles::{audit, CirclesState, DEFAULT_PER_PAGE, MAX_PER_PAGE};
use crate::services::circle_invitations::{
    self as invitations, InvitationStatus, InvitationTarget, NewInvitation, ResponseAction,
    ResponseOutcome,
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct CreateInvitationRequest {
    /// Zaps user identifier to invite. Omit to create a shareable link.
    pub invitee: Option<String>,
    /// Lifetime in hours (1..=720). Defaults to the circle setting.
    pub expires_in_hours: Option<i64>,
    /// Link invitations only: maximum number of redemptions.
    pub max_uses: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct ListInvitationsQuery {
    pub status: Option<String>,
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct RedeemInvitationRequest {
    pub token: String,
}

/// Map service error codes to HTTP responses.
pub fn error_response(code: &'static str) -> Response {
    let status = match code {
        "circle_not_found" | "invitation_not_found" => StatusCode::NOT_FOUND,
        "not_member" | "permission_denied" | "not_invitation_recipient" => StatusCode::FORBIDDEN,
        "already_member"
        | "invitation_already_accepted"
        | "invitation_already_declined"
        | "invitation_not_pending"
        | "invitation_exhausted"
        | "member_limit_reached" => StatusCode::CONFLICT,
        "invitation_expired" | "invitation_revoked" => StatusCode::GONE,
        "internal_error" => StatusCode::INTERNAL_SERVER_ERROR,
        _ => StatusCode::BAD_REQUEST,
    };
    (status, Json(serde_json::json!({ "error": code }))).into_response()
}

/// POST /api/circles/:id/invitations
pub async fn create_invitation(
    State(state): State<CirclesState>,
    auth: AuthenticatedUser,
    Path(circle_id): Path<Uuid>,
    Json(req): Json<CreateInvitationRequest>,
) -> Response {
    let input = NewInvitation {
        invitee: req.invitee,
        expires_in_hours: req.expires_in_hours,
        max_uses: req.max_uses,
    };

    match invitations::create_invitation(&state.pool, circle_id, &auth.address, input).await {
        Ok(created) => {
            let inv = &created.invitation;
            if !created.reused_existing {
                audit(
                    &state.pool,
                    circle_id,
                    &auth.address,
                    "invitation_created",
                    &inv.id.to_string(),
                    serde_json::json!({
                        "invite_type": inv.invite_type,
                        "invitee": inv.invitee_address,
                        "max_uses": inv.max_uses,
                        "expires_at": inv.expires_at,
                    }),
                )
                .await;
            }
            let status = if created.reused_existing {
                StatusCode::OK
            } else {
                StatusCode::CREATED
            };
            let mut body = serde_json::to_value(&created).unwrap_or_default();
            if let Some(token) = created.token.as_deref() {
                body["deep_link"] = format!("zaps://circle/{circle_id}?invitation={token}").into();
                body["web_fallback"] =
                    format!("https://zaps.app/circle/{circle_id}?invitation={token}").into();
            }
            (status, Json(body)).into_response()
        }
        Err(e) => error_response(e),
    }
}

/// GET /api/circles/:id/invitations
pub async fn list_invitations(
    State(state): State<CirclesState>,
    auth: AuthenticatedUser,
    Path(circle_id): Path<Uuid>,
    Query(q): Query<ListInvitationsQuery>,
) -> Response {
    let status = match q.status.as_deref() {
        None => None,
        Some(s) => match InvitationStatus::parse(s) {
            Some(s) => Some(s),
            None => return error_response("invalid_status"),
        },
    };
    let page = q.page.unwrap_or(1);
    let per_page = q.per_page.unwrap_or(DEFAULT_PER_PAGE);
    if page < 1 || !(1..=MAX_PER_PAGE).contains(&per_page) {
        return error_response("invalid_page");
    }

    match invitations::list_circle_invitations(
        &state.pool,
        circle_id,
        &auth.address,
        status,
        page,
        per_page,
    )
    .await
    {
        Ok((items, total)) => Json(serde_json::json!({
            "invitations": items,
            "total": total,
            "page": page,
            "per_page": per_page,
        }))
        .into_response(),
        Err(e) => error_response(e),
    }
}

/// DELETE /api/circles/:id/invitations/:inv_id
pub async fn revoke_invitation(
    State(state): State<CirclesState>,
    auth: AuthenticatedUser,
    Path((circle_id, invitation_id)): Path<(Uuid, Uuid)>,
) -> Response {
    match invitations::revoke_invitation(&state.pool, circle_id, invitation_id, &auth.address).await
    {
        Ok(()) => {
            audit(
                &state.pool,
                circle_id,
                &auth.address,
                "invitation_revoked",
                &invitation_id.to_string(),
                serde_json::json!({}),
            )
            .await;
            StatusCode::NO_CONTENT.into_response()
        }
        Err(e) => error_response(e),
    }
}

/// GET /api/circles/invitations/mine
pub async fn my_invitations(
    State(state): State<CirclesState>,
    auth: AuthenticatedUser,
) -> Response {
    match invitations::list_pending_for(&state.pool, &auth.address).await {
        Ok(items) => Json(serde_json::json!({ "invitations": items })).into_response(),
        Err(e) => error_response(e),
    }
}

async fn respond(
    state: &CirclesState,
    auth: &AuthenticatedUser,
    target: InvitationTarget<'_>,
    action: ResponseAction,
) -> Response {
    match invitations::respond(&state.pool, target, &auth.address, action).await {
        Ok(outcome) => {
            if !outcome.already_processed {
                record_response(state, auth, &outcome).await;
            }
            Json(outcome).into_response()
        }
        Err(e) => error_response(e),
    }
}

async fn record_response(
    state: &CirclesState,
    auth: &AuthenticatedUser,
    outcome: &ResponseOutcome,
) {
    let action = match outcome.status {
        InvitationStatus::Accepted => "invitation_accepted",
        _ => "invitation_declined",
    };
    audit(
        &state.pool,
        outcome.circle_id,
        &auth.address,
        action,
        &outcome.invitation_id.to_string(),
        serde_json::json!({ "membership_created": outcome.membership_created }),
    )
    .await;
    if outcome.membership_created {
        audit(
            &state.pool,
            outcome.circle_id,
            &auth.address,
            "member_joined",
            &auth.address,
            serde_json::json!({ "role": "member", "via": "invitation" }),
        )
        .await;
    }
}

/// POST /api/circles/invitations/:inv_id/accept
pub async fn accept_invitation(
    State(state): State<CirclesState>,
    auth: AuthenticatedUser,
    Path(invitation_id): Path<Uuid>,
) -> Response {
    respond(
        &state,
        &auth,
        InvitationTarget::Id(invitation_id),
        ResponseAction::Accept,
    )
    .await
}

/// POST /api/circles/invitations/:inv_id/decline
pub async fn decline_invitation(
    State(state): State<CirclesState>,
    auth: AuthenticatedUser,
    Path(invitation_id): Path<Uuid>,
) -> Response {
    respond(
        &state,
        &auth,
        InvitationTarget::Id(invitation_id),
        ResponseAction::Decline,
    )
    .await
}

/// POST /api/circles/invitations/redeem
pub async fn redeem_invitation(
    State(state): State<CirclesState>,
    auth: AuthenticatedUser,
    Json(req): Json<RedeemInvitationRequest>,
) -> Response {
    respond(
        &state,
        &auth,
        InvitationTarget::Token(&req.token),
        ResponseAction::Accept,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_codes_map_to_expected_statuses() {
        assert_eq!(
            error_response("invitation_not_found").status(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            error_response("not_invitation_recipient").status(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            error_response("invitation_expired").status(),
            StatusCode::GONE
        );
        assert_eq!(
            error_response("member_limit_reached").status(),
            StatusCode::CONFLICT
        );
        assert_eq!(
            error_response("invalid_expiry").status(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            error_response("internal_error").status(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }
}
