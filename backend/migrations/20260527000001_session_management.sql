-- User Session Management
-- Creates tables for session tracking, device fingerprinting, and security monitoring.

-- -------------------------------------------------------------------------
-- user_sessions
-- -------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS user_sessions (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id         VARCHAR(255) NOT NULL,
    session_token   VARCHAR(512) UNIQUE NOT NULL,
    device_id       VARCHAR(255) NOT NULL,
    device_fingerprint VARCHAR(512) NOT NULL,
    ip_address      VARCHAR(45) NOT NULL,
    user_agent      TEXT NOT NULL,
    status          VARCHAR(50) NOT NULL DEFAULT 'active',
    last_activity   TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    created_at      TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    expires_at      TIMESTAMP WITH TIME ZONE NOT NULL,

    CONSTRAINT fk_session_user
        FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,

    CONSTRAINT chk_session_status CHECK (
        status IN ('active', 'suspended', 'revoked', 'expired')
    )
);

-- -------------------------------------------------------------------------
-- session_activity_log
-- -------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS session_activity_log (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    session_id      UUID NOT NULL,
    user_id         VARCHAR(255) NOT NULL,
    activity_type   VARCHAR(100) NOT NULL,
    endpoint        VARCHAR(255),
    method          VARCHAR(10),
    status_code     INTEGER,
    ip_address      VARCHAR(45),
    user_agent      TEXT,
    created_at      TIMESTAMP WITH TIME ZONE DEFAULT NOW(),

    CONSTRAINT fk_activity_session
        FOREIGN KEY (session_id) REFERENCES user_sessions(id) ON DELETE CASCADE,

    CONSTRAINT fk_activity_user
        FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

-- -------------------------------------------------------------------------
-- session_security_events
-- -------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS session_security_events (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id         VARCHAR(255) NOT NULL,
    session_id      UUID,
    event_type      VARCHAR(100) NOT NULL,
    severity        VARCHAR(50) NOT NULL DEFAULT 'medium',
    description     TEXT NOT NULL,
    ip_address      VARCHAR(45),
    device_id       VARCHAR(255),
    action_taken    VARCHAR(100),
    created_at      TIMESTAMP WITH TIME ZONE DEFAULT NOW(),

    CONSTRAINT fk_security_event_user
        FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,

    CONSTRAINT fk_security_event_session
        FOREIGN KEY (session_id) REFERENCES user_sessions(id) ON DELETE SET NULL,

    CONSTRAINT chk_event_type CHECK (
        event_type IN ('login', 'logout', 'failed_login', 'suspicious_activity', 'device_change', 'location_change', 'concurrent_session')
    ),

    CONSTRAINT chk_severity CHECK (
        severity IN ('low', 'medium', 'high', 'critical')
    )
);

-- -------------------------------------------------------------------------
-- Indexes
-- -------------------------------------------------------------------------
CREATE INDEX IF NOT EXISTS idx_sessions_user_id
    ON user_sessions(user_id);

CREATE INDEX IF NOT EXISTS idx_sessions_status
    ON user_sessions(status);

CREATE INDEX IF NOT EXISTS idx_sessions_expires_at
    ON user_sessions(expires_at);

CREATE INDEX IF NOT EXISTS idx_sessions_device_id
    ON user_sessions(device_id);

CREATE INDEX IF NOT EXISTS idx_activity_session_id
    ON session_activity_log(session_id);

CREATE INDEX IF NOT EXISTS idx_activity_user_id
    ON session_activity_log(user_id);

CREATE INDEX IF NOT EXISTS idx_activity_created_at
    ON session_activity_log(created_at DESC);

CREATE INDEX IF NOT EXISTS idx_security_events_user_id
    ON session_security_events(user_id);

CREATE INDEX IF NOT EXISTS idx_security_events_session_id
    ON session_security_events(session_id);

CREATE INDEX IF NOT EXISTS idx_security_events_event_type
    ON session_security_events(event_type);

CREATE INDEX IF NOT EXISTS idx_security_events_created_at
    ON session_security_events(created_at DESC);

-- -------------------------------------------------------------------------
-- Trigger: auto-update last_activity on user_sessions
-- -------------------------------------------------------------------------
CREATE OR REPLACE FUNCTION update_session_last_activity()
RETURNS TRIGGER AS $
BEGIN
    NEW.last_activity = NOW();
    RETURN NEW;
END;
$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_session_last_activity ON user_sessions;
CREATE TRIGGER trg_session_last_activity
    BEFORE UPDATE ON user_sessions
    FOR EACH ROW
    EXECUTE FUNCTION update_session_last_activity();
