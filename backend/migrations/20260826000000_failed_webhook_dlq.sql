-- #806: Dead-letter queue for push/webhook dispatches that fail 5 times.
-- Stores the original payload, last error, and retry count for replay/audit.
CREATE TABLE IF NOT EXISTS failed_webhook_dlq (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    destination TEXT NOT NULL,
    payload JSONB NOT NULL,
    error_message TEXT NOT NULL,
    retry_count INTEGER NOT NULL DEFAULT 5 CHECK (retry_count >= 0),
    created_at TIMESTAMP NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_failed_webhook_dlq_created_at
    ON failed_webhook_dlq(created_at DESC);
