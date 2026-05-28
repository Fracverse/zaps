-- Multi-Currency Support
-- Creates tables for storing exchange rates and currency-related data.

-- -------------------------------------------------------------------------
-- exchange_rates
-- -------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS exchange_rates (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    from_currency   VARCHAR(3) NOT NULL,
    to_currency     VARCHAR(3) NOT NULL,
    rate            NUMERIC(20, 10) NOT NULL,
    source          VARCHAR(100),
    last_updated    TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    created_at      TIMESTAMP WITH TIME ZONE DEFAULT NOW(),

    CONSTRAINT chk_currency_code CHECK (
        from_currency IN ('USD', 'EUR', 'GBP', 'JPY') AND
        to_currency IN ('USD', 'EUR', 'GBP', 'JPY')
    ),
    CONSTRAINT chk_rate_positive CHECK (rate > 0),
    CONSTRAINT uq_currency_pair UNIQUE (from_currency, to_currency)
);

-- -------------------------------------------------------------------------
-- Indexes
-- -------------------------------------------------------------------------
CREATE INDEX IF NOT EXISTS idx_exchange_rates_currency_pair
    ON exchange_rates(from_currency, to_currency);

CREATE INDEX IF NOT EXISTS idx_exchange_rates_last_updated
    ON exchange_rates(last_updated DESC);
