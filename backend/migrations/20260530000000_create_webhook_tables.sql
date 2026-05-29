-- Webhook System Tables
-- Migration: 20260530000000_create_webhook_tables.sql

-- -------------------------------------------------------------------------
-- webhook_endpoints table for storing registered webhook URLs
-- -------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS webhook_endpoints (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    merchant_id VARCHAR(255) NOT NULL,
    url VARCHAR(2048) NOT NULL,
    secret VARCHAR(255) NOT NULL,
    events TEXT[] NOT NULL,
    is_active BOOLEAN DEFAULT true,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- -------------------------------------------------------------------------
-- webhook_deliveries table for tracking webhook delivery attempts
-- -------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS webhook_deliveries (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    endpoint_id UUID NOT NULL REFERENCES webhook_endpoints(id) ON DELETE CASCADE,
    event_type VARCHAR(100) NOT NULL,
    payload JSONB NOT NULL,
    status VARCHAR(50) DEFAULT 'pending',
    attempt_count INTEGER DEFAULT 0,
    next_retry_at TIMESTAMP WITH TIME ZONE,
    response_status INTEGER,
    error_message TEXT,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- -------------------------------------------------------------------------
-- Indexes for webhook_endpoints
-- -------------------------------------------------------------------------
CREATE INDEX IF NOT EXISTS idx_webhook_endpoints_merchant_id ON webhook_endpoints(merchant_id);
CREATE INDEX IF NOT EXISTS idx_webhook_endpoints_is_active ON webhook_endpoints(is_active);
CREATE INDEX IF NOT EXISTS idx_webhook_endpoints_events ON webhook_endpoints USING GIN(events);

-- -------------------------------------------------------------------------
-- Indexes for webhook_deliveries
-- -------------------------------------------------------------------------
CREATE INDEX IF NOT EXISTS idx_webhook_deliveries_endpoint_id ON webhook_deliveries(endpoint_id);
CREATE INDEX IF NOT EXISTS idx_webhook_deliveries_status ON webhook_deliveries(status);
CREATE INDEX IF NOT EXISTS idx_webhook_deliveries_next_retry_at ON webhook_deliveries(next_retry_at);
CREATE INDEX IF NOT EXISTS idx_webhook_deliveries_event_type ON webhook_deliveries(event_type);

-- -------------------------------------------------------------------------
-- Add updated_at trigger for webhook_endpoints
-- -------------------------------------------------------------------------
CREATE OR REPLACE FUNCTION update_webhook_endpoints_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trigger_update_webhook_endpoints_updated_at
    BEFORE UPDATE ON webhook_endpoints
    FOR EACH ROW
    EXECUTE FUNCTION update_webhook_endpoints_updated_at();

-- -------------------------------------------------------------------------
-- Add updated_at trigger for webhook_deliveries
-- -------------------------------------------------------------------------
CREATE OR REPLACE FUNCTION update_webhook_deliveries_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trigger_update_webhook_deliveries_updated_at
    BEFORE UPDATE ON webhook_deliveries
    FOR EACH ROW
    EXECUTE FUNCTION update_webhook_deliveries_updated_at();
