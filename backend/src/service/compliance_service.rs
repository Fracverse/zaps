use crate::{
    api_error::ApiError,
    config::Config,
    models::{AuditLogEntry, RiskLevel, TransactionRiskAssessment},
    service::MetricsService,
};
use deadpool_postgres::Pool;
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone)]
#[allow(dead_code)]
pub struct ComplianceService {
    db_pool: Arc<Pool>,
    config: Config,
    http: reqwest::Client,
}

#[derive(Debug, Deserialize)]
struct SanctionsApiResponse {
    #[serde(default)]
    sanctioned: bool,
    #[serde(default)]
    risk_score: Option<u8>,
    #[serde(default)]
    reasons: Vec<String>,
}

impl ComplianceService {
    pub fn new(db_pool: Arc<Pool>, config: Config) -> Self {
        Self {
            db_pool,
            config,
            http: reqwest::Client::new(),
        }
    }

    pub async fn check_sanctions(&self, address: &str) -> Result<bool, ApiError> {
        let assessment = self.assess_transaction_risk("unknown", address, 0).await?;
        Ok(assessment.sanctions_match)
    }

    pub async fn check_velocity_limits(
        &self,
        user_id: &str,
        amount: i64,
    ) -> Result<bool, ApiError> {
        if amount < 0 {
            return Ok(false);
        }

        let limits = &self.config.compliance_config.velocity_limits;
        if amount as u64 > limits.max_transaction_amount {
            return Ok(false);
        }

        let client = self.db_pool.get().await?;
        let daily_total: i64 = client
            .query_one(
                r#"
                SELECT COALESCE(SUM(amount), 0)::BIGINT
                FROM (
                    SELECT send_amount AS amount, created_at FROM payments WHERE from_address = $1
                    UNION ALL
                    SELECT amount, created_at FROM withdrawals WHERE user_id = $1
                    UNION ALL
                    SELECT amount, created_at FROM transfers WHERE from_user_id = $1
                ) tx
                WHERE created_at >= NOW() - INTERVAL '24 hours'
                "#,
                &[&user_id],
            )
            .await?
            .get(0);
        let monthly_total: i64 = client
            .query_one(
                r#"
                SELECT COALESCE(SUM(amount), 0)::BIGINT
                FROM (
                    SELECT send_amount AS amount, created_at FROM payments WHERE from_address = $1
                    UNION ALL
                    SELECT amount, created_at FROM withdrawals WHERE user_id = $1
                    UNION ALL
                    SELECT amount, created_at FROM transfers WHERE from_user_id = $1
                ) tx
                WHERE created_at >= NOW() - INTERVAL '30 days'
                "#,
                &[&user_id],
            )
            .await?
            .get(0);

        Ok(
            daily_total.saturating_add(amount) <= limits.daily_transaction_limit as i64
                && monthly_total.saturating_add(amount) <= limits.monthly_transaction_limit as i64,
        )
    }

