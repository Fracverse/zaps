-- Behavioral Profiles and Multiple Sanctions Providers for issue #152

-- Table for storing user behavioral profiles
CREATE TABLE IF NOT EXISTS behavioral_profiles (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id VARCHAR(255) NOT NULL UNIQUE,
    average_transaction_amount NUMERIC(20,2) NOT NULL DEFAULT 0,
    transaction_frequency NUMERIC(10,2) NOT NULL DEFAULT 0,
    total_transactions BIGINT NOT NULL DEFAULT 0,
    high_risk_transaction_count BIGINT NOT NULL DEFAULT 0,
    geographic_diversity_score NUMERIC(3,2) NOT NULL DEFAULT 0 CHECK (geographic_diversity_score >= 0 AND geographic_diversity_score <= 1),
    time_pattern_score NUMERIC(3,2) NOT NULL DEFAULT 0 CHECK (time_pattern_score >= 0 AND time_pattern_score <= 1),
    device_diversity_score NUMERIC(3,2) NOT NULL DEFAULT 0 CHECK (device_diversity_score >= 0 AND device_diversity_score <= 1),
    merchant_category_diversity NUMERIC(3,2) NOT NULL DEFAULT 0 CHECK (merchant_category_diversity >= 0 AND merchant_category_diversity <= 1),
    is_high_risk BOOLEAN NOT NULL DEFAULT FALSE,
    risk_score_trend VARCHAR(20) NOT NULL DEFAULT 'stable' CHECK (risk_score_trend IN ('increasing', 'stable', 'decreasing')),
    last_update TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_behavioral_profiles_user_id
    ON behavioral_profiles(user_id);
CREATE INDEX IF NOT EXISTS idx_behavioral_profiles_is_high_risk
    ON behavioral_profiles(is_high_risk, last_update DESC);
CREATE INDEX IF NOT EXISTS idx_behavioral_profiles_risk_trend
    ON behavioral_profiles(risk_score_trend);

-- Table for multiple sanctions provider configurations
CREATE TABLE IF NOT EXISTS sanctions_providers (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(100) NOT NULL UNIQUE,
    provider_type VARCHAR(50) NOT NULL,
    api_url VARCHAR(500) NOT NULL,
    api_key VARCHAR(500) NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    priority INTEGER NOT NULL DEFAULT 100,
    timeout_seconds INTEGER NOT NULL DEFAULT 10,
    health_status VARCHAR(20) NOT NULL DEFAULT 'healthy' CHECK (health_status IN ('healthy', 'degraded', 'down')),
    last_check_at TIMESTAMP WITH TIME ZONE,
    failure_count INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_sanctions_providers_enabled_priority
    ON sanctions_providers(enabled DESC, priority DESC);
CREATE INDEX IF NOT EXISTS idx_sanctions_providers_provider_type
    ON sanctions_providers(provider_type);

-- Table for tracking sanctions screening history across providers
CREATE TABLE IF NOT EXISTS sanctions_screening_history (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id VARCHAR(255) NOT NULL,
    address VARCHAR(128) NOT NULL,
    provider_id UUID NOT NULL REFERENCES sanctions_providers(id) ON DELETE RESTRICT,
    sanctioned BOOLEAN NOT NULL,
    risk_score INTEGER CHECK (risk_score >= 0 AND risk_score <= 100),
    reasons JSONB NOT NULL DEFAULT '[]'::jsonb,
    response_time_ms INTEGER,
    http_status_code INTEGER,
    error_message TEXT,
    screened_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_sanctions_screening_address_provider
    ON sanctions_screening_history(address, provider_id, screened_at DESC);
CREATE INDEX IF NOT EXISTS idx_sanctions_screening_user_screened_at
    ON sanctions_screening_history(user_id, screened_at DESC);
CREATE INDEX IF NOT EXISTS idx_sanctions_screening_sanctioned
    ON sanctions_screening_history(sanctioned) WHERE sanctioned = TRUE;

-- Insert default sanctions providers
INSERT INTO sanctions_providers (name, provider_type, api_url, api_key, enabled, priority)
VALUES 
    ('OFAC', 'ofac', 'https://api.sanctionslist.io/ofac', 'default_key', TRUE, 100),
    ('UN Security Council', 'un', 'https://api.unsanctionslist.io', 'default_key', TRUE, 90),
    ('EU Sanctions', 'eu', 'https://api.eusanctionslist.io', 'default_key', TRUE, 80),
    ('FCA Watchlist', 'fca', 'https://api.fca-watchlist.io', 'default_key', FALSE, 70)
ON CONFLICT (name) DO NOTHING;
