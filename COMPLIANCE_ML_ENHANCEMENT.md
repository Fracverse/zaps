# Backend: Enhance Compliance Service with ML Risk Assessment

## Implementation Summary

This document details the comprehensive enhancement of the compliance service with ML-based risk assessment, behavioral analysis, and automated case management.

### Issue Reference
- **Issue**: #181
- **Category**: Compliance
- **Priority**: 🔴 Critical
- **Status**: Implemented

---

## Completed Acceptance Criteria

✅ **Integrate with multiple sanctions databases**
- Added `SanctionsProviderConfig` to support multiple providers (OFAC, UN, EU, FCA)
- Implemented provider priority-based screening
- Enhanced sanctions screening to check multiple providers

✅ **Implement behavioral analysis for suspicious patterns**
- Created `BehavioralProfile` model for tracking user patterns
- Implemented `analyze_behavioral_pattern()` method with:
  - Transaction frequency analysis
  - Amount deviation detection
  - Geographic diversity scoring
  - Time pattern anomaly detection
  - Device diversity tracking
  - Merchant category diversification analysis

✅ **Add real-time risk scoring with ML models**
- Implemented `MLRiskScore` model for comprehensive ML scoring
- Created `compute_ml_risk_score()` with weighted risk factors:
  - Behavioral risk (25%)
  - Network risk (20%)
  - Geographic risk (20%)
  - Temporal risk (15%)
  - Device risk (20%)
- Added confidence level calculation
- Implemented risk factor identification

✅ **Include compliance reporting dashboard**
- Added comprehensive dashboard endpoints in `admin.rs`:
  - `/api/compliance/metrics` - Real-time compliance KPIs
  - `/api/compliance/alerts` - High-risk transaction alerts
  - `/api/compliance/cases` - Compliance case management
  - `/api/compliance/ml-risk/:assessment_id` - ML risk analysis
  - `/api/compliance/profile/:user_id` - Behavioral profiles
  - `/api/compliance/patterns` - Suspicious pattern detection
  - `/api/compliance/report` - Compliance report summary

✅ **Add automated case management for high-risk transactions**
- Created `ComplianceCase` model for case tracking
- Implemented `create_compliance_case()` for automated case creation
- Added `get_user_compliance_cases()` to retrieve user cases
- Implemented `update_case_status()` for case status management
- Added `CaseActivityLog` model for audit trail

---

## Technical Implementation Details

### 1. Models (backend/src/models.rs)

#### New Models Added:

**BehavioralProfile**
- Tracks user transaction patterns
- Stores metrics: frequency, average amount, high-risk count
- Calculates diversity scores: geographic, temporal, device, merchant

**SanctionsProvider**
- Configures multiple sanctions database providers
- Supports OFAC, UN, EU, FCA integration
- Priority-based screening mechanism

**MLRiskScore**
- Stores ML-based risk calculations
- Separate scoring for behavioral, network, geographic, temporal, device risks
- Includes confidence level and risk factors

**RiskIndicator**
- Identifies specific suspicious patterns:
  - Structured transactions (structuring)
  - Circular flows (money laundering technique)
  - Layering (fund obfuscation)

**ComplianceCase**
- Tracks high-risk transaction cases
- Supports case lifecycle: open → investigation → escalation → resolution
- Links assessments to cases for investigation

**CaseActivityLog**
- Audit trail for all case activities
- Tracks assignments, updates, escalations, resolutions

**ComplianceReport**
- Aggregates compliance metrics
- Period-based reporting (daily, weekly, monthly)
- Includes ML model performance metrics
- Provides recommendations

### 2. Configuration (backend/src/config.rs)

#### Enhanced ComplianceConfig:

**MLComplianceConfig**
- `enabled`: Toggle ML risk scoring
- `model_version`: Version tracking for model updates
- `confidence_threshold`: Minimum confidence to trust predictions
- Weighted factors for different risk components

**BehavioralAnalysisConfig**
- `enabled`: Toggle behavioral analysis
- Thresholds for anomaly detection:
  - Transaction frequency threshold
  - Amount deviation (standard deviations)
  - Geographic anomaly threshold
  - Time pattern threshold
  - Device anomaly threshold

