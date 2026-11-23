-- #1208: contribution history.

CREATE INDEX IF NOT EXISTS idx_circle_contributions_history
    ON circle_contributions (circle_id, created_at DESC, id DESC);
CREATE INDEX IF NOT EXISTS idx_circle_contributions_status
    ON circle_contributions (circle_id, status);
