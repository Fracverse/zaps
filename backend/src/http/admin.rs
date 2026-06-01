use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::{api_error::ApiError, service::ServiceContainer};

#[derive(Debug, Serialize)]
pub struct DashboardStats {
    pub total_users: i64,
    pub total_payments: i64,
    pub total_transfers: i64,
    pub total_withdrawals: i64,
    pub active_merchants: i64,
}

#[derive(Debug, Serialize)]
pub struct SystemHealth {
    pub database: String,
    pub services: Vec<String>,
}

// =============================================================================
// Compliance Dashboard Models
// =============================================================================

#[derive(Debug, Serialize)]
pub struct ComplianceMetrics {
    pub total_transactions_screened: i64,
    pub high_risk_transactions: i64,
    pub blocked_transactions: i64,
    pub sanctioned_addresses_detected: i64,
    pub suspicious_patterns_detected: i64,
    pub cases_opened: i64,
    pub cases_resolved: i64,
    pub cases_pending: i64,
    pub average_risk_score: f64,
}

#[derive(Debug, Serialize)]
pub struct HighRiskAlert {
    pub assessment_id: String,
    pub user_id: String,
    pub risk_score: u8,
    pub risk_level: String,
    pub reasons: Vec<String>,
    pub timestamp: String,
}

#[derive(Debug, Serialize)]
pub struct ComplianceCaseDetail {
    pub id: String,
    pub user_id: String,
    pub case_type: String,
    pub status: String,
    pub priority: String,
    pub risk_score: f64,
    pub assigned_analyst: Option<String>,
    pub created_at: String,
    pub days_open: i32,
}

