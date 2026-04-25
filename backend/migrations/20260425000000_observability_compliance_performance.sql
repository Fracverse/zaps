-- Observability, compliance, and performance hardening for issues #101, #102, #104, #105.

CREATE TABLE IF NOT EXISTS transaction_risk_assessments (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id VARCHAR(255) NOT NULL,
    address VARCHAR(128) NOT NULL,
    amount BIGINT NOT NULL,
    risk_score INTEGER NOT NULL CHECK (risk_score >= 0 AND risk_score <= 100),
    risk_level VARCHAR(20) NOT NULL,
    sanctions_match BOOLEAN NOT NULL DEFAULT FALSE,
    velocity_limit_exceeded BOOLEAN NOT NULL DEFAULT FALSE,
    reasons JSONB NOT NULL DEFAULT '[]'::jsonb,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_risk_assessments_user_created_at
    ON transaction_risk_assessments(user_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_risk_assessments_address
    ON transaction_risk_assessments(address);
CREATE INDEX IF NOT EXISTS idx_risk_assessments_risk_level
    ON transaction_risk_assessments(risk_level, created_at DESC);

CREATE TABLE IF NOT EXISTS merchant_api_keys (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    merchant_id VARCHAR(255) NOT NULL REFERENCES merchants(merchant_id),
    key_hash VARCHAR(255) NOT NULL UNIQUE,
    tier VARCHAR(50) NOT NULL DEFAULT 'free',
    active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    last_used_at TIMESTAMP WITH TIME ZONE
);

CREATE INDEX IF NOT EXISTS idx_merchant_api_keys_merchant_active
    ON merchant_api_keys(merchant_id, active);

CREATE TABLE IF NOT EXISTS cache_invalidation_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    cache_key VARCHAR(512) NOT NULL,
    reason VARCHAR(255) NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_cache_invalidation_events_created_at
    ON cache_invalidation_events(created_at DESC);

-- Query optimization indexes for high-traffic lookup and reporting paths.
CREATE INDEX IF NOT EXISTS idx_payments_from_address_created_at
    ON payments(from_address, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_payments_merchant_created_at
    ON payments(merchant_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_payments_status_created_at
    ON payments(status, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_transfers_from_created_at
    ON transfers(from_user_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_transfers_to_created_at
    ON transfers(to_user_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_withdrawals_user_created_at
    ON withdrawals(user_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_withdrawals_status_created_at
    ON withdrawals(status, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_bridge_transactions_user_created_at
    ON bridge_transactions(user_id, created_at DESC);

-- Keep audit search fast and support a retention job/policy outside the app.
CREATE INDEX IF NOT EXISTS idx_audit_logs_timestamp_brin
    ON audit_logs USING BRIN(timestamp);
