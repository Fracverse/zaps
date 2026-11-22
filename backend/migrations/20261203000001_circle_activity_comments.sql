-- #1214: associate circle comments with the activity they belong to.
ALTER TABLE circle_comments
    ADD COLUMN IF NOT EXISTS activity_id UUID REFERENCES circle_activities (id) ON DELETE CASCADE;

CREATE INDEX IF NOT EXISTS idx_circle_comments_activity
    ON circle_comments (circle_id, activity_id, created_at);
