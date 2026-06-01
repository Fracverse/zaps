-- ML-based Risk Scoring and Advanced Compliance Screening for issue #152

-- Table for storing ML risk score calculations
CREATE TABLE IF NOT EXISTS ml_risk_scores (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    assessment_id UUID NOT NULL REFERENCES transaction_risk_assessments(id) ON DELETE CASCADE,
    model_version VARCHAR(50) NOT NULL,
    base_risk_score NUMERIC(5,2) NOT NULL CHECK (base_risk_score >= 0 AND base_risk_score <= 100),
    behavioral_risk NUMERIC(5,2) NOT NULL CHECK (behavioral_risk >= 0 AND behavioral_risk <= 100),
    network_risk NUMERIC(5,2) NOT NULL CHECK (network_risk >= 0 AND network_risk <= 100),
    geographic_risk NUMERIC(5,2) NOT NULL CHECK (geographic_risk >= 0 AND geographic_risk <= 100),
    temporal_risk NUMERIC(5,2) NOT NULL CHECK (temporal_risk >= 0 AND temporal_risk <= 100),
    device_risk NUMERIC(5,2) NOT NULL CHECK (device_risk >= 0 AND device_risk <= 100),
    final_ml_score NUMERIC(5,2) NOT NULL CHECK (final_ml_score >= 0 AND final_ml_score <= 100),
    confidence_level NUMERIC(3,2) NOT NULL CHECK (confidence_level >= 0 AND confidence_level <= 1),
    risk_factors JSONB NOT NULL DEFAULT '[]'::jsonb,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_ml_risk_scores_assessment_id 
    ON ml_risk_scores(assessment_id);
CREATE INDEX IF NOT EXISTS idx_ml_risk_scores_user_created_at
    ON ml_risk_scores USING (final_ml_score, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_ml_risk_scores_model_version
    ON ml_risk_scores(model_version, created_at DESC);

-- Table for risk indicators (suspicious patterns)
CREATE TABLE IF NOT EXISTS risk_indicators (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    assessment_id UUID NOT NULL REFERENCES transaction_risk_assessments(id) ON DELETE CASCADE,
    user_id VARCHAR(255) NOT NULL,
    indicator_type VARCHAR(100) NOT NULL,
    severity VARCHAR(20) NOT NULL CHECK (severity IN ('low', 'medium', 'high', 'critical')),
    description TEXT NOT NULL,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    detected_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_risk_indicators_user_created_at
    ON risk_indicators(user_id, detected_at DESC);
CREATE INDEX IF NOT EXISTS idx_risk_indicators_severity
    ON risk_indicators(severity, detected_at DESC);
CREATE INDEX IF NOT EXISTS idx_risk_indicators_indicator_type
    ON risk_indicators(indicator_type, severity);

-- Table for compliance cases (manual review)
CREATE TABLE IF NOT EXISTS compliance_cases (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id VARCHAR(255) NOT NULL,
    assessment_id UUID REFERENCES transaction_risk_assessments(id) ON DELETE SET NULL,
    case_type VARCHAR(100) NOT NULL,
    status VARCHAR(50) NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'under_investigation', 'escalated', 'resolved', 'closed')),
    priority VARCHAR(20) NOT NULL DEFAULT 'medium' CHECK (priority IN ('low', 'medium', 'high', 'critical')),
    risk_score NUMERIC(5,2) NOT NULL,
    assigned_analyst UUID REFERENCES users(id) ON DELETE SET NULL,
    description TEXT NOT NULL,
    findings TEXT,
    resolution TEXT,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    resolved_at TIMESTAMP WITH TIME ZONE
);

CREATE INDEX IF NOT EXISTS idx_compliance_cases_user_status
    ON compliance_cases(user_id, status, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_compliance_cases_status_priority
    ON compliance_cases(status, priority, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_compliance_cases_created_at
    ON compliance_cases(created_at DESC);

-- Case activity log
CREATE TABLE IF NOT EXISTS case_activity_logs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    case_id UUID NOT NULL REFERENCES compliance_cases(id) ON DELETE CASCADE,
    activity_type VARCHAR(50) NOT NULL,
    performed_by UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    details JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_case_activity_logs_case_id
    ON case_activity_logs(case_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_case_activity_logs_performed_by
    ON case_activity_logs(performed_by, created_at DESC);
