-- #1216: circle announcements with authorship, timestamps and attachments.
CREATE TABLE IF NOT EXISTS circle_announcements (
    id             UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    circle_id      UUID        NOT NULL REFERENCES circles (id) ON DELETE CASCADE,
    author_address TEXT        NOT NULL,
    content        TEXT        NOT NULL,
    attachments    JSONB       NOT NULL DEFAULT '[]'::jsonb,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_circle_announcements_circle
    ON circle_announcements (circle_id, created_at DESC);
