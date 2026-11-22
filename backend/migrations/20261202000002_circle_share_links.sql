-- #1221: Secure circle/invitation share links and deep links.
-- Only SHA-256 hashes of opaque tokens are stored; the raw token is returned
-- to the creator exactly once and cannot be recovered from the database.

CREATE TABLE IF NOT EXISTS circle_share_links (
    id            UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    circle_id     UUID        NOT NULL REFERENCES circles (id) ON DELETE CASCADE,
    created_by    TEXT        NOT NULL,
    token_hash    TEXT        NOT NULL UNIQUE,
    scope         TEXT        NOT NULL DEFAULT 'circle'
                              CHECK (scope IN ('circle', 'invitation')),
    invitation_id UUID        REFERENCES circle_invitations (id) ON DELETE CASCADE,
    expires_at    TIMESTAMPTZ,
    revoked_at    TIMESTAMPTZ,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_circle_share_links_circle
    ON circle_share_links (circle_id, created_at DESC);

CREATE TABLE IF NOT EXISTS circle_share_access_logs (
    id           UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    link_id      UUID        NOT NULL REFERENCES circle_share_links (id) ON DELETE CASCADE,
    accessor     TEXT,
    user_agent   TEXT,
    ip_address   TEXT,
    outcome      TEXT        NOT NULL
                             CHECK (outcome IN ('allowed', 'denied_unauthorized',
                                                'denied_expired', 'denied_revoked',
                                                'denied_not_found')),
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_circle_share_access_logs_link
    ON circle_share_access_logs (link_id, created_at DESC);
