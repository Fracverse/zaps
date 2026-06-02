-- Migration: create payment schedules and schedule runs
CREATE TABLE IF NOT EXISTS payment_schedules (
    id UUID PRIMARY KEY,
    merchant_id TEXT NOT NULL,
    from_address TEXT NOT NULL,
    to_address TEXT NOT NULL,
    send_asset TEXT NOT NULL,
    send_amount BIGINT NOT NULL,
    memo TEXT,
    schedule_type TEXT NOT NULL,
    interval_seconds BIGINT,
    next_run TIMESTAMPTZ NOT NULL,
    status TEXT NOT NULL,
    retries INT DEFAULT 0,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_payment_schedules_next_run ON payment_schedules (next_run);

CREATE TABLE IF NOT EXISTS payment_schedule_runs (
    id UUID PRIMARY KEY,
    schedule_id UUID NOT NULL REFERENCES payment_schedules(id) ON DELETE CASCADE,
    attempted_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    success BOOLEAN NOT NULL,
    error TEXT,
    external_payment_id TEXT
);
