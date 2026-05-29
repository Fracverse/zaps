-- Audit Logging Enhancements
-- Migration: 20260529000001_audit_logging_enhancements.sql

-- -------------------------------------------------------------------------
-- audit_logs_archive table for log retention/archival
-- -------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS audit_logs_archive (
    id UUID PRIMARY KEY,
    actor_id VARCHAR(255) NOT NULL,
    action VARCHAR(100) NOT NULL,
    resource VARCHAR(100) NOT NULL,
    resource_id VARCHAR(255),
    metadata JSONB,
    timestamp TIMESTAMP WITH TIME ZONE NOT NULL,
    ip_address INET,
    user_agent TEXT,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL,
    archived_at TIMESTAMP WITH TIME ZONE DEFAULT NOW() NOT NULL
);

-- -------------------------------------------------------------------------
-- Indexes for archive table
-- -------------------------------------------------------------------------
CREATE INDEX IF NOT EXISTS idx_audit_logs_archive_actor_id ON audit_logs_archive(actor_id, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_audit_logs_archive_action ON audit_logs_archive(action, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_audit_logs_archive_timestamp ON audit_logs_archive(timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_audit_logs_archive_archived_at ON audit_logs_archive(archived_at DESC);

-- -------------------------------------------------------------------------
-- Add sensitive operation tracking
-- -------------------------------------------------------------------------
-- Ensure audit_logs table has proper constraints for sensitive operations
ALTER TABLE audit_logs
ADD CONSTRAINT chk_audit_action CHECK (
    action IN (
        'auth_success', 'auth_failed', 'payment', 'transfer', 'withdrawal',
        'payout', 'admin_action', 'user_created', 'user_deleted',
        'role_changed', 'suspicious_activity', 'exchange_rate_updated',
        'payout_created', 'payout_batch_created', 'payout_retry'
    )
) ON CONFLICT DO NOTHING;

-- -------------------------------------------------------------------------
-- Create audit statistics view
-- -------------------------------------------------------------------------
CREATE OR REPLACE VIEW audit_statistics AS
SELECT
    DATE_TRUNC('day', timestamp) as date,
    action,
    COUNT(*) as count,
    COUNT(DISTINCT actor_id) as unique_actors
FROM audit_logs
GROUP BY DATE_TRUNC('day', timestamp), action
ORDER BY date DESC, count DESC;

-- -------------------------------------------------------------------------
-- Create suspicious activity view
-- -------------------------------------------------------------------------
CREATE OR REPLACE VIEW suspicious_activity_view AS
SELECT
    id,
    actor_id,
    action,
    resource,
    resource_id,
    timestamp,
    ip_address,
    user_agent
FROM audit_logs
WHERE action IN ('auth_failed', 'suspicious_activity')
ORDER BY timestamp DESC;
