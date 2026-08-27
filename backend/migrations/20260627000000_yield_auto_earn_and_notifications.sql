-- BE-030: auto-earn preference on user profile
ALTER TABLE users
    ADD COLUMN IF NOT EXISTS auto_earn_enabled BOOLEAN NOT NULL DEFAULT false;

-- BE-031: track last on-chain / indexer sync for off-chain yield estimates
ALTER TABLE user_yield_balances
    ADD COLUMN IF NOT EXISTS last_yield_sync_at TIMESTAMP NOT NULL DEFAULT NOW();

-- BE-032: Expo push tokens for yield report notifications
CREATE TABLE IF NOT EXISTS user_push_tokens (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    expo_push_token TEXT NOT NULL,
    platform VARCHAR(20),
    created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP NOT NULL DEFAULT NOW(),
    UNIQUE (user_id, expo_push_token)
);

CREATE INDEX IF NOT EXISTS idx_user_push_tokens_user_id ON user_push_tokens(user_id);

-- BE-032: throttle daily/weekly yield report notifications per user
ALTER TABLE users
    ADD COLUMN IF NOT EXISTS last_daily_yield_report_at TIMESTAMP,
    ADD COLUMN IF NOT EXISTS last_weekly_yield_report_at TIMESTAMP;

CREATE INDEX IF NOT EXISTS idx_users_auto_earn ON users(auto_earn_enabled)
    WHERE auto_earn_enabled = true;
