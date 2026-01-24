use axum::{
    extract::{Path, Query, State},
    Json,
};
use std::sync::Arc;

use crate::{
    api_error::ApiError,
    models::{AuditLogListResponse, AuditLogQueryParams, AuditLogResponse},
    service::ServiceContainer,
};

/// GET /audit-logs - List audit logs with optional filtering
#[axum::debug_handler]
pub async fn list_audit_logs(
    State(services): State<Arc<ServiceContainer>>,
    Query(params): Query<AuditLogQueryParams>,
) -> Result<Json<AuditLogListResponse>, ApiError> {
    let total = services.audit.count_audit_logs(&params).await?;
    let logs = services.audit.list_audit_logs(&params).await?;

    // Convert to response DTOs
    let log_responses: Vec<AuditLogResponse> = logs
        .into_iter()
        .map(|log| AuditLogResponse {
            id: log.id,
            actor_id: log.actor_id,
            action: log.action,
            resource: log.resource,
            resource_id: log.resource_id,
            metadata: log.metadata,
            timestamp: log.timestamp,
        })
        .collect();

    Ok(Json(AuditLogListResponse {
        logs: log_responses,
        total,
        limit: params.limit.min(100).max(1),
        offset: params.offset.max(0),
    }))
}

/// GET /audit-logs/:id - Get a single audit log by ID
pub async fn get_audit_log(
    State(services): State<Arc<ServiceContainer>>,
    Path(id): Path<String>,
) -> Result<Json<AuditLogResponse>, ApiError> {
    let log = services.audit.get_audit_log(&id).await?;

    Ok(Json(AuditLogResponse {
        id: log.id,
        actor_id: log.actor_id,
        action: log.action,
        resource: log.resource,
        resource_id: log.resource_id,
        metadata: log.metadata,
        timestamp: log.timestamp,
    }))
}