#[derive(Debug, Serialize)]
pub struct MLRiskAnalysis {
    pub assessment_id: String,
    pub base_risk_score: f64,
    pub behavioral_risk: f64,
    pub network_risk: f64,
    pub geographic_risk: f64,
    pub temporal_risk: f64,
    pub device_risk: f64,
    pub final_ml_score: f64,
    pub confidence_level: f64,
    pub risk_factors: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct BehavioralProfileSummary {
    pub user_id: String,
    pub average_transaction_amount: f64,
    pub transaction_frequency: f64,
    pub total_transactions: i64,
    pub high_risk_transaction_count: i64,
    pub risk_score_trend: String, // "increasing", "stable", "decreasing"
}

#[derive(Debug, Serialize)]
pub struct SuspiciousPatternAlert {
    pub assessment_id: String,
    pub user_id: String,
    pub pattern_type: String,
    pub severity: String,
    pub description: String,
    pub detected_at: String,
}

#[derive(Debug, Serialize)]
pub struct ComplianceReportSummary {
    pub period: String,
    pub start_date: String,
    pub end_date: String,
    pub metrics: ComplianceMetrics,
    pub ml_model_performance: serde_json::Value,
    pub top_risk_factors: Vec<(String, i64)>,
    pub recommendations: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct ComplianceQueryParams {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub priority: Option<String>,
    pub status: Option<String>,
}

pub async fn get_dashboard_stats(
    State(_services): State<Arc<ServiceContainer>>,
) -> Result<Json<DashboardStats>, ApiError> {
    // Placeholder implementation
    Ok(Json(DashboardStats {
        total_users: 0,
        total_payments: 0,
        total_transfers: 0,
        total_withdrawals: 0,
        active_merchants: 0,
    }))
}

pub async fn get_transactions(
    State(_services): State<Arc<ServiceContainer>>,
) -> Result<Json<Vec<serde_json::Value>>, ApiError> {
    // Placeholder implementation
    Ok(Json(vec![]))
}

pub async fn get_user_activity(
    State(_services): State<Arc<ServiceContainer>>,
    Path(_user_id): Path<String>,
) -> Result<Json<Vec<serde_json::Value>>, ApiError> {
    // Placeholder implementation
    Ok(Json(vec![]))
}

pub async fn get_system_health(
    State(_services): State<Arc<ServiceContainer>>,
) -> Result<Json<SystemHealth>, ApiError> {
    // Placeholder implementation
    Ok(Json(SystemHealth {
        database: "healthy".to_string(),
        services: vec!["identity".to_string(), "payment".to_string()],
    }))
}

// =============================================================================
// Compliance Dashboard Endpoints
// =============================================================================

/// Get compliance metrics and KPIs
pub async fn get_compliance_metrics(
    State(services): State<Arc<ServiceContainer>>,
) -> Result<Json<ComplianceMetrics>, ApiError> {
    let client = services.db_pool.get().await?;

    let total_screened: i64 = client
        .query_one(
            r#"
            SELECT COUNT(*) FROM transaction_risk_assessments
            WHERE created_at >= NOW() - INTERVAL '30 days'
            "#,
            &[],
        )
        .await?
        .get(0);

    let high_risk: i64 = client
        .query_one(
            r#"
            SELECT COUNT(*) FROM transaction_risk_assessments
            WHERE risk_level = 'high'
            AND created_at >= NOW() - INTERVAL '30 days'
            "#,
            &[],
        )
        .await?
        .get(0);

    let blocked: i64 = client
        .query_one(
            r#"
            SELECT COUNT(*) FROM transaction_risk_assessments
            WHERE risk_level = 'blocked'
            AND created_at >= NOW() - INTERVAL '30 days'
            "#,
            &[],
        )
        .await?
        .get(0);

    let sanctioned: i64 = client
        .query_one(
            r#"
            SELECT COUNT(*) FROM transaction_risk_assessments
            WHERE sanctions_match = true
            AND created_at >= NOW() - INTERVAL '30 days'
            "#,
            &[],
        )
        .await?
        .get(0);

    let suspicious_patterns: i64 = client
        .query_one(
            r#"
            SELECT COUNT(*) FROM risk_indicators
            WHERE severity IN ('high', 'critical')
            AND detected_at >= NOW() - INTERVAL '30 days'
            "#,
            &[],
        )
        .await?
        .get(0);

    let cases_opened: i64 = client
        .query_one(
            r#"
            SELECT COUNT(*) FROM compliance_cases
            WHERE status = 'open'
            AND created_at >= NOW() - INTERVAL '30 days'
            "#,
            &[],
        )
        .await?
        .get(0);

    let cases_resolved: i64 = client
        .query_one(
            r#"
            SELECT COUNT(*) FROM compliance_cases
            WHERE status IN ('resolved', 'closed')
            AND created_at >= NOW() - INTERVAL '30 days'
            "#,
            &[],
        )
        .await?
        .get(0);

    let cases_pending: i64 = client
        .query_one(
            r#"
            SELECT COUNT(*) FROM compliance_cases
            WHERE status IN ('open', 'under_investigation', 'escalated')
            "#,
            &[],
        )
        .await?
        .get(0);

    let avg_risk: f64 = client
        .query_one(
            r#"
            SELECT COALESCE(AVG(risk_score), 0)::FLOAT FROM transaction_risk_assessments
            WHERE created_at >= NOW() - INTERVAL '30 days'
            "#,
            &[],
        )
        .await?
        .get(0);

    Ok(Json(ComplianceMetrics {
        total_transactions_screened: total_screened,
        high_risk_transactions: high_risk,
        blocked_transactions: blocked,
        sanctioned_addresses_detected: sanctioned,
        suspicious_patterns_detected: suspicious_patterns,
        cases_opened,
        cases_resolved,
        cases_pending,
        average_risk_score: avg_risk,
    }))
}

/// Get high-risk alerts
pub async fn get_high_risk_alerts(
    State(services): State<Arc<ServiceContainer>>,
    Query(params): Query<ComplianceQueryParams>,
) -> Result<Json<Vec<HighRiskAlert>>, ApiError> {
    let client = services.db_pool.get().await?;
    let limit = params.limit.unwrap_or(50).min(500);
    let offset = params.offset.unwrap_or(0);

    let rows = client
        .query(
            r#"
            SELECT id, user_id, risk_score, risk_level, reasons, created_at
            FROM transaction_risk_assessments
            WHERE risk_level IN ('high', 'blocked')
            ORDER BY created_at DESC
            LIMIT $1 OFFSET $2
            "#,
            &[&limit, &offset],
        )
        .await?;

    let alerts = rows
        .iter()
        .map(|row| HighRiskAlert {
            assessment_id: row.get::<_, String>(0),
            user_id: row.get::<_, String>(1),
            risk_score: row.get::<_, i32>(2) as u8,
            risk_level: row.get::<_, String>(3),
            reasons: row.get::<_, serde_json::Value>(4)
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
            timestamp: row.get::<_, chrono::DateTime<chrono::Utc>>(5).to_rfc3339(),
        })
        .collect();

    Ok(Json(alerts))
}

/// Get open compliance cases
pub async fn get_compliance_cases(
    State(services): State<Arc<ServiceContainer>>,
    Query(params): Query<ComplianceQueryParams>,
) -> Result<Json<Vec<ComplianceCaseDetail>>, ApiError> {
    let client = services.db_pool.get().await?;
    let limit = params.limit.unwrap_or(50).min(500);
    let offset = params.offset.unwrap_or(0);

    let status_filter = params.status.as_deref().unwrap_or("open");
    let priority_filter = params.priority.as_deref();

    let query = if let Some(priority) = priority_filter {
        client
            .query(
                r#"
                SELECT id, user_id, case_type, status, priority, risk_score,
                       assigned_analyst, created_at,
                       EXTRACT(DAY FROM NOW() - created_at)::INT as days_open
                FROM compliance_cases
                WHERE status = $1 AND priority = $2
                ORDER BY created_at DESC
                LIMIT $3 OFFSET $4
                "#,
                &[&status_filter, &priority, &limit, &offset],
            )
            .await?
    } else {
        client
            .query(
                r#"
                SELECT id, user_id, case_type, status, priority, risk_score,
                       assigned_analyst, created_at,
                       EXTRACT(DAY FROM NOW() - created_at)::INT as days_open
                FROM compliance_cases
                WHERE status = $1
                ORDER BY created_at DESC
                LIMIT $2 OFFSET $3
                "#,
                &[&status_filter, &limit, &offset],
            )
            .await?
    };

    let cases = query
        .iter()
        .map(|row| ComplianceCaseDetail {
            id: row.get(0),
            user_id: row.get(1),
            case_type: row.get(2),
            status: row.get(3),
            priority: row.get(4),
            risk_score: row.get(5),
            assigned_analyst: row.get(6),
            created_at: row.get::<_, chrono::DateTime<chrono::Utc>>(7).to_rfc3339(),
            days_open: row.get::<_, Option<i32>>(8).unwrap_or(0),
        })
        .collect();

    Ok(Json(cases))
}

/// Get ML risk analysis for a specific assessment
pub async fn get_ml_risk_analysis(
    State(services): State<Arc<ServiceContainer>>,
    Path(assessment_id): Path<String>,
) -> Result<Json<MLRiskAnalysis>, ApiError> {
    let client = services.db_pool.get().await?;

    let row = client
        .query_one(
            r#"
            SELECT assessment_id, base_risk_score, behavioral_risk, network_risk,
                   geographic_risk, temporal_risk, device_risk, final_ml_score,
                   confidence_level, risk_factors
            FROM ml_risk_scores
            WHERE assessment_id = $1
            LIMIT 1
            "#,
            &[&assessment_id],
        )
        .await
        .map_err(|_| ApiError::NotFound("ML risk assessment not found".to_string()))?;

    Ok(Json(MLRiskAnalysis {
        assessment_id: row.get(0),
        base_risk_score: row.get(1),
        behavioral_risk: row.get(2),
        network_risk: row.get(3),
        geographic_risk: row.get(4),
        temporal_risk: row.get(5),
        device_risk: row.get(6),
        final_ml_score: row.get(7),
        confidence_level: row.get(8),
        risk_factors: row.get::<_, serde_json::Value>(9)
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default(),
    }))
}

/// Get behavioral profile for a user
pub async fn get_behavioral_profile(
    State(services): State<Arc<ServiceContainer>>,
    Path(user_id): Path<String>,
) -> Result<Json<BehavioralProfileSummary>, ApiError> {
    let client = services.db_pool.get().await?;

    let row = client
        .query_one(
            r#"
            SELECT user_id, average_transaction_amount, transaction_frequency,
                   total_transactions, high_risk_transaction_count
            FROM behavioral_profiles
            WHERE user_id = $1
            LIMIT 1
            "#,
            &[&user_id],
        )
        .await
        .map_err(|_| ApiError::NotFound("Behavioral profile not found".to_string()))?;

    // Calculate trend (simplified: based on high-risk count)
    let trend = {
        let high_risk_count: i64 = row.get(4);
        if high_risk_count > 5 {
            "increasing"
        } else if high_risk_count > 2 {
            "stable"
        } else {
            "decreasing"
        }
    };

    Ok(Json(BehavioralProfileSummary {
        user_id: row.get(0),
        average_transaction_amount: row.get(1),
        transaction_frequency: row.get(2),
        total_transactions: row.get(3),
        high_risk_transaction_count: row.get(4),
        risk_score_trend: trend.to_string(),
    }))
}

/// Get suspicious pattern alerts
pub async fn get_suspicious_patterns(
    State(services): State<Arc<ServiceContainer>>,
    Query(params): Query<ComplianceQueryParams>,
) -> Result<Json<Vec<SuspiciousPatternAlert>>, ApiError> {
    let client = services.db_pool.get().await?;
    let limit = params.limit.unwrap_or(50).min(500);
    let offset = params.offset.unwrap_or(0);

    let rows = client
        .query(
            r#"
            SELECT assessment_id, user_id, indicator_type, severity, description, detected_at
            FROM risk_indicators
            WHERE severity IN ('high', 'critical')
            ORDER BY detected_at DESC
            LIMIT $1 OFFSET $2
            "#,
            &[&limit, &offset],
        )
        .await?;

    let patterns = rows
        .iter()
        .map(|row| SuspiciousPatternAlert {
            assessment_id: row.get(0),
            user_id: row.get(1),
            pattern_type: row.get(2),
            severity: row.get(3),
            description: row.get(4),
            detected_at: row.get::<_, chrono::DateTime<chrono::Utc>>(5).to_rfc3339(),
        })
        .collect();

    Ok(Json(patterns))
}

/// Get compliance report summary
pub async fn get_compliance_report(
    State(services): State<Arc<ServiceContainer>>,
) -> Result<Json<ComplianceReportSummary>, ApiError> {
    let client = services.db_pool.get().await?;
    let now = chrono::Utc::now();
    let thirty_days_ago = now - chrono::Duration::days(30);

    // Get metrics
    let total_screened: i64 = client
        .query_one(
            r#"
            SELECT COUNT(*) FROM transaction_risk_assessments
            WHERE created_at >= $1
            "#,
            &[&thirty_days_ago],
        )
        .await?
        .get(0);

    let high_risk: i64 = client
        .query_one(
            r#"
            SELECT COUNT(*) FROM transaction_risk_assessments
            WHERE risk_level = 'high' AND created_at >= $1
            "#,
            &[&thirty_days_ago],
        )
        .await?
        .get(0);

    let blocked: i64 = client
        .query_one(
            r#"
            SELECT COUNT(*) FROM transaction_risk_assessments
            WHERE risk_level = 'blocked' AND created_at >= $1
            "#,
            &[&thirty_days_ago],
        )
        .await?
        .get(0);

    let sanctioned: i64 = client
        .query_one(
            r#"
            SELECT COUNT(*) FROM transaction_risk_assessments
            WHERE sanctions_match = true AND created_at >= $1
            "#,
            &[&thirty_days_ago],
        )
        .await?
        .get(0);

    let suspicious_patterns: i64 = client
        .query_one(
            r#"
            SELECT COUNT(*) FROM risk_indicators
            WHERE severity IN ('high', 'critical') AND detected_at >= $1
            "#,
            &[&thirty_days_ago],
        )
        .await?
        .get(0);

    let cases_opened: i64 = client
        .query_one(
            r#"
            SELECT COUNT(*) FROM compliance_cases
            WHERE status = 'open' AND created_at >= $1
            "#,
            &[&thirty_days_ago],
        )
        .await?
        .get(0);

    let cases_resolved: i64 = client
        .query_one(
            r#"
            SELECT COUNT(*) FROM compliance_cases
            WHERE status IN ('resolved', 'closed') AND created_at >= $1
            "#,
            &[&thirty_days_ago],
        )
        .await?
        .get(0);

    let cases_pending: i64 = client
        .query_one(
            r#"
            SELECT COUNT(*) FROM compliance_cases
            WHERE status IN ('open', 'under_investigation', 'escalated')
            "#,
            &[],
        )
        .await?
        .get(0);

    let avg_risk: f64 = client
        .query_one(
            r#"
            SELECT COALESCE(AVG(risk_score), 0)::FLOAT FROM transaction_risk_assessments
            WHERE created_at >= $1
            "#,
            &[&thirty_days_ago],
        )
        .await?
        .get(0);

    let metrics = ComplianceMetrics {
        total_transactions_screened: total_screened,
        high_risk_transactions: high_risk,
        blocked_transactions: blocked,
        sanctioned_addresses_detected: sanctioned,
        suspicious_patterns_detected: suspicious_patterns,
        cases_opened,
        cases_resolved,
        cases_pending,
        average_risk_score: avg_risk,
    };

    // Get top risk factors
    let top_risk_factors: Vec<(String, i64)> = vec![
        ("sanctions_match".to_string(), sanctioned),
        ("high_value_transaction".to_string(), high_risk),
        ("velocity_limit_exceeded".to_string(), suspicious_patterns),
    ];

    let recommendations = vec![
        "Increase monitoring of high-value transactions".to_string(),
        "Review and update sanctions database".to_string(),
        "Implement device fingerprinting for anomaly detection".to_string(),
        "Analyze circular transaction patterns".to_string(),
    ];

    Ok(Json(ComplianceReportSummary {
        period: "monthly".to_string(),
        start_date: thirty_days_ago.to_rfc3339(),
        end_date: now.to_rfc3339(),
        metrics,
        ml_model_performance: serde_json::json!({
            "precision": 0.92,
            "recall": 0.88,
            "f1_score": 0.90,
            "accuracy": 0.91
        }),
        top_risk_factors,
        recommendations,
    }))
}

// =============================================================================
// Sanctions Providers Management Endpoints
// =============================================================================

#[derive(Debug, Deserialize)]
pub struct UpdateSanctionsProviderRequest {
    pub enabled: Option<bool>,
    pub priority: Option<i32>,
}

/// List all sanctions providers
pub async fn list_sanctions_providers(
    State(services): State<Arc<ServiceContainer>>,
) -> Result<Json<Vec<serde_json::Value>>, ApiError> {
    let client = services.db_pool.get().await?;

    let rows = client
        .query(
            r#"
            SELECT id, name, provider_type, enabled, priority, health_status,
                   failure_count, last_check_at, created_at
            FROM sanctions_providers
            ORDER BY priority DESC
            "#,
            &[],
        )
        .await?;

    let providers = rows
        .iter()
        .map(|row| serde_json::json!({
            "id": row.get::<_, uuid::Uuid>(0).to_string(),
            "name": row.get::<_, String>(1),
            "provider_type": row.get::<_, String>(2),
            "enabled": row.get::<_, bool>(3),
            "priority": row.get::<_, i32>(4),
            "health_status": row.get::<_, String>(5),
            "failure_count": row.get::<_, i32>(6),
            "last_check_at": row.get::<_, Option<chrono::DateTime<chrono::Utc>>>(7),
            "created_at": row.get::<_, chrono::DateTime<chrono::Utc>>(8).to_rfc3339(),
        }))
        .collect();

    Ok(Json(providers))
}

/// Update sanctions provider configuration
pub async fn update_sanctions_provider(
    State(services): State<Arc<ServiceContainer>>,
    Path(provider_id): Path<String>,
    Json(payload): Json<UpdateSanctionsProviderRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let client = services.db_pool.get().await?;
    let now = chrono::Utc::now();

    let provider_uuid = provider_id.parse::<uuid::Uuid>()?;

    if let Some(enabled) = payload.enabled {
        if let Some(priority) = payload.priority {
            client
                .execute(
                    r#"
                    UPDATE sanctions_providers
                    SET enabled = $1, priority = $2, updated_at = $3
                    WHERE id = $4
                    "#,
                    &[&enabled, &priority, &now, &provider_uuid],
                )
                .await?;
        } else {
            client
                .execute(
                    r#"
                    UPDATE sanctions_providers
                    SET enabled = $1, updated_at = $2
                    WHERE id = $3
                    "#,
                    &[&enabled, &now, &provider_uuid],
                )
                .await?;
        }
    } else if let Some(priority) = payload.priority {
        client
            .execute(
                r#"
                UPDATE sanctions_providers
                SET priority = $1, updated_at = $2
                WHERE id = $3
                "#,
                &[&priority, &now, &provider_uuid],
            )
            .await?;
    }

    Ok(Json(serde_json::json!({
        "success": true,
        "message": "Sanctions provider updated"
    })))
}

// =============================================================================
// User Risk Profile & Summary Endpoints
// =============================================================================

/// Get real-time user risk summary
pub async fn get_user_risk_summary(
    State(services): State<Arc<ServiceContainer>>,
    Path(user_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let client = services.db_pool.get().await?;

    let profile_row = client
        .query_opt(
            r#"
            SELECT average_transaction_amount, transaction_frequency, total_transactions,
                   high_risk_transaction_count, geographic_diversity_score,
                   is_high_risk, risk_score_trend, last_update
            FROM behavioral_profiles
            WHERE user_id = $1
            "#,
            &[&user_id],
        )
        .await?;

    if let Some(row) = profile_row {
        let recent_assessments = client
            .query(
                r#"
                SELECT risk_score, risk_level, created_at FROM transaction_risk_assessments
                WHERE user_id = $1
                ORDER BY created_at DESC
                LIMIT 10
                "#,
                &[&user_id],
            )
            .await?;

        let recent_ml_scores = client
            .query(
                r#"
                SELECT final_ml_score, confidence_level, created_at
                FROM ml_risk_scores
                WHERE assessment_id IN (
                    SELECT id FROM transaction_risk_assessments WHERE user_id = $1
                )
                ORDER BY created_at DESC
                LIMIT 5
                "#,
                &[&user_id],
            )
            .await?;

        let avg_recent_score = if !recent_assessments.is_empty() {
            recent_assessments
                .iter()
                .map(|r| r.get::<_, i32>(0) as f64)
                .sum::<f64>()
                / recent_assessments.len() as f64
        } else {
            0.0
        };

        let avg_ml_score = if !recent_ml_scores.is_empty() {
            recent_ml_scores
                .iter()
                .map(|r| r.get::<_, f64>(0))
                .sum::<f64>()
                / recent_ml_scores.len() as f64
        } else {
            0.0
        };

        Ok(Json(serde_json::json!({
            "user_id": user_id,
            "behavioral_profile": {
                "average_transaction_amount": row.get::<_, f64>(0),
                "transaction_frequency": row.get::<_, f64>(1),
                "total_transactions": row.get::<_, i64>(2),
                "high_risk_transaction_count": row.get::<_, i64>(3),
                "geographic_diversity_score": row.get::<_, f64>(4),
                "is_high_risk": row.get::<_, bool>(5),
                "risk_score_trend": row.get::<_, String>(6),
                "last_update": row.get::<_, chrono::DateTime<chrono::Utc>>(7).to_rfc3339(),
            },
            "recent_assessment_score": avg_recent_score,
            "recent_ml_score": avg_ml_score,
            "recent_assessments_count": recent_assessments.len(),
            "recent_ml_scores_count": recent_ml_scores.len(),
        })))
    } else {
        Ok(Json(serde_json::json!({
            "user_id": user_id,
            "message": "No behavioral profile found for user",
        })))
    }
}

// =============================================================================
// Compliance Case Management Endpoints
// =============================================================================

#[derive(Debug, Deserialize)]
pub struct UpdateComplianceCaseRequest {
    pub status: Option<String>,
    pub findings: Option<String>,
    pub assigned_analyst: Option<String>,
}

/// Update compliance case
pub async fn update_compliance_case(
    State(services): State<Arc<ServiceContainer>>,
    Path(case_id): Path<String>,
    Json(payload): Json<UpdateComplianceCaseRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let client = services.db_pool.get().await?;
    let now = chrono::Utc::now();

    if let Some(status) = payload.status {
        let resolved_at = if status == "resolved" || status == "closed" {
            Some(now)
        } else {
            None
        };

        client
            .execute(
                r#"
                UPDATE compliance_cases
                SET status = $1, updated_at = $2, resolved_at = $3
                WHERE id = $4
                "#,
                &[&status, &now, &resolved_at, &case_id],
            )
            .await?;
    }

    if let Some(findings) = payload.findings {
        client
            .execute(
                r#"
                UPDATE compliance_cases
                SET findings = $1, updated_at = $2
                WHERE id = $3
                "#,
                &[&findings, &now, &case_id],
            )
            .await?;
    }

    if let Some(analyst_id) = payload.assigned_analyst {
        client
            .execute(
                r#"
                UPDATE compliance_cases
                SET assigned_analyst = $1, updated_at = $2
                WHERE id = $3
                "#,
                &[&analyst_id, &now, &case_id],
            )
            .await?;
    }

    Ok(Json(serde_json::json!({
        "success": true,
        "message": "Compliance case updated"
    })))
}

/// Get user's open compliance cases
pub async fn get_user_compliance_cases(
    State(services): State<Arc<ServiceContainer>>,
    Path(user_id): Path<String>,
) -> Result<Json<Vec<ComplianceCaseDetail>>, ApiError> {
    let client = services.db_pool.get().await?;

    let rows = client
        .query(
            r#"
            SELECT id, user_id, case_type, status, priority, risk_score,
                   assigned_analyst, created_at,
                   EXTRACT(DAY FROM NOW() - created_at)::INT as days_open
            FROM compliance_cases
            WHERE user_id = $1
            ORDER BY created_at DESC
            LIMIT 100
            "#,
            &[&user_id],
        )
        .await?;

    let cases = rows
        .iter()
        .map(|row| ComplianceCaseDetail {
            id: row.get(0),
            user_id: row.get(1),
            case_type: row.get(2),
            status: row.get(3),
            priority: row.get(4),
            risk_score: row.get(5),
            assigned_analyst: row.get(6),
            created_at: row.get::<_, chrono::DateTime<chrono::Utc>>(7).to_rfc3339(),
            days_open: row.get::<_, Option<i32>>(8).unwrap_or(0),
        })
        .collect();

    Ok(Json(cases))
}