**SanctionsProviderConfig**
- Multiple provider configuration
- Support for different provider types
- Priority ordering for screening sequence
- Timeout management per provider

**Case Management**
- `case_management_enabled`: Toggle automated case creation

### 3. Compliance Service (backend/src/service/compliance_service.rs)

#### New Methods:

**Behavioral Analysis**
```rust
pub async fn analyze_behavioral_pattern(user_id: &str) -> BehavioralProfile
```
- Analyzes user transaction history (90 days)
- Calculates behavioral metrics and diversity scores
- Persists profiles for historical tracking

**Geographic Analysis**
```rust
async fn calculate_geographic_diversity(user_id: &str) -> f64
```
- Measures transaction country diversity
- Flags concentrated geographic activity

**Temporal Analysis**
```rust
async fn calculate_time_pattern_anomaly(user_id: &str) -> f64
```
- Detects unusual transaction times
- Higher risk outside business hours

**ML Risk Scoring**
```rust
pub async fn compute_ml_risk_score(
    assessment_id: &str,
    user_id: &str,
    address: &str,
    amount: i64,
    base_risk_score: u8
) -> MLRiskScore
```
- Computes weighted ML risk score
- Integrates all risk components
- Calculates model confidence level

**Pattern Detection**
```rust
pub async fn detect_suspicious_patterns(user_id: &str) -> Vec<RiskIndicator>
```
- Detects structuring patterns (multiple transactions near thresholds)
- Identifies circular flows (same address sends/receives)
- Flags potential money laundering techniques

**Case Management**
```rust
pub async fn create_compliance_case(...) -> ComplianceCase
pub async fn get_user_compliance_cases(user_id: &str) -> Vec<ComplianceCase>
pub async fn update_case_status(case_id: &str, status: &str) -> ()
```
- Automated case creation for high-risk transactions
- Case retrieval and status management
- Audit logging for compliance

### 4. Admin Dashboard (backend/src/http/admin.rs)

#### New Response Models:

- `ComplianceMetrics` - KPI aggregation
- `HighRiskAlert` - Recent high-risk transactions
- `ComplianceCaseDetail` - Case summary with aging
- `MLRiskAnalysis` - Detailed ML scoring breakdown
- `BehavioralProfileSummary` - User profile with trend
- `SuspiciousPatternAlert` - Detected pattern details
- `ComplianceReportSummary` - Period report with metrics

#### New API Endpoints:

1. **GET /api/compliance/metrics**
   - Returns 30-day compliance KPIs
   - Includes high-risk counts, blocked transactions, detected patterns

2. **GET /api/compliance/alerts**
   - High-risk transaction alerts
   - Pagination support with limit/offset
   - Includes reason codes for each alert

3. **GET /api/compliance/cases**
   - Retrieves compliance cases
   - Filterable by status and priority
   - Shows days open for each case

4. **GET /api/compliance/ml-risk/:assessment_id**
   - Detailed ML risk breakdown for assessment
   - Shows individual component scores
   - Lists contributing risk factors

5. **GET /api/compliance/profile/:user_id**
   - User behavioral profile summary
   - Transaction statistics
   - Risk score trend indicator

6. **GET /api/compliance/patterns**
   - Suspicious pattern alerts
   - Severity-based filtering
   - Pattern type details

7. **GET /api/compliance/report**
   - Monthly compliance report
   - Aggregated metrics and statistics
   - ML model performance metrics
   - Actionable recommendations

---

## Risk Scoring Algorithm

### ML Risk Score Calculation

```
Final ML Score = 
    (Base Risk × 0.25) +
    (Behavioral Risk × 0.25) +
    (Network Risk × 0.20) +
    (Geographic Risk × 0.20) +
    (Temporal Risk × 0.15) +
    (Device Risk × 0.20)
```

### Risk Components:

1. **Behavioral Risk** (0-100)
   - Abnormal transaction frequency
   - Unusual transaction amounts
   - High-risk transaction history
   - Geographic diversity

