use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use std::str::FromStr;
use std::sync::Arc;

use crate::models::{PaymentReconciliation, ReconciliationAuditLog, ReconciliationStatus};
use crate::service::{
    ReconciliationRequest, ReconciliationResult, ServiceContainer,
};

#[derive(Debug, Deserialize)]
pub struct ReconciliationQuery {
    pub status: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

fn default_limit() -> i64 {
    50
}

/// Run reconciliation
pub async fn run_reconciliation(
    State(services): State<Arc<ServiceContainer>>,
    Json(request): Json<ReconciliationRequest>,
) -> Result<Json<ReconciliationResult>, (StatusCode, String)> {
    // TODO: Get actor_id from auth context
    let actor_id = "system".to_string();

    let result = services
        .reconciliation
        .run_reconciliation(request, actor_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(result))
}

/// Get reconciliation records
pub async fn get_reconciliations(
    State(services): State<Arc<ServiceContainer>>,
    Query(query): Query<ReconciliationQuery>,
) -> Result<Json<Vec<PaymentReconciliation>>, (StatusCode, String)> {
    let status = query.status.and_then(|s| ReconciliationStatus::from_str(&s).ok());

    let reconciliations = services
        .reconciliation
        .get_reconciliations(status, query.limit, query.offset)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(reconciliations))
}

#[derive(Debug, Deserialize)]
pub struct ResolveReconciliationRequest {
    pub new_status: String,
    pub notes: Option<String>,
}

/// Manually resolve a reconciliation
pub async fn resolve_reconciliation(
    State(services): State<Arc<ServiceContainer>>,
    Path(reconciliation_id): Path<String>,
    Json(request): Json<ResolveReconciliationRequest>,
) -> Result<Json<PaymentReconciliation>, (StatusCode, String)> {
    let new_status = ReconciliationStatus::from_str(&request.new_status)
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid status".to_string()))?;

    // TODO: Get actor_id from auth context
    let actor_id = "system".to_string();

    let reconciliation = services
        .reconciliation
        .resolve_reconciliation(reconciliation_id, actor_id, new_status, request.notes)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(reconciliation))
}

/// Get audit log for a reconciliation
pub async fn get_audit_log(
    State(services): State<Arc<ServiceContainer>>,
    Path(reconciliation_id): Path<String>,
) -> Result<Json<Vec<ReconciliationAuditLog>>, (StatusCode, String)> {
    let logs = services
        .reconciliation
        .get_audit_log(reconciliation_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(logs))
}
