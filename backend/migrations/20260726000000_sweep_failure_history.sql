-- BE-053: Track sweep failures per user so a user whose sweep keeps failing
-- gets temporarily skipped (with increasing backoff) instead of being
-- retried — and logged as a warning — every single sweep cycle.
CREATE TABLE IF NOT EXISTS sweep_failure_history (
    user_id UUID PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    failure_count INTEGER NOT NULL DEFAULT 0 CHECK (failure_count >= 0),
    last_error TEXT,
    last_failed_at TIMESTAMP NOT NULL DEFAULT NOW(),
    -- Worker skips this user while now() < next_retry_at.
    next_retry_at TIMESTAMP NOT NULL DEFAULT NOW()
);

-- Hot-path check: "is this user currently in backoff?"
CREATE INDEX IF NOT EXISTS idx_sweep_failure_history_next_retry
    ON sweep_failure_history(next_retry_at);