2. **Network Risk** (0-100)
   - Connection to known criminal networks
   - Previous sanctions matches
   - Associated address risks

3. **Geographic Risk** (0-100)
   - Low geographic diversity
   - Concentration in high-risk regions
   - Rapid geographic movement

4. **Temporal Risk** (0-100)
   - Transactions outside business hours
   - Unusual timing patterns
   - Rapid succession transactions

5. **Device Risk** (0-100)
   - Device fingerprinting anomalies
   - New device usage
   - Suspicious device characteristics

---

## Database Schema Requirements

The following tables are required:

```sql
-- Behavioral profiles
CREATE TABLE behavioral_profiles (
    user_id VARCHAR PRIMARY KEY,
    average_transaction_amount FLOAT,
    transaction_frequency FLOAT,
    total_transactions BIGINT,
    high_risk_transaction_count BIGINT,
    geographic_diversity_score FLOAT,
    time_pattern_score FLOAT,
    device_diversity_score FLOAT,
    merchant_category_diversity FLOAT,
    last_update TIMESTAMP
);

-- ML risk scores
CREATE TABLE ml_risk_scores (
    assessment_id VARCHAR PRIMARY KEY,
    model_version VARCHAR,
    base_risk_score FLOAT,
    behavioral_risk FLOAT,
    network_risk FLOAT,
    geographic_risk FLOAT,
    temporal_risk FLOAT,
    device_risk FLOAT,
    final_ml_score FLOAT,
    confidence_level FLOAT,
    risk_factors JSONB,
    created_at TIMESTAMP
);

-- Risk indicators
CREATE TABLE risk_indicators (
    id VARCHAR PRIMARY KEY,
    assessment_id VARCHAR,
    indicator_type VARCHAR,
    severity VARCHAR,
    description TEXT,
    detected_at TIMESTAMP
);

-- Compliance cases
CREATE TABLE compliance_cases (
    id VARCHAR PRIMARY KEY,
    user_id VARCHAR,
    assessment_id VARCHAR,
    case_type VARCHAR,
    status VARCHAR,
    priority VARCHAR,
    risk_score FLOAT,
    assigned_analyst VARCHAR,
    description TEXT,
    findings TEXT,
    resolution TEXT,
    created_at TIMESTAMP,
    updated_at TIMESTAMP,
    resolved_at TIMESTAMP
);

-- Case activity logs
CREATE TABLE case_activity_logs (
    id VARCHAR PRIMARY KEY,
    case_id VARCHAR,
    activity_type VARCHAR,
    performed_by VARCHAR,
    details JSONB,
    created_at TIMESTAMP
);

-- Sanctions providers
CREATE TABLE sanctions_providers (
    id VARCHAR PRIMARY KEY,
    name VARCHAR,
    provider_type VARCHAR,
    api_url VARCHAR,
    api_key VARCHAR,
    enabled BOOLEAN,
    priority INTEGER,
    timeout_seconds INTEGER,
    created_at TIMESTAMP
);

-- Compliance reports
CREATE TABLE compliance_reports (
    id VARCHAR PRIMARY KEY,
    report_period VARCHAR,
    start_date TIMESTAMP,
    end_date TIMESTAMP,
    total_transactions_screened BIGINT,
    high_risk_transactions BIGINT,
    blocked_transactions BIGINT,
    sanctioned_addresses_detected BIGINT,
    suspicious_patterns_detected BIGINT,
    cases_opened BIGINT,
    cases_resolved BIGINT,
    cases_escalated BIGINT,
    total_amount_flagged BIGINT,
    ml_model_performance JSONB,
    recommendations JSONB,
    created_at TIMESTAMP
);
```

---

## Configuration Example

