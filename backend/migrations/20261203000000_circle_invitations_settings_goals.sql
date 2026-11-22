-- #1199, #1200, #1202, #1204: Circle invitations, settings and savings goals.

-- ── Invitations (#1199 / #1200) ──────────────────────────────────────────────
-- Invitations are either *direct* (addressed to one Zaps user) or *link*
-- (a shareable token any authenticated user can redeem, optionally capped by
-- `max_uses`). Only the SHA-256 hash of a link token is stored.

ALTER TABLE circle_invitations
    ALTER COLUMN invitee_address DROP NOT NULL,
    ADD COLUMN IF NOT EXISTS invite_type  TEXT        NOT NULL DEFAULT 'direct',
    ADD COLUMN IF NOT EXISTS token_hash   TEXT,
    ADD COLUMN IF NOT EXISTS max_uses     INT,
    ADD COLUMN IF NOT EXISTS use_count    INT         NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS responded_by TEXT,
    ADD COLUMN IF NOT EXISTS responded_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS updated_at   TIMESTAMPTZ NOT NULL DEFAULT NOW();

ALTER TABLE circle_invitations
    DROP CONSTRAINT IF EXISTS circle_invitations_status_check;
ALTER TABLE circle_invitations
    ADD CONSTRAINT circle_invitations_status_check
    CHECK (status IN ('pending', 'accepted', 'declined', 'expired', 'revoked'));

ALTER TABLE circle_invitations
    DROP CONSTRAINT IF EXISTS circle_invitations_invite_type_check;
ALTER TABLE circle_invitations
    ADD CONSTRAINT circle_invitations_invite_type_check
    CHECK (
        (invite_type = 'direct' AND invitee_address IS NOT NULL AND token_hash IS NULL)
        OR (invite_type = 'link' AND token_hash IS NOT NULL)
    );

ALTER TABLE circle_invitations
    DROP CONSTRAINT IF EXISTS circle_invitations_uses_check;
ALTER TABLE circle_invitations
    ADD CONSTRAINT circle_invitations_uses_check
    CHECK (use_count >= 0 AND (max_uses IS NULL OR max_uses > 0));

-- A user could previously be invited to a circle only once, ever. Re-inviting
-- after a decline/expiry is legitimate, so uniqueness only applies to the one
-- *active* direct invitation per (circle, invitee).
ALTER TABLE circle_invitations
    DROP CONSTRAINT IF EXISTS circle_invitations_circle_id_invitee_address_key;
CREATE UNIQUE INDEX IF NOT EXISTS idx_circle_invitations_active_invitee
    ON circle_invitations (circle_id, invitee_address)
    WHERE status = 'pending' AND invite_type = 'direct';

CREATE UNIQUE INDEX IF NOT EXISTS idx_circle_invitations_token_hash
    ON circle_invitations (token_hash)
    WHERE token_hash IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_circle_invitations_status_expiry
    ON circle_invitations (status, expires_at);

-- One row per redemption so link invitations stay idempotent per user.
CREATE TABLE IF NOT EXISTS circle_invitation_redemptions (
    invitation_id UUID        NOT NULL REFERENCES circle_invitations (id) ON DELETE CASCADE,
    address       TEXT        NOT NULL,
    redeemed_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (invitation_id, address)
);

-- ── Circle settings (#1202) ──────────────────────────────────────────────────

ALTER TABLE circles
    ADD COLUMN IF NOT EXISTS invitation_expiry_hours INT     NOT NULL DEFAULT 168,
    ADD COLUMN IF NOT EXISTS allow_join_requests     BOOLEAN NOT NULL DEFAULT TRUE,
    ADD COLUMN IF NOT EXISTS max_active_goals        INT     NOT NULL DEFAULT 5;

ALTER TABLE circles
    DROP CONSTRAINT IF EXISTS circles_settings_check;
ALTER TABLE circles
    ADD CONSTRAINT circles_settings_check
    CHECK (
        invitation_expiry_hours BETWEEN 1 AND 720
        AND max_active_goals BETWEEN 1 AND 50
        AND (member_limit IS NULL OR member_limit >= 2)
    );

-- ── Savings goals (#1204) ────────────────────────────────────────────────────

ALTER TABLE circle_goals
    ADD COLUMN IF NOT EXISTS description  TEXT,
    ADD COLUMN IF NOT EXISTS created_by   TEXT,
    ADD COLUMN IF NOT EXISTS completed_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS completed_by TEXT,
    ADD COLUMN IF NOT EXISTS updated_at   TIMESTAMPTZ NOT NULL DEFAULT NOW();

ALTER TABLE circle_goals
    DROP CONSTRAINT IF EXISTS circle_goals_target_check;
ALTER TABLE circle_goals
    ADD CONSTRAINT circle_goals_target_check CHECK (target_amount > 0);

CREATE INDEX IF NOT EXISTS idx_circle_goals_circle_open
    ON circle_goals (circle_id, completed, deadline);
