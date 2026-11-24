-- #1217: Circle notification service.
-- Extends the existing circle_notifications table with standardized types,
-- deduplication keys, read tracking and delivery state.

ALTER TABLE circle_notifications
    ADD COLUMN IF NOT EXISTS dedupe_key TEXT,
    ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW();

-- Standardize notification types so every producer uses the same vocabulary.
-- Existing rows are left untouched; new writes are constrained by the service.
CREATE UNIQUE INDEX IF NOT EXISTS idx_circle_notifications_dedupe
    ON circle_notifications (dedupe_key) WHERE dedupe_key IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_circle_notifications_type
    ON circle_notifications (notification_type, created_at DESC);

-- Per-channel delivery attempts. Failures stay in this table with an
-- exponential backoff so a worker can retry them; they are never silently lost.
CREATE TABLE IF NOT EXISTS circle_notification_deliveries (
    id               UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    notification_id  UUID        NOT NULL REFERENCES circle_notifications (id) ON DELETE CASCADE,
    channel          TEXT        NOT NULL,
    status           TEXT        NOT NULL DEFAULT 'pending'
                                 CHECK (status IN ('pending', 'delivered', 'failed')),
    attempts         INT         NOT NULL DEFAULT 0,
    next_retry_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_error       TEXT,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (notification_id, channel)
);

CREATE INDEX IF NOT EXISTS idx_circle_notification_deliveries_retry
    ON circle_notification_deliveries (status, next_retry_at)
    WHERE status = 'pending';
