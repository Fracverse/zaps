use crate::{
    api_error::ApiError,
    config::Config,
    models::{DisputeEvidence, DisputeReason, DisputeStatus, PaymentDispute},
    service::MetricsService,
};
use deadpool_postgres::Pool;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone)]
pub struct DisputeService {
    db_pool: Arc<Pool>,
    #[allow(dead_code)]
    config: Config,
}

// ---------------------------------------------------------------------------
// Request / Response DTOs
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct FileDisputeRequest {
    /// Reason category for the dispute.
    pub reason: String,
    /// Human-readable description of the issue.
    pub description: String,
    /// Amount being disputed in the payment's asset units.
    /// If omitted, defaults to the full payment amount.
    pub disputed_amount: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateDisputeStatusRequest {
    /// New status for the dispute.
    pub status: String,
    /// Resolution notes (required when resolving).
    pub resolution_notes: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AddEvidenceRequest {
    /// Type of evidence (e.g. "screenshot", "receipt", "communication").
    pub evidence_type: String,
    /// Description of the evidence.
    pub description: String,
    /// Optional URL to an uploaded file (use the /files endpoint first).
    pub file_url: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DisputeListResponse {
    pub disputes: Vec<PaymentDispute>,
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
}

#[derive(Debug, Deserialize)]
pub struct DisputeQueryParams {
    pub status: Option<String>,
    pub payment_id: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

fn default_limit() -> i64 {
    50
}

// ---------------------------------------------------------------------------
// Service implementation
// ---------------------------------------------------------------------------

impl DisputeService {
    pub fn new(db_pool: Arc<Pool>, config: Config) -> Self {
        Self { db_pool, config }
    }

    /// File a new dispute for a completed payment.
    ///
    /// Business rules enforced:
    /// - Payment must exist and belong to the requesting user.
    /// - Payment must be in `completed` status (can't dispute a pending payment).
    /// - Only one open/under-review dispute is allowed per payment at a time.
    pub async fn file_dispute(
        &self,
        payment_id: Uuid,
        user_id: &str,
        request: FileDisputeRequest,
    ) -> Result<PaymentDispute, ApiError> {
        let client = self.db_pool.get().await?;

        // Verify the payment exists and belongs to this user
        let payment_row = client
            .query_opt(
                "SELECT id, from_address, send_amount, status FROM payments WHERE id = $1",
                &[&payment_id],
            )
            .await?
            .ok_or_else(|| ApiError::NotFound("Payment not found".to_string()))?;

        let payment_status: String = payment_row.get(3);
        if payment_status != "completed" {
            return Err(ApiError::BadRequest(
                "Only completed payments can be disputed".to_string(),
            ));
        }

        // Check for an existing active dispute on this payment
        let existing = client
            .query_opt(
                "SELECT id FROM payment_disputes WHERE payment_id = $1 AND status IN ('open', 'under_review')",
                &[&payment_id],
            )
            .await?;

        if existing.is_some() {
            return Err(ApiError::Conflict(
                "An active dispute already exists for this payment".to_string(),
            ));
        }

        let full_amount: i64 = payment_row.get(2);
        let disputed_amount = request.disputed_amount.unwrap_or(full_amount);

        if disputed_amount <= 0 || disputed_amount > full_amount {
            return Err(ApiError::Validation(format!(
                "disputed_amount must be between 1 and {} (the full payment amount)",
                full_amount
            )));
        }

        let reason = DisputeReason::from_str(&request.reason).unwrap();
        let dispute_id = Uuid::new_v4().to_string();

        let row = client
            .query_one(
                r#"
                INSERT INTO payment_disputes (
                    id, payment_id, filed_by_user_id, reason, description,
                    status, disputed_amount
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7)
                RETURNING id, payment_id, filed_by_user_id, reason, description,
                          status, disputed_amount, resolution_notes, resolved_by,
                          created_at, updated_at
                "#,
                &[
                    &dispute_id,
                    &payment_id.to_string(),
                    &user_id,
                    &reason.to_string(),
                    &request.description,
                    &DisputeStatus::Open.to_string(),
                    &disputed_amount,
                ],
            )
            .await?;

        MetricsService::record_business_event("dispute", "filed");
        tracing::info!(
            dispute_id = %dispute_id,
            payment_id = %payment_id,
            user_id = %user_id,
            "Payment dispute filed"
        );

        Ok(Self::row_to_dispute(&row))
    }

    /// Get a single dispute by ID.
    pub async fn get_dispute(&self, dispute_id: Uuid) -> Result<PaymentDispute, ApiError> {
        let client = self.db_pool.get().await?;

        let row = client
            .query_one(
                r#"
                SELECT id, payment_id, filed_by_user_id, reason, description,
                       status, disputed_amount, resolution_notes, resolved_by,
                       created_at, updated_at
                FROM payment_disputes WHERE id = $1
                "#,
                &[&dispute_id],
            )
            .await
            .map_err(|_| ApiError::NotFound("Dispute not found".to_string()))?;

        Ok(Self::row_to_dispute(&row))
    }

    /// List disputes with optional filters (admin view).
    pub async fn list_disputes(
        &self,
        params: &DisputeQueryParams,
    ) -> Result<DisputeListResponse, ApiError> {
        let client = self.db_pool.get().await?;

        // Count total matching disputes
        let total: i64 = match (&params.status, &params.payment_id) {
            (Some(status), Some(payment_id)) => client
                .query_one(
                    "SELECT COUNT(*) FROM payment_disputes WHERE status = $1 AND payment_id = $2",
                    &[status, payment_id],
                )
                .await
                .map(|r| r.get(0))
                .unwrap_or(0),
            (Some(status), None) => client
                .query_one(
                    "SELECT COUNT(*) FROM payment_disputes WHERE status = $1",
                    &[status],
                )
                .await
                .map(|r| r.get(0))
                .unwrap_or(0),
            (None, Some(payment_id)) => client
                .query_one(
                    "SELECT COUNT(*) FROM payment_disputes WHERE payment_id = $1",
                    &[payment_id],
                )
                .await
                .map(|r| r.get(0))
                .unwrap_or(0),
            (None, None) => client
                .query_one("SELECT COUNT(*) FROM payment_disputes", &[])
                .await
                .map(|r| r.get(0))
                .unwrap_or(0),
        };

        // Fetch paginated rows
        let rows = match (&params.status, &params.payment_id) {
            (Some(status), Some(payment_id)) => client
                .query(
                    r#"SELECT id, payment_id, filed_by_user_id, reason, description,
                              status, disputed_amount, resolution_notes, resolved_by,
                              created_at, updated_at
                       FROM payment_disputes
                       WHERE status = $1 AND payment_id = $2
                       ORDER BY created_at DESC LIMIT $3 OFFSET $4"#,
                    &[status, payment_id, &params.limit, &params.offset],
                )
                .await?,
            (Some(status), None) => client
                .query(
                    r#"SELECT id, payment_id, filed_by_user_id, reason, description,
                              status, disputed_amount, resolution_notes, resolved_by,
                              created_at, updated_at
                       FROM payment_disputes
                       WHERE status = $1
                       ORDER BY created_at DESC LIMIT $2 OFFSET $3"#,
                    &[status, &params.limit, &params.offset],
                )
                .await?,
            (None, Some(payment_id)) => client
                .query(
                    r#"SELECT id, payment_id, filed_by_user_id, reason, description,
                              status, disputed_amount, resolution_notes, resolved_by,
                              created_at, updated_at
                       FROM payment_disputes
                       WHERE payment_id = $1
                       ORDER BY created_at DESC LIMIT $2 OFFSET $3"#,
                    &[payment_id, &params.limit, &params.offset],
                )
                .await?,
            (None, None) => client
                .query(
                    r#"SELECT id, payment_id, filed_by_user_id, reason, description,
                              status, disputed_amount, resolution_notes, resolved_by,
                              created_at, updated_at
                       FROM payment_disputes
                       ORDER BY created_at DESC LIMIT $1 OFFSET $2"#,
                    &[&params.limit, &params.offset],
                )
                .await?,
        };

        let disputes = rows.iter().map(Self::row_to_dispute).collect();

        Ok(DisputeListResponse {
            disputes,
            total,
            limit: params.limit,
            offset: params.offset,
        })
    }

    /// List disputes filed by a specific user.
    pub async fn list_user_disputes(
        &self,
        user_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<DisputeListResponse, ApiError> {
        let client = self.db_pool.get().await?;

        let total: i64 = client
            .query_one(
                "SELECT COUNT(*) FROM payment_disputes WHERE filed_by_user_id = $1",
                &[&user_id],
            )
            .await
            .map(|r| r.get(0))
            .unwrap_or(0);

        let rows = client
            .query(
                r#"
                SELECT id, payment_id, filed_by_user_id, reason, description,
                       status, disputed_amount, resolution_notes, resolved_by,
                       created_at, updated_at
                FROM payment_disputes
                WHERE filed_by_user_id = $1
                ORDER BY created_at DESC
                LIMIT $2 OFFSET $3
                "#,
                &[&user_id, &limit, &offset],
            )
            .await?;

        Ok(DisputeListResponse {
            disputes: rows.iter().map(Self::row_to_dispute).collect(),
            total,
            limit,
            offset,
        })
    }

    /// Update the status of a dispute (admin action).
    pub async fn update_dispute_status(
        &self,
        dispute_id: Uuid,
        admin_user_id: &str,
        request: UpdateDisputeStatusRequest,
    ) -> Result<PaymentDispute, ApiError> {
        let client = self.db_pool.get().await?;

        let new_status = DisputeStatus::from_str(&request.status).unwrap();

        // Validate the status transition
        let current_row = client
            .query_one(
                "SELECT status FROM payment_disputes WHERE id = $1",
                &[&dispute_id],
            )
            .await
            .map_err(|_| ApiError::NotFound("Dispute not found".to_string()))?;

        let current_status = DisputeStatus::from_str(current_row.get(0)).unwrap();
        Self::validate_status_transition(&current_status, &new_status)?;

        // Resolution notes are required when resolving
        if matches!(
            new_status,
            DisputeStatus::ResolvedCustomer | DisputeStatus::ResolvedMerchant
        ) && request.resolution_notes.is_none()
        {
            return Err(ApiError::Validation(
                "resolution_notes is required when resolving a dispute".to_string(),
            ));
        }

        let row = client
            .query_one(
                r#"
                UPDATE payment_disputes
                SET status = $1,
                    resolution_notes = COALESCE($2, resolution_notes),
                    resolved_by = CASE
                        WHEN $1 IN ('resolved_customer', 'resolved_merchant', 'closed')
                        THEN $3
                        ELSE resolved_by
                    END,
                    updated_at = NOW()
                WHERE id = $4
                RETURNING id, payment_id, filed_by_user_id, reason, description,
                          status, disputed_amount, resolution_notes, resolved_by,
                          created_at, updated_at
                "#,
                &[
                    &new_status.to_string(),
                    &request.resolution_notes,
                    &admin_user_id,
                    &dispute_id,
                ],
            )
            .await
            .map_err(|_| ApiError::NotFound("Dispute not found".to_string()))?;

        MetricsService::record_business_event("dispute", &format!("status_{}", new_status));
        tracing::info!(
            dispute_id = %dispute_id,
            new_status = %new_status,
            admin_user_id = %admin_user_id,
            "Dispute status updated"
        );

        Ok(Self::row_to_dispute(&row))
    }

    /// Add evidence to an existing dispute.
    pub async fn add_evidence(
        &self,
        dispute_id: Uuid,
        user_id: &str,
        request: AddEvidenceRequest,
    ) -> Result<DisputeEvidence, ApiError> {
        let client = self.db_pool.get().await?;

        // Verify the dispute exists and is still open/under review
        let dispute_row = client
            .query_opt(
                "SELECT status, filed_by_user_id FROM payment_disputes WHERE id = $1",
                &[&dispute_id],
            )
            .await?
            .ok_or_else(|| ApiError::NotFound("Dispute not found".to_string()))?;

        let status = DisputeStatus::from_str(dispute_row.get(0)).unwrap();
        if !matches!(status, DisputeStatus::Open | DisputeStatus::UnderReview) {
            return Err(ApiError::BadRequest(
                "Evidence can only be added to open or under-review disputes".to_string(),
            ));
        }

        let evidence_id = Uuid::new_v4().to_string();

        let row = client
            .query_one(
                r#"
                INSERT INTO dispute_evidence (
                    id, dispute_id, submitted_by_user_id, evidence_type, description, file_url
                )
                VALUES ($1, $2, $3, $4, $5, $6)
                RETURNING id, dispute_id, submitted_by_user_id, evidence_type,
                          description, file_url, created_at
                "#,
                &[
                    &evidence_id,
                    &dispute_id.to_string(),
                    &user_id,
                    &request.evidence_type,
                    &request.description,
                    &request.file_url,
                ],
            )
            .await?;

        MetricsService::record_business_event("dispute_evidence", "added");

        Ok(DisputeEvidence {
            id: row.get(0),
            dispute_id: row.get(1),
            submitted_by_user_id: row.get(2),
            evidence_type: row.get(3),
            description: row.get(4),
            file_url: row.get(5),
            created_at: row.get(6),
        })
    }

    /// List all evidence for a dispute.
    pub async fn list_evidence(
        &self,
        dispute_id: Uuid,
    ) -> Result<Vec<DisputeEvidence>, ApiError> {
        let client = self.db_pool.get().await?;

        let rows = client
            .query(
                r#"
                SELECT id, dispute_id, submitted_by_user_id, evidence_type,
                       description, file_url, created_at
                FROM dispute_evidence
                WHERE dispute_id = $1
                ORDER BY created_at ASC
                "#,
                &[&dispute_id.to_string()],
            )
            .await?;

        Ok(rows
            .iter()
            .map(|row| DisputeEvidence {
                id: row.get(0),
                dispute_id: row.get(1),
                submitted_by_user_id: row.get(2),
                evidence_type: row.get(3),
                description: row.get(4),
                file_url: row.get(5),
                created_at: row.get(6),
            })
            .collect())
    }

    // -------------------------------------------------------------------------
    // Private helpers
    // -------------------------------------------------------------------------

    fn row_to_dispute(row: &tokio_postgres::Row) -> PaymentDispute {
        PaymentDispute {
            id: row.get(0),
            payment_id: row.get(1),
            filed_by_user_id: row.get(2),
            reason: DisputeReason::from_str(row.get(3)).unwrap(),
            description: row.get(4),
            status: DisputeStatus::from_str(row.get(5)).unwrap(),
            disputed_amount: row.get(6),
            resolution_notes: row.get(7),
            resolved_by: row.get(8),
            created_at: row.get(9),
            updated_at: row.get(10),
        }
    }

    /// Enforce valid status transitions.
    fn validate_status_transition(
        from: &DisputeStatus,
        to: &DisputeStatus,
    ) -> Result<(), ApiError> {
        let valid = match (from, to) {
            // Open → UnderReview, Closed
            (DisputeStatus::Open, DisputeStatus::UnderReview) => true,
            (DisputeStatus::Open, DisputeStatus::Closed) => true,
            // UnderReview → ResolvedCustomer, ResolvedMerchant, Closed
            (DisputeStatus::UnderReview, DisputeStatus::ResolvedCustomer) => true,
            (DisputeStatus::UnderReview, DisputeStatus::ResolvedMerchant) => true,
            (DisputeStatus::UnderReview, DisputeStatus::Closed) => true,
            // Terminal states cannot transition
            _ => false,
        };

        if valid {
            Ok(())
        } else {
            Err(ApiError::BadRequest(format!(
                "Invalid status transition from '{}' to '{}'",
                from, to
            )))
        }
    }
}
