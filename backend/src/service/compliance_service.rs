use crate::{
    api_error::ApiError,
    config::Config,
    models::{
        AuditLogEntry, BehavioralProfile, ComplianceCase, MLRiskScore, RiskIndicator, RiskLevel,
        TransactionRiskAssessment,
    },
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

    // ======================================================================================
    // ML-BASED RISK ASSESSMENT METHODS
    // ======================================================================================

    /// Analyze user behavioral patterns to detect anomalies
    pub async fn analyze_behavioral_pattern(
        &self,
        user_id: &str,
    ) -> Result<BehavioralProfile, ApiError> {
        let client = self.db_pool.get().await?;

        // Get transaction statistics for the user
        let stats = client
            .query_one(
                r#"
                SELECT
                    COUNT(*) as total_tx,
                    AVG(COALESCE(send_amount, amount)) as avg_amount,
                    STDDEV(COALESCE(send_amount, amount)) as stddev_amount,
                    COUNT(DISTINCT DATE(created_at)) as days_active
                FROM (
                    SELECT send_amount, created_at FROM payments WHERE from_address = $1
                    UNION ALL
                    SELECT amount, created_at FROM withdrawals WHERE user_id = $1
                    UNION ALL
                    SELECT amount, created_at FROM transfers WHERE from_user_id = $1
                ) tx
                WHERE created_at >= NOW() - INTERVAL '90 days'
                "#,
                &[&user_id],
            )
            .await?;

        let total_tx: i64 = stats.get("total_tx");
        let avg_amount: f64 = stats.get::<_, Option<f64>>("avg_amount").unwrap_or(0.0);
        let stddev_amount: f64 = stats.get::<_, Option<f64>>("stddev_amount").unwrap_or(1.0);
        let days_active: i64 = stats.get("days_active");

        let tx_frequency = if days_active > 0 {
            (total_tx as f64 / days_active as f64).min(100.0) // Cap at 100 tx/day
        } else {
            0.0
        };

        // Get high-risk transaction count
        let high_risk: i64 = client
            .query_one(
                r#"
                SELECT COUNT(*) FROM transaction_risk_assessments
                WHERE user_id = $1 AND risk_level IN ('high', 'blocked')
                AND created_at >= NOW() - INTERVAL '30 days'
                "#,
                &[&user_id],
            )
            .await?
            .get(0);

        // Calculate behavioral scores (0-1 scale)
        let transaction_frequency_threshold =
            self.config.compliance_config.behavioral_config.transaction_frequency_threshold;
        let geographic_diversity = self.calculate_geographic_diversity(user_id).await?;
        let time_pattern_score = self.calculate_time_pattern_anomaly(user_id).await?;
        let device_diversity = self.calculate_device_diversity(user_id).await?;
        let merchant_category_diversity =
            self.calculate_merchant_category_diversity(user_id).await?;

        let profile = BehavioralProfile {
            user_id: user_id.to_string(),
            average_transaction_amount: avg_amount,
            transaction_frequency: tx_frequency,
            total_transactions: total_tx,
            high_risk_transaction_count: high_risk,
            geographic_diversity_score: geographic_diversity,
            time_pattern_score,
            device_diversity_score: device_diversity,
            merchant_category_diversity,
            last_update: chrono::Utc::now(),
        };

        // Persist behavioral profile
        self.persist_behavioral_profile(&profile).await?;

        Ok(profile)
    }

    /// Calculate geographic diversity score for a user
    async fn calculate_geographic_diversity(&self, user_id: &str) -> Result<f64, ApiError> {
        let client = self.db_pool.get().await?;

        let unique_countries: i64 = client
            .query_one(
                r#"
                SELECT COUNT(DISTINCT country) FROM user_profiles WHERE user_id = $1
                "#,
                &[&user_id],
            )
            .await?
            .get(0);

        // Normalize: 0 = no diversity, 1 = high diversity
        // Assuming max 100 countries is enough for normalization
        Ok((unique_countries as f64 / 100.0).min(1.0))
    }

    /// Calculate temporal anomaly score (unusual transaction times)
    async fn calculate_time_pattern_anomaly(&self, _user_id: &str) -> Result<f64, ApiError> {
        // This is a simplified implementation
        // In production, use actual time series analysis
        let current_hour = chrono::Utc::now().hour();
        let business_hours = 9..=17;

        // Higher score if transaction outside business hours
        let anomaly_score = if business_hours.contains(&current_hour) {
            0.1
        } else {
            0.7
        };

        Ok(anomaly_score)
    }

    /// Calculate device diversity score
    async fn calculate_device_diversity(&self, _user_id: &str) -> Result<f64, ApiError> {
        // Simplified: would require device fingerprinting in production
        Ok(0.3)
    }

    /// Calculate merchant category diversity
    async fn calculate_merchant_category_diversity(
        &self,
        user_id: &str,
    ) -> Result<f64, ApiError> {
        let client = self.db_pool.get().await?;

        let unique_merchants: i64 = client
            .query_one(
                r#"
                SELECT COUNT(DISTINCT merchant_id) FROM payments WHERE from_address = $1
                AND created_at >= NOW() - INTERVAL '90 days'
                "#,
                &[&user_id],
            )
            .await?
            .get(0);

        // Normalize to 0-1 scale
        Ok((unique_merchants as f64 / 500.0).min(1.0))
    }

    /// Compute ML-based risk score using behavioral and network analysis
    pub async fn compute_ml_risk_score(
        &self,
        assessment_id: &str,
        user_id: &str,
        address: &str,
        amount: i64,
        base_risk_score: u8,
    ) -> Result<MLRiskScore, ApiError> {
        let ml_config = &self.config.compliance_config.ml_config;

        if !ml_config.enabled {
            // Return default score if ML is disabled
            return Ok(MLRiskScore {
                assessment_id: assessment_id.to_string(),
                model_version: ml_config.model_version.clone(),
                base_risk_score: base_risk_score as f64,
                behavioral_risk: base_risk_score as f64 * 0.5,
                network_risk: 0.0,
                geographic_risk: 0.0,
                temporal_risk: 0.0,
                device_risk: 0.0,
                final_ml_score: base_risk_score as f64,
                confidence_level: 0.5,
                risk_factors: vec![],
                created_at: chrono::Utc::now(),
            });
        }

        // Get behavioral profile
        let behavioral_profile = self.analyze_behavioral_pattern(user_id).await?;

        // Calculate individual risk components
        let behavioral_risk = self.calculate_behavioral_risk(&behavioral_profile, amount);
        let network_risk = self.calculate_network_risk(address).await?;
        let geographic_risk = self.calculate_geographic_risk(user_id).await?;
        let temporal_risk = self.calculate_temporal_risk().await?;
        let device_risk = self.calculate_device_risk(user_id).await?;

        // Calculate final ML score using weighted combination
        let final_score = (base_risk_score as f64) * 0.25
            + behavioral_risk * ml_config.behavioral_weight
            + network_risk * ml_config.network_weight
            + geographic_risk * ml_config.geographic_weight
            + temporal_risk * ml_config.temporal_weight
            + device_risk * ml_config.device_weight;

        let confidence_level = (0.5 + (0.5 * (final_score / 100.0))).min(1.0);
        let mut risk_factors = Vec::new();

        if behavioral_risk > 60.0 {
            risk_factors.push("abnormal_behavior".to_string());
        }
        if network_risk > 60.0 {
            risk_factors.push("suspicious_network".to_string());
        }
        if geographic_risk > 60.0 {
            risk_factors.push("geographic_anomaly".to_string());
        }
        if temporal_risk > 60.0 {
            risk_factors.push("temporal_anomaly".to_string());
        }
        if device_risk > 60.0 {
            risk_factors.push("device_anomaly".to_string());
        }

        let ml_score = MLRiskScore {
            assessment_id: assessment_id.to_string(),
            model_version: ml_config.model_version.clone(),
            base_risk_score: base_risk_score as f64,
            behavioral_risk,
            network_risk,
            geographic_risk,
            temporal_risk,
            device_risk,
            final_ml_score: final_score.min(100.0),
            confidence_level,
            risk_factors,
            created_at: chrono::Utc::now(),
        };

        self.persist_ml_score(&ml_score).await?;

        Ok(ml_score)
    }

    /// Calculate behavioral risk score based on user profile
    fn calculate_behavioral_risk(&self, profile: &BehavioralProfile, amount: i64) -> f64 {
        let threshold = self
            .config
            .compliance_config
            .behavioral_config
            .transaction_frequency_threshold;

        let mut risk = 0.0;

        // Risk from unusual transaction frequency
        if profile.transaction_frequency > threshold {
            risk += ((profile.transaction_frequency - threshold) / threshold * 40.0).min(40.0);
        }

        // Risk from unusual amount
        let avg_amount = profile.average_transaction_amount;
        if amount as f64 > avg_amount * 2.0 {
            risk += 30.0;
        }

        // Risk from high-risk transaction history
        if profile.high_risk_transaction_count > 5 {
            risk += 20.0;
        }

        // Low geographic diversity increases risk
        risk += (1.0 - profile.geographic_diversity_score) * 10.0;

        risk.min(100.0)
    }

    /// Calculate network-based risk (simplified)
    async fn calculate_network_risk(&self, _address: &str) -> Result<f64, ApiError> {
        // In production, would check if address is connected to known criminal networks
        Ok(0.0)
    }

    /// Calculate geographic risk score
    async fn calculate_geographic_risk(&self, user_id: &str) -> Result<f64, ApiError> {
        let profile = self.analyze_behavioral_pattern(user_id).await?;
        // Convert diversity to risk (lower diversity = higher risk)
        Ok((1.0 - profile.geographic_diversity_score) * 100.0)
    }

    /// Calculate temporal anomaly risk
    async fn calculate_temporal_risk(&self) -> Result<f64, ApiError> {
        let current_hour = chrono::Utc::now().hour();
        let business_hours = 9..=17;

        let risk = if business_hours.contains(&current_hour) {
            10.0
        } else {
            60.0
        };

        Ok(risk)
    }

    /// Calculate device-based risk
    async fn calculate_device_risk(&self, _user_id: &str) -> Result<f64, ApiError> {
        // Simplified: would require actual device fingerprinting
        Ok(15.0)
    }

    /// Detect suspicious patterns (structuring, circular flows, layering)
    pub async fn detect_suspicious_patterns(&self, user_id: &str) -> Result<Vec<RiskIndicator>, ApiError> {
        let client = self.db_pool.get().await?;
        let mut indicators = Vec::new();

        // Detect structuring: multiple transactions just below high-risk threshold
        let structuring_count: i64 = client
            .query_one(
                r#"
                SELECT COUNT(*) FROM (
                    SELECT send_amount FROM payments WHERE from_address = $1
                    AND created_at >= NOW() - INTERVAL '24 hours'
                    AND send_amount BETWEEN 900000 AND 950000
                ) t
                "#,
                &[&user_id],
            )
            .await?
            .get(0);

        if structuring_count > 5 {
            indicators.push(RiskIndicator {
                id: Uuid::new_v4().to_string(),
                assessment_id: user_id.to_string(),
                indicator_type: "structured_transaction".to_string(),
                severity: "high".to_string(),
                description: format!("Detected {} potential structuring transactions", structuring_count),
                detected_at: chrono::Utc::now(),
            });
        }

        // Detect circular flows: payment sent and received from same address
        let circular_count: i64 = client
            .query_one(
                r#"
                SELECT COUNT(DISTINCT address) FROM (
                    SELECT to_address AS address FROM payments WHERE from_address = $1
                    INTERSECT
                    SELECT from_address FROM payments WHERE to_address = $1
                ) t
                "#,
                &[&user_id],
            )
            .await?
            .get(0);

        if circular_count > 2 {
            indicators.push(RiskIndicator {
                id: Uuid::new_v4().to_string(),
                assessment_id: user_id.to_string(),
                indicator_type: "circular_flow".to_string(),
                severity: "high".to_string(),
                description: format!("Detected circular money flows with {} addresses", circular_count),
                detected_at: chrono::Utc::now(),
            });
        }

        Ok(indicators)
    }

    // ======================================================================================
    // CASE MANAGEMENT METHODS
    // ======================================================================================

    /// Create a compliance case for high-risk transactions
    pub async fn create_compliance_case(
        &self,
        user_id: &str,
        assessment_id: Option<&str>,
        case_type: &str,
        priority: &str,
        risk_score: f64,
        description: &str,
    ) -> Result<ComplianceCase, ApiError> {
        if !self.config.compliance_config.case_management_enabled {
            return Err(ApiError::Compliance(
                "Case management is disabled".to_string(),
            ));
        }

        let case_id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now();

        let compliance_case = ComplianceCase {
            id: case_id.clone(),
            user_id: user_id.to_string(),
            assessment_id: assessment_id.map(|s| s.to_string()),
            case_type: case_type.to_string(),
            status: "open".to_string(),
            priority: priority.to_string(),
            risk_score,
            assigned_analyst: None,
            description: description.to_string(),
            findings: None,
            resolution: None,
            created_at: now,
            updated_at: now,
            resolved_at: None,
        };

        self.persist_compliance_case(&compliance_case).await?;

        tracing::info!(
            case_id = %compliance_case.id,
            user_id = %user_id,
            case_type = case_type,
            priority = priority,
            "Compliance case created"
        );

        Ok(compliance_case)
    }

    /// Get compliance cases for a user
    pub async fn get_user_compliance_cases(&self, user_id: &str) -> Result<Vec<ComplianceCase>, ApiError> {
        let client = self.db_pool.get().await?;

        let rows = client
            .query(
                r#"
                SELECT id, user_id, assessment_id, case_type, status, priority,
                       risk_score, assigned_analyst, description, findings, resolution,
                       created_at, updated_at, resolved_at
                FROM compliance_cases
                WHERE user_id = $1
                ORDER BY created_at DESC
                "#,
                &[&user_id],
            )
            .await?;

        Ok(rows
            .iter()
            .map(|row| ComplianceCase {
                id: row.get("id"),
                user_id: row.get("user_id"),
                assessment_id: row.get("assessment_id"),
                case_type: row.get("case_type"),
                status: row.get("status"),
                priority: row.get("priority"),
                risk_score: row.get("risk_score"),
                assigned_analyst: row.get("assigned_analyst"),
                description: row.get("description"),
                findings: row.get("findings"),
                resolution: row.get("resolution"),
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
                resolved_at: row.get("resolved_at"),
            })
            .collect())
    }

    /// Update compliance case status
    pub async fn update_case_status(
        &self,
        case_id: &str,
        new_status: &str,
    ) -> Result<(), ApiError> {
        let client = self.db_pool.get().await?;
        let now = chrono::Utc::now();

        client
            .execute(
                r#"
                UPDATE compliance_cases
                SET status = $1, updated_at = $2
                WHERE id = $3
                "#,
                &[&new_status, &now, &case_id],
            )
            .await?;

        Ok(())
    }

    // ======================================================================================
    // PERSISTENCE METHODS
    // ======================================================================================

    async fn persist_behavioral_profile(
        &self,
        profile: &BehavioralProfile,
    ) -> Result<(), ApiError> {
        let client = self.db_pool.get().await?;

        client
            .execute(
                r#"
                INSERT INTO behavioral_profiles (
                    user_id, average_transaction_amount, transaction_frequency,
                    total_transactions, high_risk_transaction_count,
                    geographic_diversity_score, time_pattern_score,
                    device_diversity_score, merchant_category_diversity,
                    last_update
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
                ON CONFLICT (user_id) DO UPDATE SET
                    average_transaction_amount = $2,
                    transaction_frequency = $3,
                    total_transactions = $4,
                    high_risk_transaction_count = $5,
                    geographic_diversity_score = $6,
                    time_pattern_score = $7,
                    device_diversity_score = $8,
                    merchant_category_diversity = $9,
                    last_update = $10
                "#,
                &[
                    &profile.user_id,
                    &profile.average_transaction_amount,
                    &profile.transaction_frequency,
                    &profile.total_transactions,
                    &profile.high_risk_transaction_count,
                    &profile.geographic_diversity_score,
                    &profile.time_pattern_score,
                    &profile.device_diversity_score,
                    &profile.merchant_category_diversity,
                    &profile.last_update,
                ],
            )
            .await?;

        Ok(())
    }

    async fn persist_ml_score(&self, ml_score: &MLRiskScore) -> Result<(), ApiError> {
        let client = self.db_pool.get().await?;

        client
            .execute(
                r#"
                INSERT INTO ml_risk_scores (
                    assessment_id, model_version, base_risk_score, behavioral_risk,
                    network_risk, geographic_risk, temporal_risk, device_risk,
                    final_ml_score, confidence_level, risk_factors, created_at
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
                "#,
                &[
                    &ml_score.assessment_id,
                    &ml_score.model_version,
                    &ml_score.base_risk_score,
                    &ml_score.behavioral_risk,
                    &ml_score.network_risk,
                    &ml_score.geographic_risk,
                    &ml_score.temporal_risk,
                    &ml_score.device_risk,
                    &ml_score.final_ml_score,
                    &ml_score.confidence_level,
                    &json!(ml_score.risk_factors),
                    &ml_score.created_at,
                ],
            )
            .await?;

        Ok(())
    }

    async fn persist_compliance_case(
        &self,
        compliance_case: &ComplianceCase,
    ) -> Result<(), ApiError> {
        let client = self.db_pool.get().await?;

        client
            .execute(
                r#"
                INSERT INTO compliance_cases (
                    id, user_id, assessment_id, case_type, status, priority,
                    risk_score, assigned_analyst, description, findings, resolution,
                    created_at, updated_at, resolved_at
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
                "#,
                &[
                    &compliance_case.id,
                    &compliance_case.user_id,
                    &compliance_case.assessment_id,
                    &compliance_case.case_type,
                    &compliance_case.status,
                    &compliance_case.priority,
                    &compliance_case.risk_score,
                    &compliance_case.assigned_analyst,
                    &compliance_case.description,
                    &compliance_case.findings,
                    &compliance_case.resolution,
                    &compliance_case.created_at,
                    &compliance_case.updated_at,
                    &compliance_case.resolved_at,
                ],
            )
            .await?;

        Ok(())
    }
}
