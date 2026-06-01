use crate::{
    api_error::ApiError,
    config::Config,
    models::{
        AuditLogEntry, BehavioralProfile, ComplianceCase, MLRiskScore, RiskIndicator, RiskLevel,
        TransactionRiskAssessment, SanctionsProvider,
    },
    service::MetricsService,
};
use chrono::Timelike;
use deadpool_postgres::Pool;
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;
use std::time::Instant;

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
        // Use multiple sanctions providers instead of single provider
        let sanctions = self.screen_address_multiple_providers(user_id, address).await?;
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

        // Compute ML-based risk score
        let assessment_id = Uuid::new_v4().to_string();
        let ml_score = self
            .compute_ml_risk_score(
                &assessment_id,
                user_id,
                address,
                amount,
                assessment.risk_score,
            )
            .await?;

        // Detect suspicious patterns and create risk indicators
        let indicators = self.detect_suspicious_patterns(user_id).await?;
        for indicator in &indicators {
            self.persist_risk_indicator(indicator).await?;
        }

        // If high risk, create a compliance case for review
        if matches!(assessment.risk_level, RiskLevel::High | RiskLevel::Blocked) {
            let _ = self
                .create_compliance_case(
                    user_id,
                    Some(&assessment_id),
                    "high_risk_transaction",
                    if assessment.risk_level == RiskLevel::Blocked {
                        "critical"
                    } else {
                        "high"
                    },
                    ml_score.final_ml_score,
                    &format!(
                        "High-risk transaction detected: {} (ML Score: {:.2})",
                        assessment.reasons.join(", "),
                        ml_score.final_ml_score
                    ),
                )
                .await;
        }

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
                ml_score = ml_score.final_ml_score,
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

    // ======================================================================================
    // MULTIPLE SANCTIONS DATABASE SCREENING
    // ======================================================================================

    /// Screen an address against multiple sanctions databases concurrently
    pub async fn screen_address_multiple_providers(
        &self,
        user_id: &str,
        address: &str,
    ) -> Result<SanctionsApiResponse, ApiError> {
        let client = self.db_pool.get().await?;

        // Get enabled sanctions providers ordered by priority
        let providers = client
            .query(
                r#"
                SELECT id, name, provider_type, api_url, api_key, timeout_seconds
                FROM sanctions_providers
                WHERE enabled = TRUE
                ORDER BY priority DESC
                "#,
                &[],
            )
            .await?;

        if providers.is_empty() {
            return Err(ApiError::Compliance(
                "No sanctions providers configured".to_string(),
            ));
        }

        let mut all_sanctioned = false;
        let mut combined_risk_score = 0u8;
        let mut all_reasons = Vec::new();
        let mut screening_results = Vec::new();

        // Screen against multiple providers concurrently
        let mut handles = Vec::new();

        for row in providers {
            let provider_id: uuid::Uuid = row.get("id");
            let provider_name: String = row.get("name");
            let api_url: String = row.get("api_url");
            let api_key: String = row.get("api_key");
            let timeout_seconds: i32 = row.get("timeout_seconds");
            let address = address.to_string();
            let http = self.http.clone();

            let handle = tokio::spawn(async move {
                let start = Instant::now();
                let result = self::screen_against_provider(
                    &http,
                    &api_url,
                    &api_key,
                    &address,
                    timeout_seconds,
                )
                .await;
                let response_time_ms = start.elapsed().as_millis() as i32;

                (provider_id, provider_name, result, response_time_ms)
            });

            handles.push(handle);
        }

        // Collect results from all providers
        for handle in handles {
            match handle.await {
                Ok((provider_id, provider_name, result, response_time_ms)) => {
                    match result {
                        Ok((sanctioned, risk_score, reasons, http_status)) => {
                            screening_results.push((
                                provider_id,
                                sanctioned,
                                risk_score,
                                response_time_ms,
                                http_status,
                                None,
                            ));

                            if sanctioned {
                                all_sanctioned = true;
                                all_reasons.push(format!("{}: sanctioned", provider_name));
                            }

                            if let Some(score) = risk_score {
                                combined_risk_score = combined_risk_score.max(score);
                            }

                            if !reasons.is_empty() {
                                all_reasons.extend(reasons);
                            }
                        }
                        Err(error) => {
                            screening_results.push((
                                provider_id,
                                false,
                                None,
                                response_time_ms,
                                Some(500),
                                Some(error),
                            ));
                            all_reasons.push(format!(
                                "{}: screening failed",
                                provider_name
                            ));
                        }
                    }
                }
                Err(e) => {
                    tracing::error!(error = %e, "Failed to await sanctions provider screening");
                    all_reasons.push("provider_timeout".to_string());
                }
            }
        }

        // Store screening results in history
        for (provider_id, sanctioned, risk_score, response_time_ms, http_status, error) in screening_results {
            let _ = self.store_sanctions_screening_history(
                user_id,
                address,
                provider_id,
                sanctioned,
                risk_score,
                &all_reasons,
                response_time_ms,
                http_status,
                error.as_deref(),
            )
            .await;
        }

        Ok(SanctionsApiResponse {
            sanctioned: all_sanctioned,
            risk_score: if combined_risk_score > 0 {
                Some(combined_risk_score)
            } else {
                None
            },
            reasons: all_reasons,
        })
    }

    /// Store sanctions screening history for audit and analysis
    async fn store_sanctions_screening_history(
        &self,
        user_id: &str,
        address: &str,
        provider_id: uuid::Uuid,
        sanctioned: bool,
        risk_score: Option<u8>,
        reasons: &[String],
        response_time_ms: i32,
        http_status: Option<i32>,
        error_message: Option<&str>,
    ) -> Result<(), ApiError> {
        let client = self.db_pool.get().await?;

        client
            .execute(
                r#"
                INSERT INTO sanctions_screening_history (
                    user_id, address, provider_id, sanctioned, risk_score,
                    reasons, response_time_ms, http_status_code, error_message
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
                "#,
                &[
                    &user_id,
                    &address,
                    &provider_id,
                    &sanctioned,
                    &(risk_score.map(|s| s as i32)),
                    &json!(reasons),
                    &response_time_ms,
                    &http_status,
                    &error_message,
                ],
            )
            .await?;

        Ok(())
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
        let assessment_id = Uuid::new_v4().to_string();

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
                assessment_id: assessment_id.clone(),
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
                assessment_id: assessment_id.clone(),
                indicator_type: "circular_flow".to_string(),
                severity: "high".to_string(),
                description: format!("Detected circular money flows with {} addresses", circular_count),
                detected_at: chrono::Utc::now(),
            });
        }

        // Detect rapid transaction escalation (layering)
        let rapid_escalation: i64 = client
            .query_one(
                r#"
                SELECT COUNT(*) FROM payments
                WHERE from_address = $1
                AND created_at >= NOW() - INTERVAL '1 hour'
                "#,
                &[&user_id],
            )
            .await?
            .get(0);

        if rapid_escalation > 10 {
            indicators.push(RiskIndicator {
                id: Uuid::new_v4().to_string(),
                assessment_id: assessment_id.clone(),
                indicator_type: "layering".to_string(),
                severity: "critical".to_string(),
                description: format!(
                    "Detected potential layering: {} transactions in 1 hour",
                    rapid_escalation
                ),
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

    async fn persist_risk_indicator(
        &self,
        indicator: &RiskIndicator,
    ) -> Result<(), ApiError> {
        let client = self.db_pool.get().await?;

        // Get user_id from the assessment
        let assessment_row = client
            .query_one(
                r#"
                SELECT user_id FROM transaction_risk_assessments
                WHERE id = $1
                LIMIT 1
                "#,
                &[&indicator.assessment_id],
            )
            .await?;
        let user_id: String = assessment_row.get("user_id");

        client
            .execute(
                r#"
                INSERT INTO risk_indicators (
                    id, assessment_id, user_id, indicator_type, severity,
                    description, detected_at
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7)
                "#,
                &[
                    &indicator.id,
                    &indicator.assessment_id,
                    &user_id,
                    &indicator.indicator_type,
                    &indicator.severity,
                    &indicator.description,
                    &indicator.detected_at,
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

    // ======================================================================================
    // SANCTIONS PROVIDER MANAGEMENT
    // ======================================================================================

    /// Get all sanctions providers
    pub async fn get_sanctions_providers(&self) -> Result<Vec<SanctionsProvider>, ApiError> {
        let client = self.db_pool.get().await?;

        let rows = client
            .query(
                r#"
                SELECT id, name, provider_type, api_url, api_key, enabled, priority,
                       timeout_seconds, health_status, last_check_at, failure_count, created_at
                FROM sanctions_providers
                ORDER BY priority DESC
                "#,
                &[],
            )
            .await?;

        Ok(rows
            .iter()
            .map(|row| SanctionsProvider {
                id: row.get::<_, uuid::Uuid>(0).to_string(),
                name: row.get("name"),
                provider_type: row.get("provider_type"),
                api_url: row.get("api_url"),
                api_key: row.get("api_key"),
                enabled: row.get("enabled"),
                priority: row.get("priority"),
                timeout_seconds: row.get("timeout_seconds"),
                created_at: row.get("created_at"),
            })
            .collect())
    }

    /// Update sanctions provider configuration
    pub async fn update_sanctions_provider(
        &self,
        provider_id: &str,
        enabled: bool,
        priority: i32,
    ) -> Result<(), ApiError> {
        let client = self.db_pool.get().await?;
        let now = chrono::Utc::now();

        client
            .execute(
                r#"
                UPDATE sanctions_providers
                SET enabled = $1, priority = $2, updated_at = $3
                WHERE id = $4
                "#,
                &[&enabled, &priority, &now, &provider_id.parse::<uuid::Uuid>()?],
            )
            .await?;

        tracing::info!(
            provider_id = %provider_id,
            enabled = enabled,
            priority = priority,
            "Sanctions provider updated"
        );

        Ok(())
    }

    /// Update sanctions provider health status
    pub async fn update_provider_health_status(
        &self,
        provider_id: &str,
        health_status: &str,
    ) -> Result<(), ApiError> {
        let client = self.db_pool.get().await?;
        let now = chrono::Utc::now();

        client
            .execute(
                r#"
                UPDATE sanctions_providers
                SET health_status = $1, last_check_at = $2
                WHERE id = $3
                "#,
                &[&health_status, &now, &provider_id.parse::<uuid::Uuid>()?],
            )
            .await?;

        Ok(())
    }

    // ======================================================================================
    // REAL-TIME RISK SCORING UPDATES
    // ======================================================================================

    /// Update user's behavioral profile and risk trend
    pub async fn update_user_risk_profile(&self, user_id: &str) -> Result<BehavioralProfile, ApiError> {
        let client = self.db_pool.get().await?;

        // Calculate current risk metrics
        let profile = self.analyze_behavioral_pattern(user_id).await?;

        // Get previous risk trend
        let prev_profile: Option<(f64, f64)> = client
            .query_opt(
                r#"
                SELECT high_risk_transaction_count, total_transactions
                FROM behavioral_profiles
                WHERE user_id = $1
                "#,
                &[&user_id],
            )
            .await?
            .map(|row| (row.get::<_, i64>(0) as f64, row.get::<_, i64>(1) as f64));

        // Determine trend
        let risk_trend = if let Some((prev_high, prev_total)) = prev_profile {
            let current_high_rate = if profile.total_transactions > 0 {
                profile.high_risk_transaction_count as f64 / profile.total_transactions as f64
            } else {
                0.0
            };

            let prev_high_rate = if prev_total > 0 {
                prev_high / prev_total
            } else {
                0.0
            };

            if current_high_rate > prev_high_rate * 1.2 {
                "increasing"
            } else if current_high_rate < prev_high_rate * 0.8 {
                "decreasing"
            } else {
                "stable"
            }
        } else {
            "stable"
        };

        // Update profile with trend
        let is_high_risk = profile.high_risk_transaction_count > 10
            || profile.geographic_diversity_score < 0.2
            || profile.transaction_frequency > 50.0;

        client
            .execute(
                r#"
                INSERT INTO behavioral_profiles (
                    user_id, average_transaction_amount, transaction_frequency,
                    total_transactions, high_risk_transaction_count,
                    geographic_diversity_score, time_pattern_score,
                    device_diversity_score, merchant_category_diversity,
                    is_high_risk, risk_score_trend, last_update
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
                ON CONFLICT (user_id) DO UPDATE SET
                    average_transaction_amount = $2,
                    transaction_frequency = $3,
                    total_transactions = $4,
                    high_risk_transaction_count = $5,
                    geographic_diversity_score = $6,
                    time_pattern_score = $7,
                    device_diversity_score = $8,
                    merchant_category_diversity = $9,
                    is_high_risk = $10,
                    risk_score_trend = $11,
                    last_update = $12
                "#,
                &[
                    &user_id,
                    &profile.average_transaction_amount,
                    &profile.transaction_frequency,
                    &profile.total_transactions,
                    &profile.high_risk_transaction_count,
                    &profile.geographic_diversity_score,
                    &profile.time_pattern_score,
                    &profile.device_diversity_score,
                    &profile.merchant_category_diversity,
                    &is_high_risk,
                    &risk_trend,
                    &chrono::Utc::now(),
                ],
            )
            .await?;

        Ok(profile)
    }

    /// Get real-time risk score updates for a user
    pub async fn get_user_risk_summary(&self, user_id: &str) -> Result<serde_json::Value, ApiError> {
        let client = self.db_pool.get().await?;

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
            let recent_assessments: Vec<_> = client
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

            let recent_ml_scores: Vec<_> = client
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

            Ok(json!({
                "user_id": user_id,
                "behavioral_profile": {
                    "average_transaction_amount": row.get::<_, f64>(0),
                    "transaction_frequency": row.get::<_, f64>(1),
                    "total_transactions": row.get::<_, i64>(2),
                    "high_risk_transaction_count": row.get::<_, i64>(3),
                    "geographic_diversity_score": row.get::<_, f64>(4),
                    "is_high_risk": row.get::<_, bool>(5),
                    "risk_score_trend": row.get::<_, String>(6),
                    "last_update": row.get::<_, chrono::DateTime<chrono::Utc>>(7),
                },
                "recent_assessment_score": avg_recent_score,
                "recent_assessments_count": recent_assessments.len(),
                "recent_ml_scores_count": recent_ml_scores.len(),
            }))
        } else {
            Ok(json!({
                "user_id": user_id,
                "message": "No behavioral profile found for user",
            }))
        }
    }
}

async fn screen_against_provider(
    http: &reqwest::Client,
    api_url: &str,
    api_key: &str,
    address: &str,
    timeout_seconds: i32,
) -> Result<(bool, Option<u8>, Vec<String>, u16), String> {
    let timeout = std::time::Duration::from_secs(timeout_seconds as u64);

    match tokio::time::timeout(
        timeout,
        http.post(api_url)
            .bearer_auth(api_key)
            .json(&json!({ "address": address }))
            .send(),
    )
    .await
    {
        Ok(Ok(response)) => {
            let status = response.status().as_u16();
            match response.json::<SanctionsApiResponse>().await {
                Ok(api_response) => Ok((
                    api_response.sanctioned,
                    api_response.risk_score,
                    api_response.reasons,
                    status,
                )),
                Err(e) => Err(format!("Failed to parse response: {}", e)),
            }
        }
        Ok(Err(e)) => Err(format!("Request failed: {}", e)),
        Err(_) => Err("Request timeout".to_string()),
    }
}
