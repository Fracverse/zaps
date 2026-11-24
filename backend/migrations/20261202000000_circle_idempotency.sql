-- #1224: Idempotency records for circle financial operations.
CREATE TABLE IF NOT EXISTS circle_idempotency_records (
    id              UUID PRIMARY KEY,
    scope           TEXT NOT NULL,          -- authenticated user (or client IP)
    operation       TEXT NOT NULL,          -- contribute / payout / approve
    idempotency_key TEXT NOT NULL,
    request_hash    TEXT NOT NULL,
    status          TEXT NOT NULL DEFAULT 'in_progress',
    response_status INTEGER,
    response_body   TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    completed_at    TIMESTAMPTZ,
    UNIQUE (scope, operation, idempotency_key)
);

CREATE INDEX IF NOT EXISTS circle_idempotency_retention_idx
    ON circle_idempotency_records (created_at);
