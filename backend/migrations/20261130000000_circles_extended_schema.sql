-- #1193: Extended Circles Schema
-- Adds product-facing columns to `circles`, plus new tables for invitations,
-- membership requests, contribution rules, activities, comments, reactions,
-- and notifications required by the Zaps Circles feature.

-- ── Extend the existing `circles` table ──────────────────────────────────────

ALTER TABLE circles
    ADD COLUMN IF NOT EXISTS description   TEXT,
    ADD COLUMN IF NOT EXISTS category      TEXT        NOT NULL DEFAULT 'general',
    ADD COLUMN IF NOT EXISTS visibility    TEXT        NOT NULL DEFAULT 'public'
                                           CHECK (visibility IN ('public', 'private', 'invite_only')),
    ADD COLUMN IF NOT EXISTS image_url     TEXT,
    ADD COLUMN IF NOT EXISTS target_amount BIGINT      NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS is_discoverable BOOLEAN  NOT NULL DEFAULT TRUE,
    ADD COLUMN IF NOT EXISTS member_limit  INT;

-- Back-fill: existing circles stay discoverable and public.
UPDATE circles SET is_discoverable = TRUE WHERE is_discoverable IS NULL;

CREATE INDEX IF NOT EXISTS idx_circles_category        ON circles (category);
CREATE INDEX IF NOT EXISTS idx_circles_visibility      ON circles (visibility);
CREATE INDEX IF NOT EXISTS idx_circles_is_discoverable ON circles (is_discoverable);
CREATE INDEX IF NOT EXISTS idx_circles_created_at      ON circles (created_at DESC);

-- ── Circle invitations ────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS circle_invitations (
    id            UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    circle_id     UUID        NOT NULL REFERENCES circles (id) ON DELETE CASCADE,
    inviter_address TEXT      NOT NULL,
    invitee_address TEXT      NOT NULL,
    status        TEXT        NOT NULL DEFAULT 'pending'
                              CHECK (status IN ('pending', 'accepted', 'declined', 'expired')),
    expires_at    TIMESTAMPTZ,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (circle_id, invitee_address)
);

CREATE INDEX IF NOT EXISTS idx_circle_invitations_circle  ON circle_invitations (circle_id);
CREATE INDEX IF NOT EXISTS idx_circle_invitations_invitee ON circle_invitations (invitee_address);

-- ── Membership requests ───────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS circle_membership_requests (
    id            UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    circle_id     UUID        NOT NULL REFERENCES circles (id) ON DELETE CASCADE,
    requester_address TEXT    NOT NULL,
    message       TEXT,
    status        TEXT        NOT NULL DEFAULT 'pending'
                              CHECK (status IN ('pending', 'approved', 'rejected')),
    reviewed_by   TEXT,
    reviewed_at   TIMESTAMPTZ,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (circle_id, requester_address)
);

CREATE INDEX IF NOT EXISTS idx_circle_membership_requests_circle
    ON circle_membership_requests (circle_id);
CREATE INDEX IF NOT EXISTS idx_circle_membership_requests_requester
    ON circle_membership_requests (requester_address);

-- ── Contribution rules ────────────────────────────────────────────────────────
-- Defines recurring contribution schedules (e.g. weekly 10 USDC).

CREATE TABLE IF NOT EXISTS circle_contribution_rules (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    circle_id       UUID        NOT NULL REFERENCES circles (id) ON DELETE CASCADE,
    min_amount      BIGINT      NOT NULL DEFAULT 0,
    max_amount      BIGINT,
    frequency       TEXT        NOT NULL DEFAULT 'none'
                                CHECK (frequency IN ('none', 'daily', 'weekly', 'monthly')),
    is_mandatory    BOOLEAN     NOT NULL DEFAULT FALSE,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_circle_contribution_rules_circle
    ON circle_contribution_rules (circle_id);

-- ── Savings goals (already exists; ensure no-op if re-run) ───────────────────
-- (circle_goals already created by 20261121000000_create_circles_tables.sql)

-- ── Circle activities (audit log) ────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS circle_activities (
    id            UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    circle_id     UUID        NOT NULL REFERENCES circles (id) ON DELETE CASCADE,
    actor_address TEXT        NOT NULL,
    activity_type TEXT        NOT NULL,   -- e.g. 'member_joined', 'contribution', 'payout_requested'
    metadata      JSONB       NOT NULL DEFAULT '{}',
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_circle_activities_circle     ON circle_activities (circle_id);
CREATE INDEX IF NOT EXISTS idx_circle_activities_created_at ON circle_activities (created_at DESC);

-- ── Comments ──────────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS circle_comments (
    id             UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    circle_id      UUID        NOT NULL REFERENCES circles (id) ON DELETE CASCADE,
    author_address TEXT        NOT NULL,
    content        TEXT        NOT NULL,
    parent_id      UUID        REFERENCES circle_comments (id) ON DELETE CASCADE,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_circle_comments_circle ON circle_comments (circle_id);

-- ── Reactions ─────────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS circle_reactions (
    circle_id      UUID        NOT NULL REFERENCES circles (id) ON DELETE CASCADE,
    actor_address  TEXT        NOT NULL,
    reaction_type  TEXT        NOT NULL DEFAULT 'like',
    created_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (circle_id, actor_address, reaction_type)
);

CREATE INDEX IF NOT EXISTS idx_circle_reactions_circle ON circle_reactions (circle_id);

-- ── Notifications ─────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS circle_notifications (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    circle_id       UUID        REFERENCES circles (id) ON DELETE CASCADE,
    recipient_address TEXT      NOT NULL,
    notification_type TEXT      NOT NULL,
    title           TEXT        NOT NULL,
    body            TEXT        NOT NULL,
    metadata        JSONB       NOT NULL DEFAULT '{}',
    is_read         BOOLEAN     NOT NULL DEFAULT FALSE,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_circle_notifications_recipient
    ON circle_notifications (recipient_address, is_read, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_circle_notifications_circle
    ON circle_notifications (circle_id);
