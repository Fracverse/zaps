-- #1220: Circle member search — verified identifiers and per-user privacy.

ALTER TABLE users
    ADD COLUMN IF NOT EXISTS circle_search_enabled BOOLEAN NOT NULL DEFAULT TRUE;

-- Approved (verified) identifiers are stored hashed. Raw phone numbers,
-- emails or government IDs are never persisted in the search index or
-- returned by any API.
CREATE TABLE IF NOT EXISTS user_verified_identifiers (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id         UUID        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    identifier_type TEXT        NOT NULL CHECK (identifier_type IN ('email', 'phone', 'privy_did')),
    identifier_hash TEXT        NOT NULL,
    is_verified     BOOLEAN     NOT NULL DEFAULT FALSE,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (identifier_type, identifier_hash)
);

CREATE INDEX IF NOT EXISTS idx_user_verified_identifiers_user
    ON user_verified_identifiers (user_id);