```toml
[compliance_config]
sanctions_api_url = "https://api.ofac.example.com"
sanctions_api_key = "api-key"
alert_webhook_url = "https://your-domain.com/webhooks/compliance"

case_management_enabled = true

[compliance_config.velocity_limits]
daily_transaction_limit = 10000000
monthly_transaction_limit = 100000000
max_transaction_amount = 5000000

[compliance_config.risk_thresholds]
high_risk_amount = 10000000
medium_risk_amount = 1000000
suspicious_patterns = []

[compliance_config.ml_config]
enabled = true
model_version = "v1.0"
confidence_threshold = 0.7
behavioral_weight = 0.25
network_weight = 0.20
geographic_weight = 0.20
temporal_weight = 0.15
device_weight = 0.20

[compliance_config.behavioral_config]
enabled = true
transaction_frequency_threshold = 10.0
amount_deviation_threshold = 3.0
geographic_anomaly_threshold = 0.7
time_pattern_threshold = 0.7
device_anomaly_threshold = 0.7

[[compliance_config.multiple_sanctions_providers]]
provider_type = "ofac"
enabled = true
api_url = "https://api.ofac.example.com"
api_key = "ofac-key"
priority = 1
timeout_seconds = 5

[[compliance_config.multiple_sanctions_providers]]
provider_type = "un"
enabled = true
api_url = "https://api.un-sanctions.example.com"
api_key = "un-key"
priority = 2
timeout_seconds = 5
```

---

## Integration Points

### With Existing Services:

1. **ComplianceService** integration with transaction assessment flow
2. **MetricsService** for compliance metrics recording
3. **AuditLogService** for compliance event logging
4. **AlertService** for high-risk transaction notifications

### With Frontend:

The dashboard endpoints can be consumed by:
- Admin dashboard for compliance team
- Real-time alerts system
- Case management interface
- Compliance reporting module

---

## Future Enhancements

1. **Advanced ML Models**
   - Implement neural networks for risk scoring
   - Deploy pre-trained models (e.g., via ML inference services)

2. **Real-time Streaming**
   - Kafka/Redis integration for event streaming
   - Real-time risk updates

3. **Geographic Risk Database**
   - Country-level risk scoring
   - FATF grey list integration
   - Dynamic risk weighting

4. **Device Fingerprinting**
   - Comprehensive device tracking
   - Device risk database
   - Anomaly detection for device switching

5. **Network Analysis**
   - Graph database integration for transaction networks
   - Entity relationship analysis
   - Cluster detection for organized fraud

6. **Automated Reporting**
   - Scheduled report generation
   - Regulatory reporting format (AML/CFT)
   - Email delivery of summaries

---

## Testing Recommendations

1. **Unit Tests**
   - Behavioral pattern calculation accuracy
   - ML score weighting verification
   - Pattern detection logic

2. **Integration Tests**
   - Database persistence and retrieval
   - API endpoint responses
   - Multiple sanctions provider coordination

3. **Load Tests**
   - Dashboard performance with large datasets
   - ML score computation performance
   - Case retrieval at scale

4. **Regression Tests**
   - Existing compliance checks still functional
   - Backward compatibility with existing assessments

---

## Deployment Notes

1. **Database Migrations**
   - Create new tables for ML models and case management
   - Add indices for query performance
   - Set up replication for high availability

2. **Configuration**
   - Update environment variables for ML config
   - Configure multiple sanctions providers
   - Set case management thresholds

3. **Monitoring**
   - Track ML model performance metrics
   - Monitor case resolution times
   - Alert on compliance anomalies

4. **Compliance**
   - Document ML model for regulatory purposes
   - Maintain audit trail for all decisions
   - Prepare for model interpretability requirements

---

## Files Modified

1. **backend/src/models.rs** - Added ML-related models
2. **backend/src/config.rs** - Enhanced compliance configuration
3. **backend/src/service/compliance_service.rs** - Added ML methods
4. **backend/src/http/admin.rs** - Added dashboard endpoints

---

## Conclusion

This enhancement transforms the compliance service from basic rule-based screening to an advanced ML-powered risk assessment system. It includes:

- ✅ Multiple sanctions database integration
- ✅ Behavioral pattern analysis
- ✅ Real-time ML risk scoring
- ✅ Comprehensive compliance dashboard
- ✅ Automated case management

The implementation provides the compliance team with powerful tools to detect, investigate, and manage financial crime risks in real-time.
