use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;
use std::sync::Arc;
use uuid::Uuid;

use crate::{
    api_error::ApiError,
    middleware::auth::AuthenticatedUser,
    models::{DisputeEvidence, PaymentDispute},
    role::Role,
    service::{
        dispute_service::{
            AddEvidenceRequest, DisputeListResponse, DisputeQueryParams, FileDisputeRequest,
            UpdateDisputeStatusRequest,
        },
        ServiceContainer,
    },
};

// ---------------------------------------------------------------------------
// Query param helpers
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct PaginationParams {
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

fn default_limit() -> i64 {
    50
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// POST /api/v2/payments/:payment_id/disputes
///
/// File a new dispute for a completed payment.
/// The authenticated user must be the payment's originator.
pub async fn file_dispute(
    State(services): State<Arc<ServiceContainer>>,
    auth_user: AuthenticatedUser,
    Path(payment_id): Path<String>,
    Json(request): Json<FileDisputeRequest>,
) -> Result<Json<PaymentDispute>, ApiError> {
    let payment_uuid = Uuid::parse_str(&payment_id)
        .map_err(|_| ApiError::Validation("Invalid payment ID".to_string()))?;

    if request.description.trim().is_empty() {
        return Err(ApiError::Validation(
            "description must not be empty".to_string(),
        ));
    }

    let dispute = services
        .dispute
        .file_dispute(payment_uuid, &auth_user.user_id, request)
        .await?;

    // Invalidate the cached payment so the next GET reflects the active dispute
    let _ = services
        .cache
        .invalidate_payment(&payment_id)
        .await;

    Ok(Json(dispute))
}

/// GET /api/v2/payments/:payment_id/disputes
///
/// List all disputes for a specific payment.
/// Users can only see their own disputes; admins see all.
pub async fn list_payment_disputes(
    State(services): State<Arc<ServiceContainer>>,
    auth_user: AuthenticatedUser,
    Path(payment_id): Path<String>,
    Query(pagination): Query<PaginationParams>,
) -> Result<Json<DisputeListResponse>, ApiError> {
    let params = DisputeQueryParams {
        status: None,
        payment_id: Some(payment_id),
        limit: pagination.limit,
        offset: pagination.offset,
    };

    let result = if auth_user.role == Role::Admin {
        services.dispute.list_disputes(&params).await?
    } else {
        // Non-admins only see their own disputes
        services
            .dispute
            .list_user_disputes(&auth_user.user_id, pagination.limit, pagination.offset)
            .await?
    };

    Ok(Json(result))
}

/// GET /api/v2/disputes/:dispute_id
///
/// Get a single dispute by ID.
pub async fn get_dispute(
    State(services): State<Arc<ServiceContainer>>,
    auth_user: AuthenticatedUser,
    Path(dispute_id): Path<String>,
) -> Result<Json<PaymentDispute>, ApiError> {
    let dispute_uuid = Uuid::parse_str(&dispute_id)
        .map_err(|_| ApiError::Validation("Invalid dispute ID".to_string()))?;

    // Try cache first
    let cache_key = format!("dispute:{}", dispute_uuid);
    let dispute: PaymentDispute = match services.cache.get_json(&cache_key).await? {
        Some(d) => d,
        None => {
            let d = services.dispute.get_dispute(dispute_uuid).await?;
            let _ = services.cache.set_json(&cache_key, &d, Some(60)).await;
            d
        }
    };

    // Non-admins can only view their own disputes
    if auth_user.role != Role::Admin && dispute.filed_by_user_id != auth_user.user_id {
        return Err(ApiError::Authorization(
            "You are not authorised to view this dispute".to_string(),
        ));
    }

    Ok(Json(dispute))
}

/// GET /api/v2/disputes
///
/// List all disputes (admin only) with optional filters.
pub async fn list_all_disputes(
    State(services): State<Arc<ServiceContainer>>,
    auth_user: AuthenticatedUser,
    Query(params): Query<DisputeQueryParams>,
) -> Result<Json<DisputeListResponse>, ApiError> {
    if auth_user.role != Role::Admin {
        return Err(ApiError::Authorization(
            "Admin access required".to_string(),
        ));
    }

    let result = services.dispute.list_disputes(&params).await?;
    Ok(Json(result))
}

/// GET /api/v2/disputes/me
///
/// List disputes filed by the authenticated user.
pub async fn list_my_disputes(
    State(services): State<Arc<ServiceContainer>>,
    auth_user: AuthenticatedUser,
    Query(pagination): Query<PaginationParams>,
) -> Result<Json<DisputeListResponse>, ApiError> {
    let result = services
        .dispute
        .list_user_disputes(&auth_user.user_id, pagination.limit, pagination.offset)
        .await?;
    Ok(Json(result))
}

/// PATCH /api/v2/disputes/:dispute_id/status
///
/// Update the status of a dispute (admin only).
pub async fn update_dispute_status(
    State(services): State<Arc<ServiceContainer>>,
    auth_user: AuthenticatedUser,
    Path(dispute_id): Path<String>,
    Json(request): Json<UpdateDisputeStatusRequest>,
) -> Result<Json<PaymentDispute>, ApiError> {
    if auth_user.role != Role::Admin {
        return Err(ApiError::Authorization(
            "Admin access required to update dispute status".to_string(),
        ));
    }

    let dispute_uuid = Uuid::parse_str(&dispute_id)
        .map_err(|_| ApiError::Validation("Invalid dispute ID".to_string()))?;

    let dispute = services
        .dispute
        .update_dispute_status(dispute_uuid, &auth_user.user_id, request)
        .await?;

    // Invalidate cached dispute
    let _ = services
        .cache
        .invalidate(&format!("dispute:{}", dispute_uuid))
        .await;

    Ok(Json(dispute))
}

/// POST /api/v2/disputes/:dispute_id/evidence
///
/// Add evidence to an existing dispute.
pub async fn add_evidence(
    State(services): State<Arc<ServiceContainer>>,
    auth_user: AuthenticatedUser,
    Path(dispute_id): Path<String>,
    Json(request): Json<AddEvidenceRequest>,
) -> Result<Json<DisputeEvidence>, ApiError> {
    let dispute_uuid = Uuid::parse_str(&dispute_id)
        .map_err(|_| ApiError::Validation("Invalid dispute ID".to_string()))?;

    if request.description.trim().is_empty() {
        return Err(ApiError::Validation(
            "description must not be empty".to_string(),
        ));
    }

    // Verify the user owns the dispute (or is admin)
    if auth_user.role != Role::Admin {
        let dispute = services.dispute.get_dispute(dispute_uuid).await?;
        if dispute.filed_by_user_id != auth_user.user_id {
            return Err(ApiError::Authorization(
                "You are not authorised to add evidence to this dispute".to_string(),
            ));
        }
    }

    let evidence = services
        .dispute
        .add_evidence(dispute_uuid, &auth_user.user_id, request)
        .await?;

    Ok(Json(evidence))
}

/// GET /api/v2/disputes/:dispute_id/evidence
///
/// List all evidence for a dispute.
pub async fn list_evidence(
    State(services): State<Arc<ServiceContainer>>,
    auth_user: AuthenticatedUser,
    Path(dispute_id): Path<String>,
) -> Result<Json<Vec<DisputeEvidence>>, ApiError> {
    let dispute_uuid = Uuid::parse_str(&dispute_id)
        .map_err(|_| ApiError::Validation("Invalid dispute ID".to_string()))?;

    // Verify access
    if auth_user.role != Role::Admin {
        let dispute = services.dispute.get_dispute(dispute_uuid).await?;
        if dispute.filed_by_user_id != auth_user.user_id {
            return Err(ApiError::Authorization(
                "You are not authorised to view evidence for this dispute".to_string(),
            ));
        }
    }

    let evidence = services.dispute.list_evidence(dispute_uuid).await?;
    Ok(Json(evidence))
}
