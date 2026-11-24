-- #1218: push notification preferences for circle events.
ALTER TABLE users
    ADD COLUMN IF NOT EXISTS circle_push_enabled BOOLEAN NOT NULL DEFAULT TRUE,
    ADD COLUMN IF NOT EXISTS push_financial_details BOOLEAN NOT NULL DEFAULT FALSE;
