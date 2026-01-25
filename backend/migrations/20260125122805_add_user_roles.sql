-- Migration: add_user_roles
-- Created: 2026-01-25 12:28:05 UTC

-- Add role column to users table
ALTER TABLE users ADD COLUMN IF NOT EXISTS role VARCHAR(20) NOT NULL DEFAULT 'user';

-- Create index on role for faster lookups
CREATE INDEX IF NOT EXISTS idx_users_role ON users(role);

-- Update existing users to have 'user' role (already handled by default)
-- In production, you may want to migrate specific users to admin/merchant roles