    pub async fn assess_transaction_risk(
        &self,
        user_id: &str,
        address: &str,
        amount: i64,
    ) -> Result<TransactionRiskAssessment, ApiError> {
        let sanctions = self.screen_address(address).await?;
        let velocity_ok = self.check_velocity_limits(user_id, amount).await?;
        let thresholds = &self.config.compliance_config.risk_thresholds;

        let mut risk_score = sanctions.risk_score.unwrap_or(0);
        let mut reasons = sanctions.reasons;

        if sanctions.sanctioned {
            risk_score = risk_score.max(100);
            reasons.push("sanctions_match".to_string());
        }

        if amount as u64 >= thresholds.high_risk_amount {
            risk_score = risk_score.max(80);
            reasons.push("high_value_transaction".to_string());
        } else if amount as u64 >= thresholds.medium_risk_amount {
            risk_score = risk_score.max(45);
            reasons.push("medium_value_transaction".to_string());
        }

        if !velocity_ok {
            risk_score = risk_score.max(75);
            reasons.push("velocity_limit_exceeded".to_string());
        }

        for pattern in &thresholds.suspicious_patterns {
            if !pattern.is_empty() && address.contains(pattern) {
                risk_score = risk_score.max(70);
                reasons.push(format!("suspicious_pattern:{}", pattern));
            }
        }

        let risk_level = if sanctions.sanctioned {
            RiskLevel::Blocked
        } else if risk_score >= 75 {
            RiskLevel::High
        } else if risk_score >= 40 {
            RiskLevel::Medium
        } else {
            RiskLevel::Low
        };

        let assessment = TransactionRiskAssessment {
            user_id: user_id.to_string(),
            address: address.to_string(),
            amount,
            risk_score,
            risk_level,
            sanctions_match: sanctions.sanctioned,
            velocity_limit_exceeded: !velocity_ok,
            reasons,
        };

        self.persist_assessment(&assessment).await?;

        let decision = if assessment.risk_level == RiskLevel::Blocked {
            "blocked"
        } else if assessment.risk_level == RiskLevel::High {
            "flagged"
        } else {
            "approved"
        };
        MetricsService::record_compliance_screening(
            decision,
            &assessment.risk_level.to_string(),
            assessment.risk_score,
        );

        if matches!(assessment.risk_level, RiskLevel::High | RiskLevel::Blocked) {
            tracing::warn!(
                user_id = %assessment.user_id,
                address = %assessment.address,
                amount = assessment.amount,
                risk_score = assessment.risk_score,
                risk_level = %assessment.risk_level,
                reasons = ?assessment.reasons,
                "Compliance screening flagged transaction"
            );
        }

        Ok(assessment)
    }

    pub async fn log_audit_event(&self, event: AuditLogEntry) -> Result<(), ApiError> {
        let client = self.db_pool.get().await?;
        client
            .execute(
                r#"
                INSERT INTO audit_logs (id, actor_id, action, resource, resource_id, metadata, timestamp, ip_address, user_agent)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
                "#,
                &[
                    &event.id,
                    &event.actor_id,
                    &event.action,
                    &event.resource,
                    &event.resource_id,
                    &event.metadata,
                    &event.timestamp,
                    &event.ip_address,
                    &event.user_agent,
                ],
            )
            .await?;
        Ok(())
    }

    async fn screen_address(&self, address: &str) -> Result<SanctionsApiResponse, ApiError> {
        let compliance_config = &self.config.compliance_config;
        if compliance_config.sanctions_api_url.contains("example.com")
            || compliance_config.sanctions_api_key == "api-key"
        {
            return Ok(SanctionsApiResponse {
                sanctioned: false,
                risk_score: None,
                reasons: vec!["sanctions_provider_not_configured".to_string()],
            });
        }

        let response = self
            .http
            .post(&compliance_config.sanctions_api_url)
            .bearer_auth(&compliance_config.sanctions_api_key)
            .json(&json!({ "address": address }))
            .send()
            .await
            .map_err(|error| {
                ApiError::Compliance(format!("Sanctions screening failed: {}", error))
            })?;

        if !response.status().is_success() {
            return Err(ApiError::Compliance(format!(
                "Sanctions provider returned {}",
                response.status()
            )));
        }

        response
            .json::<SanctionsApiResponse>()
            .await
            .map_err(|error| ApiError::Compliance(format!("Invalid sanctions response: {}", error)))
    }

    async fn persist_assessment(
        &self,
        assessment: &TransactionRiskAssessment,
    ) -> Result<(), ApiError> {
        let client = self.db_pool.get().await?;
        let assessment_id = Uuid::new_v4();
        client
            .execute(
                r#"
                INSERT INTO transaction_risk_assessments (
                    id, user_id, address, amount, risk_score, risk_level,
                    sanctions_match, velocity_limit_exceeded, reasons
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
                "#,
                &[
                    &assessment_id,
                    &assessment.user_id,
                    &assessment.address,
                    &assessment.amount,
                    &(assessment.risk_score as i32),
                    &assessment.risk_level.to_string(),
                    &assessment.sanctions_match,
                    &assessment.velocity_limit_exceeded,
                    &json!(assessment.reasons),
                ],
            )
            .await?;

        Ok(())
    }
}
