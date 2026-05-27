use crate::api_error::ApiError;
use crate::config::Config;
use chrono::{DateTime, Duration, Utc};
use deadpool_postgres::Pool;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tracing::{info, warn};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SessionStatus {
    Active,
    Suspended,
    Revoked,
    Expired,
}

impl std::fmt::Display for SessionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SessionStatus::Active => write!(f, "active"),
            SessionStatus::Suspended => write!(f, "suspended"),
            SessionStatus::Revoked => write!(f, "revoked"),
            SessionStatus::Expired => write!(f, "expired"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SecurityEventType {
    Login,
    Logout,
    FailedLogin,
    SuspiciousActivity,
    DeviceChange,
    LocationChange,
    ConcurrentSession,
}

impl std::fmt::Display for SecurityEventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SecurityEventType::Login => write!(f, "login"),
            SecurityEventType::Logout => write!(f, "logout"),
            SecurityEventType::FailedLogin => write!(f, "failed_login"),
            SecurityEventType::SuspiciousActivity => write!(f, "suspicious_activity"),
            SecurityEventType::DeviceChange => write!(f, "device_change"),
            SecurityEventType::LocationChange => write!(f, "location_change"),
            SecurityEventType::ConcurrentSession => write!(f, "concurrent_session"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EventSeverity {
    Low,
    Medium,
    High,
    Critical,
}

impl std::fmt::Display for EventSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EventSeverity::Low => write!(f, "low"),
            EventSeverity::Medium => write!(f, "medium"),
            EventSeverity::High => write!(f, "high"),
            EventSeverity::Critical => write!(f, "critical"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserSession {
    pub id: String,
    pub user_id: String,
    pub session_token: String,
    pub device_id: String,
    pub device_fingerprint: String,
    pub ip_address: String,
    pub user_agent: String,
    pub status: String,
    pub last_activity: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionActivityLog {
    pub id: String,
    pub session_id: String,
    pub user_id: String,
    pub activity_type: String,
    pub endpoint: Option<String>,
    pub method: Option<String>,
    pub status_code: Option<i32>,
    pub ip_address: String,
    pub user_agent: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityEvent {
    pub id: String,
    pub user_id: String,
    pub session_id: Option<String>,
    pub event_type: String,
    pub severity: String,
    pub description: String,
    pub ip_address: Option<String>,
    pub device_id: Option<String>,
    pub action_taken: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone)]
pub struct SessionService {
    db_pool: Arc<Pool>,
    config: Config,
}

impl SessionService {
    pub fn new(db_pool: Arc<Pool>, config: Config) -> Self {
        Self { db_pool, config }
    }

    /// Create a new user session
    pub async fn create_session(
        &self,
        user_id: &str,
        device_id: &str,
        device_fingerprint: &str,
        ip_address: &str,
        user_agent: &str,
        session_duration_hours: i64,
    ) -> Result<UserSession, ApiError> {
        let client = self.db_pool.get().await?;
        let session_id = Uuid::new_v4().to_string();
        let session_token = self.generate_session_token();
        let now = Utc::now();
        let expires_at = now + Duration::hours(session_duration_hours);

        let row = client
            .query_one(
                "INSERT INTO user_sessions (id, user_id, session_token, device_id, device_fingerprint, ip_address, user_agent, status, expires_at)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
                 RETURNING id, user_id, session_token, device_id, device_fingerprint, ip_address, user_agent, status, last_activity, created_at, expires_at",
                &[
                    &session_id,
                    &user_id,
                    &session_token,
                    &device_id,
                    &device_fingerprint,
                    &ip_address,
                    &user_agent,
                    &"active",
                    &expires_at,
                ],
            )
            .await?;

        info!(user_id = %user_id, session_id = %session_id, "Session created");

        Ok(UserSession {
            id: row.get("id"),
            user_id: row.get("user_id"),
            session_token: row.get("session_token"),
            device_id: row.get("device_id"),
            device_fingerprint: row.get("device_fingerprint"),
            ip_address: row.get("ip_address"),
            user_agent: row.get("user_agent"),
            status: row.get("status"),
            last_activity: row.get("last_activity"),
            created_at: row.get("created_at"),
            expires_at: row.get("expires_at"),
        })
    }

    /// Get session by token
    pub async fn get_session_by_token(&self, token: &str) -> Result<Option<UserSession>, ApiError> {
        let client = self.db_pool.get().await?;

        let row = client
            .query_opt(
                "SELECT id, user_id, session_token, device_id, device_fingerprint, ip_address, user_agent, status, last_activity, created_at, expires_at
                 FROM user_sessions WHERE session_token = $1",
                &[&token],
            )
            .await?;

        Ok(row.map(|r| UserSession {
            id: r.get("id"),
            user_id: r.get("user_id"),
            session_token: r.get("session_token"),
            device_id: r.get("device_id"),
            device_fingerprint: r.get("device_fingerprint"),
            ip_address: r.get("ip_address"),
            user_agent: r.get("user_agent"),
            status: r.get("status"),
            last_activity: r.get("last_activity"),
            created_at: r.get("created_at"),
            expires_at: r.get("expires_at"),
        }))
    }

    /// Get active sessions for user
    pub async fn get_active_sessions(&self, user_id: &str) -> Result<Vec<UserSession>, ApiError> {
        let client = self.db_pool.get().await?;

        let rows = client
            .query(
                "SELECT id, user_id, session_token, device_id, device_fingerprint, ip_address, user_agent, status, last_activity, created_at, expires_at
                 FROM user_sessions WHERE user_id = $1 AND status = 'active' AND expires_at > NOW()
                 ORDER BY last_activity DESC",
                &[&user_id],
            )
            .await?;

        Ok(rows
            .iter()
            .map(|r| UserSession {
                id: r.get("id"),
                user_id: r.get("user_id"),
                session_token: r.get("session_token"),
                device_id: r.get("device_id"),
                device_fingerprint: r.get("device_fingerprint"),
                ip_address: r.get("ip_address"),
                user_agent: r.get("user_agent"),
                status: r.get("status"),
                last_activity: r.get("last_activity"),
                created_at: r.get("created_at"),
                expires_at: r.get("expires_at"),
            })
            .collect())
    }

    /// Revoke a session
    pub async fn revoke_session(&self, session_id: &str) -> Result<(), ApiError> {
        let client = self.db_pool.get().await?;

        client
            .execute(
                "UPDATE user_sessions SET status = 'revoked' WHERE id = $1",
                &[&session_id],
            )
            .await?;

        info!(session_id = %session_id, "Session revoked");

        Ok(())
    }

    /// Suspend a session (due to suspicious activity)
    pub async fn suspend_session(&self, session_id: &str) -> Result<(), ApiError> {
        let client = self.db_pool.get().await?;

        client
            .execute(
                "UPDATE user_sessions SET status = 'suspended' WHERE id = $1",
                &[&session_id],
            )
            .await?;

        info!(session_id = %session_id, "Session suspended");

        Ok(())
    }

    /// Log session activity
    pub async fn log_activity(
        &self,
        session_id: &str,
        user_id: &str,
        activity_type: &str,
        endpoint: Option<&str>,
        method: Option<&str>,
        status_code: Option<i32>,
        ip_address: &str,
        user_agent: &str,
    ) -> Result<(), ApiError> {
        let client = self.db_pool.get().await?;
        let log_id = Uuid::new_v4().to_string();

        client
            .execute(
                "INSERT INTO session_activity_log (id, session_id, user_id, activity_type, endpoint, method, status_code, ip_address, user_agent)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
                &[
                    &log_id,
                    &session_id,
                    &user_id,
                    &activity_type,
                    &endpoint.unwrap_or(""),
                    &method.unwrap_or(""),
                    &status_code,
                    &ip_address,
                    &user_agent,
                ],
            )
            .await?;

        Ok(())
    }

    /// Record a security event
    pub async fn record_security_event(
        &self,
        user_id: &str,
        session_id: Option<&str>,
        event_type: &str,
        severity: &str,
        description: &str,
        ip_address: Option<&str>,
        device_id: Option<&str>,
        action_taken: Option<&str>,
    ) -> Result<SecurityEvent, ApiError> {
        let client = self.db_pool.get().await?;
        let event_id = Uuid::new_v4().to_string();

        let row = client
            .query_one(
                "INSERT INTO session_security_events (id, user_id, session_id, event_type, severity, description, ip_address, device_id, action_taken)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
                 RETURNING id, user_id, session_id, event_type, severity, description, ip_address, device_id, action_taken, created_at",
                &[
                    &event_id,
                    &user_id,
                    &session_id.unwrap_or(""),
                    &event_type,
                    &severity,
                    &description,
                    &ip_address.unwrap_or(""),
                    &device_id.unwrap_or(""),
                    &action_taken.unwrap_or(""),
                ],
            )
            .await?;

        warn!(
            user_id = %user_id,
            event_type = %event_type,
            severity = %severity,
            "Security event recorded"
        );

        Ok(SecurityEvent {
            id: row.get("id"),
            user_id: row.get("user_id"),
            session_id: row.get("session_id"),
            event_type: row.get("event_type"),
            severity: row.get("severity"),
            description: row.get("description"),
            ip_address: row.get("ip_address"),
            device_id: row.get("device_id"),
            action_taken: row.get("action_taken"),
            created_at: row.get("created_at"),
        })
    }

    /// Check for concurrent sessions and enforce limit
    pub async fn enforce_concurrent_session_limit(
        &self,
        user_id: &str,
        max_concurrent: usize,
    ) -> Result<(), ApiError> {
        let active_sessions = self.get_active_sessions(user_id).await?;

        if active_sessions.len() >= max_concurrent {
            // Revoke oldest session
            if let Some(oldest) = active_sessions.last() {
                self.revoke_session(&oldest.id).await?;
                self.record_security_event(
                    user_id,
                    Some(&oldest.id),
                    "concurrent_session",
                    "medium",
                    "Oldest session revoked due to concurrent session limit",
                    Some(&oldest.ip_address),
                    Some(&oldest.device_id),
                    Some("revoked_oldest_session"),
                )
                .await?;
            }
        }

        Ok(())
    }

    /// Detect suspicious activity (device change, location change, etc.)
    pub async fn detect_suspicious_activity(
        &self,
        user_id: &str,
        device_fingerprint: &str,
        ip_address: &str,
    ) -> Result<bool, ApiError> {
        let active_sessions = self.get_active_sessions(user_id).await?;

        for session in active_sessions {
            if session.device_fingerprint != device_fingerprint {
                self.record_security_event(
                    user_id,
                    Some(&session.id),
                    "device_change",
                    "high",
                    "Device fingerprint mismatch detected",
                    Some(ip_address),
                    Some(&device_fingerprint),
                    Some("flagged_for_review"),
                )
                .await?;
                return Ok(true);
            }

            if session.ip_address != ip_address {
                self.record_security_event(
                    user_id,
                    Some(&session.id),
                    "location_change",
                    "medium",
                    "IP address change detected",
                    Some(ip_address),
                    None,
                    Some("flagged_for_review"),
                )
                .await?;
                return Ok(true);
            }
        }

        Ok(false)
    }

    /// Clean up expired sessions
    pub async fn cleanup_expired_sessions(&self) -> Result<u64, ApiError> {
        let client = self.db_pool.get().await?;

        let result = client
            .execute(
                "UPDATE user_sessions SET status = 'expired' WHERE expires_at <= NOW() AND status = 'active'",
                &[],
            )
            .await?;

        info!(expired_count = %result, "Expired sessions cleaned up");

        Ok(result)
    }

    /// Generate a secure session token
    fn generate_session_token(&self) -> String {
        let random_bytes = Uuid::new_v4().as_bytes().to_vec();
        let mut hasher = Sha256::new();
        hasher.update(&random_bytes);
        let result = hasher.finalize();
        format!("{:x}", result)
    }
}
