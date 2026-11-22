-- #1197, #1198, #1201, #1219: Circles Search, Membership Roles, and Reminder Scheduler

-- 1. Ensure trigram extension for fast partial-match search
CREATE EXTENSION IF NOT EXISTS pg_trgm;

-- Trigram GIN indexes on circles for fast partial matching (O(log N) lookup)
CREATE INDEX IF NOT EXISTS idx_circles_name_trgm ON circles USING GIN (LOWER(name) gin_trgm_ops);
CREATE INDEX IF NOT EXISTS idx_circles_desc_trgm ON circles USING GIN (LOWER(COALESCE(description, '')) gin_trgm_ops);

-- 2. Add role column to circle_members
ALTER TABLE circle_members
    ADD COLUMN IF NOT EXISTS role TEXT NOT NULL DEFAULT 'member'
    CHECK (role IN ('owner', 'admin', 'moderator', 'member'));

-- Backfill existing circle admins as 'owner'
UPDATE circle_members cm
SET role = 'owner'
FROM circles c
WHERE c.id = cm.circle_id AND c.admin_address = cm.address AND cm.role = 'member';

CREATE INDEX IF NOT EXISTS idx_circle_members_role ON circle_members (circle_id, role);

-- 3. Membership history: persistent audit log for joins, leaves, removals, and role changes
CREATE TABLE IF NOT EXISTS circle_membership_history (
    id             UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    circle_id      UUID        NOT NULL REFERENCES circles (id) ON DELETE CASCADE,
    member_address TEXT        NOT NULL,
    action         TEXT        NOT NULL
                               CHECK (action IN ('joined', 'left', 'removed', 'role_changed', 'approved', 'rejected')),
    actor_address  TEXT        NOT NULL,
    old_role       TEXT,
    new_role       TEXT,
    reason         TEXT,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_circle_membership_history_circle
    ON circle_membership_history (circle_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_circle_membership_history_member
    ON circle_membership_history (member_address);

-- 4. Reminder preferences for members
CREATE TABLE IF NOT EXISTS circle_reminder_preferences (
    circle_id        UUID        NOT NULL REFERENCES circles (id) ON DELETE CASCADE,
    member_address   TEXT        NOT NULL,
    enabled          BOOLEAN     NOT NULL DEFAULT TRUE,
    lead_time_hours  INT         NOT NULL DEFAULT 24 CHECK (lead_time_hours >= 1 AND lead_time_hours <= 168),
    channels         TEXT[]      NOT NULL DEFAULT ARRAY['in_app']::TEXT[],
    created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (circle_id, member_address)
);

CREATE INDEX IF NOT EXISTS idx_circle_reminder_prefs_circle
    ON circle_reminder_preferences (circle_id);

-- 5. Reminder logs to prevent duplicate reminder generation
CREATE TABLE IF NOT EXISTS circle_reminder_logs (
    id               UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    circle_id        UUID        NOT NULL REFERENCES circles (id) ON DELETE CASCADE,
    member_address   TEXT        NOT NULL,
    reminder_type    TEXT        NOT NULL CHECK (reminder_type IN ('upcoming', 'overdue')),
    cycle_reference  TEXT        NOT NULL, -- unique period identifier, e.g. 'weekly_20261122'
    sent_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (circle_id, member_address, reminder_type, cycle_reference)
);

CREATE INDEX IF NOT EXISTS idx_circle_reminder_logs_lookup
    ON circle_reminder_logs (circle_id, member_address, cycle_reference);

