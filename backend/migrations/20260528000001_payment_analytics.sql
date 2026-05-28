-- Payment Analytics
-- Creates tables for storing aggregated analytics data for performance.

-- -------------------------------------------------------------------------
-- payment_analytics_daily
-- -------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS payment_analytics_daily (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    merchant_id     VARCHAR(255),
    date            DATE NOT NULL,
    total_payments  BIGINT NOT NULL DEFAULT 0,
    total_amount    NUMERIC(20, 10) NOT NULL DEFAULT 0,
    successful_payments BIGINT NOT NULL DEFAULT 0,
    failed_payments BIGINT NOT NULL DEFAULT 0,
    created_at      TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at      TIMESTAMP WITH TIME ZONE DEFAULT NOW(),

    CONSTRAINT uq_merchant_date UNIQUE (merchant_id, date)
);

-- -------------------------------------------------------------------------
-- Indexes
-- -------------------------------------------------------------------------
CREATE INDEX IF NOT EXISTS idx_payment_analytics_merchant_date
    ON payment_analytics_daily(merchant_id, date DESC);

CREATE INDEX IF NOT EXISTS idx_payment_analytics_date
    ON payment_analytics_daily(date DESC);
