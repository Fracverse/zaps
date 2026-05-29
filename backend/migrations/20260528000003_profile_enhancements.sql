-- Profile Enhancements: verification, preferences, activity history
-- Migration: 20260528000003_profile_enhancements.sql

-- Add verification status to user_profiles
ALTER TABLE user_profiles
ADD COLUMN IF NOT EXISTS verification_status VARCHAR(20) NOT NULL DEFAULT 'unverified'
CHECK (verification_status IN ('unverified', 'pending', 'verified', 'rejected'));

-- Create user_preferences table
CREATE TABLE IF NOT EXISTS user_preferences (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    preferences JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT user_preferences_user_id_key UNIQUE (user_id)
);

-- Create profile_activity table
CREATE TABLE IF NOT EXISTS profile_activity (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    activity_type VARCHAR(50) NOT NULL,
    description TEXT,
    metadata JSONB DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Index for activity queries
CREATE INDEX IF NOT EXISTS idx_profile_activity_user_id ON profile_activity(user_id);
CREATE INDEX IF NOT EXISTS idx_profile_activity_created_at ON profile_activity(created_at DESC);

-- Trigger for user_preferences updated_at
CREATE OR REPLACE FUNCTION update_user_preferences_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER update_user_preferences_updated_at
    BEFORE UPDATE ON user_preferences
    FOR EACH ROW
    EXECUTE FUNCTION update_user_preferences_updated_at();
