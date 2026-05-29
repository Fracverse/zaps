use crate::{
    api_error::ApiError,
    config::Config,
    models::{AuditLogEntry, AuditLogQueryParams, CreateAuditLogParams},
};
use chrono::{Duration, Utc};
use deadpool_postgres::Pool;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone)]
#[allow(dead_code)]
pub struct AuditService {
    db_pool: Arc<Pool>,
    config: Config,
}

impl AuditService {
    pub fn new(db_pool: Arc<Pool>, config: Config) -> Self {
        Self { db_pool, config }
    }

    /// Log authentication attempt
    pub async fn log_auth_attempt(
        &self,
        user_id: &str,
        success: bool,
        ip_address: Option<String>,
        user_agent: Option<String>,
    ) -> Result<(), ApiError> {
        let action = if success {
            "auth_success"
        } else {
            "auth_failed"
        };

        let metadata = serde_json::json!({
            "success": success,
            "timestamp": Utc::now().to_rfc3339()
        });

        self.create_audit_log(CreateAuditLogParams {
            actor_id: user_id.to_string(),
            action: action.to_string(),
            resource: "authentication".to_string(),
            resource_id: Some(user_id.to_string()),
            metadata: Some(metadata),
            ip_address,
            user_agent,
        })
        .await?;

        Ok(())
    }

    /// Log payment operation
    pub async fn log_payment_operation(
        &self,
        actor_id: &str,
        action: &str,
        payment_id: &str,
        amount: i64,
        ip_address: Option<String>,
        user_agent: Option<String>,
    ) -> Result<(), ApiError> {
        let metadata = serde_json::json!({
            "amount": amount,
            "timestamp": Utc::now().to_rfc3339()
        });

        self.create_audit_log(CreateAuditLogParams {
            actor_id: actor_id.to_string(),
            action: action.to_string(),
            resource: "payment".to_string(),
            resource_id: Some(payment_id.to_string()),
            metadata: Some(metadata),
            ip_address,
            user_agent,
        })
        .await?;

        Ok(())
    }

    /// Log transfer operation
    pub async fn log_transfer_operation(
        &self,
        actor_id: &str,
        transfer_id: &str,
        from_user: &str,
        to_user: &str,
        amount: i64,
        ip_address: Option<String>,
        user_agent: Option<String>,
    ) -> Result<(), ApiError> {
        let metadata = serde_json::json!({
            "from_user": from_user,
            "to_user": to_user,
            "amount": amount,
            "timestamp": Utc::now().to_rfc3339()
        });

        self.create_audit_log(CreateAuditLogParams {
            actor_id: actor_id.to_string(),
            action: "transfer".to_string(),
            resource: "transfer".to_string(),
            resource_id: Some(transfer_id.to_string()),
            metadata: Some(metadata),
            ip_address,
            user_agent,
        })
        .await?;

        Ok(())
    }

    /// Log admin action
    pub async fn log_admin_action(
        &self,
        admin_id: &str,
        action: &str,
        resource: &str,
        resource_id: Option<String>,
        details: Option<serde_json::Value>,
        ip_address: Option<String>,
        user_agent: Option<String>,
    ) -> Result<(), ApiError> {
        self.create_audit_log(CreateAuditLogParams {
            actor_id: admin_id.to_string(),
            action: action.to_string(),
            resource: resource.to_string(),
            resource_id,
            metadata: details,
            ip_address,
            user_agent,
        })
        .await?;

        Ok(())
    }

    /// Create a new audit log entry (immutable)
    pub async fn create_audit_log(
        &self,
        params: CreateAuditLogParams,
    ) -> Result<AuditLogEntry, ApiError> {
        let client = self.db_pool.get().await?;

        let id = Uuid::new_v4().to_string();
        let timestamp = Utc::now();

        let row = client
            .query_one(
                "INSERT INTO audit_logs (id, actor_id, action, resource, resource_id, metadata, timestamp, ip_address, user_agent)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
                 RETURNING id, actor_id, action, resource, resource_id, metadata, timestamp, ip_address, user_agent",
                &[
                    &id,
                    &params.actor_id,
                    &params.action,
                    &params.resource,
                    &params.resource_id,
                    &params.metadata,
                    &timestamp,
                    &params.ip_address,
                    &params.user_agent,
                ],
            )
            .await?;

        Ok(AuditLogEntry {
            id: row.get("id"),
            actor_id: row.get("actor_id"),
            action: row.get("action"),
            resource: row.get("resource"),
            resource_id: row.get("resource_id"),
            metadata: row
                .try_get::<_, Option<serde_json::Value>>("metadata")
                .ok()
                .flatten(),
            timestamp: row.get("timestamp"),
            ip_address: row.get("ip_address"),
            user_agent: row.get("user_agent"),
        })
    }

    /// Get a single audit log entry by ID
    pub async fn get_audit_log(&self, id: &str) -> Result<AuditLogEntry, ApiError> {
        let client = self.db_pool.get().await?;

        let row = client
            .query_opt(
                "SELECT id, actor_id, action, resource, resource_id, metadata, timestamp, ip_address, user_agent
                 FROM audit_logs
                 WHERE id = $1",
                &[&id],
            )
            .await?
            .ok_or_else(|| ApiError::NotFound("Audit log not found".to_string()))?;

        Ok(AuditLogEntry {
            id: row.get("id"),
            actor_id: row.get("actor_id"),
            action: row.get("action"),
            resource: row.get("resource"),
            resource_id: row.get("resource_id"),
            metadata: row
                .try_get::<_, Option<serde_json::Value>>("metadata")
                .ok()
                .flatten(),
            timestamp: row.get("timestamp"),
            ip_address: row.get("ip_address"),
            user_agent: row.get("user_agent"),
        })
    }

    /// List audit logs with filtering
    pub async fn list_audit_logs(
        &self,
        params: &AuditLogQueryParams,
    ) -> Result<Vec<AuditLogEntry>, ApiError> {
        let client = self.db_pool.get().await?;

        // Build dynamic query based on filters
        let mut query = String::from(
            "SELECT id, actor_id, action, resource, resource_id, metadata, timestamp, ip_address, user_agent
             FROM audit_logs
             WHERE 1=1",
        );
        let mut param_index = 1;
        let mut params_vec: Vec<Box<dyn tokio_postgres::types::ToSql + Sync + Send>> = Vec::new();

        if let Some(ref actor_id) = params.actor_id {
            query.push_str(&format!(" AND actor_id = ${}", param_index));
            params_vec.push(Box::new(actor_id.clone()));
            param_index += 1;
        }

        if let Some(ref action) = params.action {
            query.push_str(&format!(" AND action = ${}", param_index));
            params_vec.push(Box::new(action.clone()));
            param_index += 1;
        }

        if let Some(ref from_date) = params.from_date {
            query.push_str(&format!(" AND timestamp >= ${}", param_index));
            params_vec.push(Box::new(*from_date));
            param_index += 1;
        }

        if let Some(ref to_date) = params.to_date {
            query.push_str(&format!(" AND timestamp <= ${}", param_index));
            params_vec.push(Box::new(*to_date));
            param_index += 1;
        }

        query.push_str(" ORDER BY timestamp DESC");

        // Sanitize limit and offset
        let limit = params.limit.clamp(1, 100);
        let offset = params.offset.max(0);

        query.push_str(&format!(
            " LIMIT ${} OFFSET ${}",
            param_index,
            param_index + 1
        ));
        params_vec.push(Box::new(limit));
        params_vec.push(Box::new(offset));

        let param_refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = params_vec
            .iter()
            .map(|p| p.as_ref() as &(dyn tokio_postgres::types::ToSql + Sync))
            .collect();

        let rows = client.query(&query, &param_refs[..]).await?;

        let logs = rows
            .into_iter()
            .map(|row| AuditLogEntry {
                id: row.get("id"),
                actor_id: row.get("actor_id"),
                action: row.get("action"),
                resource: row.get("resource"),
                resource_id: row.get("resource_id"),
                metadata: row
                    .try_get::<_, Option<serde_json::Value>>("metadata")
                    .ok()
                    .flatten(),
                timestamp: row.get("timestamp"),
                ip_address: row.get("ip_address"),
                user_agent: row.get("user_agent"),
            })
            .collect();

        Ok(logs)
    }

    /// Count audit logs for pagination
    pub async fn count_audit_logs(&self, params: &AuditLogQueryParams) -> Result<i64, ApiError> {
        let client = self.db_pool.get().await?;

        let mut query = String::from("SELECT COUNT(*) FROM audit_logs WHERE 1=1");
        let mut param_index = 1;
        let mut params_vec: Vec<Box<dyn tokio_postgres::types::ToSql + Sync + Send>> = Vec::new();

        if let Some(ref actor_id) = params.actor_id {
            query.push_str(&format!(" AND actor_id = ${}", param_index));
            params_vec.push(Box::new(actor_id.clone()));
            param_index += 1;
        }

        if let Some(ref action) = params.action {
            query.push_str(&format!(" AND action = ${}", param_index));
            params_vec.push(Box::new(action.clone()));
            param_index += 1;
        }

        if let Some(ref from_date) = params.from_date {
            query.push_str(&format!(" AND timestamp >= ${}", param_index));
            params_vec.push(Box::new(*from_date));
            param_index += 1;
        }

        if let Some(ref to_date) = params.to_date {
            query.push_str(&format!(" AND timestamp <= ${}", param_index));
            params_vec.push(Box::new(*to_date));
        }

        let param_refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = params_vec
            .iter()
            .map(|p| p.as_ref() as &(dyn tokio_postgres::types::ToSql + Sync))
            .collect();

        let row = client.query_one(&query, &param_refs[..]).await?;
        let count: i64 = row.get(0);

        Ok(count)
    }
}

    /// Archive old audit logs (older than retention period)
    pub async fn archive_old_logs(&self, retention_days: i64) -> Result<u64, ApiError> {
        let client = self.db_pool.get().await?;

        let cutoff_date = Utc::now() - Duration::days(retention_days);

        let result = client
            .execute(
                r#"
                INSERT INTO audit_logs_archive
                SELECT * FROM audit_logs WHERE timestamp < $1;
                DELETE FROM audit_logs WHERE timestamp < $1;
                "#,
                &[&cutoff_date],
            )
            .await?;

        Ok(result as u64)
    }

    /// Get audit log statistics
    pub async fn get_audit_statistics(
        &self,
        days: i64,
    ) -> Result<serde_json::Value, ApiError> {
        let client = self.db_pool.get().await?;

        let cutoff_date = Utc::now() - Duration::days(days);

        let row = client
            .query_one(
                r#"
                SELECT
                    COUNT(*) as total_logs,
                    COUNT(DISTINCT actor_id) as unique_actors,
                    COUNT(DISTINCT action) as unique_actions,
                    COUNT(DISTINCT resource) as unique_resources
                FROM audit_logs
                WHERE timestamp >= $1
                "#,
                &[&cutoff_date],
            )
            .await?;

        let stats = serde_json::json!({
            "total_logs": row.get::<_, i64>(0),
            "unique_actors": row.get::<_, i64>(1),
            "unique_actions": row.get::<_, i64>(2),
            "unique_resources": row.get::<_, i64>(3),
            "period_days": days
        });

        Ok(stats)
    }

    /// Get audit logs by action type
    pub async fn get_logs_by_action(
        &self,
        action: &str,
        limit: i64,
    ) -> Result<Vec<AuditLogEntry>, ApiError> {
        let client = self.db_pool.get().await?;

        let rows = client
            .query(
                r#"
                SELECT id, actor_id, action, resource, resource_id, metadata,
                       timestamp, ip_address, user_agent
                FROM audit_logs
                WHERE action = $1
                ORDER BY timestamp DESC
                LIMIT $2
                "#,
                &[&action, &limit],
            )
            .await?;

        Ok(rows
            .into_iter()
            .map(|row| AuditLogEntry {
                id: row.get("id"),
                actor_id: row.get("actor_id"),
                action: row.get("action"),
                resource: row.get("resource"),
                resource_id: row.get("resource_id"),
                metadata: row
                    .try_get::<_, Option<serde_json::Value>>("metadata")
                    .ok()
                    .flatten(),
                timestamp: row.get("timestamp"),
                ip_address: row.get("ip_address"),
                user_agent: row.get("user_agent"),
            })
            .collect())
    }

    /// Get suspicious activity logs
    pub async fn get_suspicious_activity(
        &self,
        hours: i64,
    ) -> Result<Vec<AuditLogEntry>, ApiError> {
        let client = self.db_pool.get().await?;

        let cutoff_time = Utc::now() - Duration::hours(hours);

        let rows = client
            .query(
                r#"
                SELECT id, actor_id, action, resource, resource_id, metadata,
                       timestamp, ip_address, user_agent
                FROM audit_logs
                WHERE action IN ('auth_failed', 'suspicious_activity')
                  AND timestamp >= $1
                ORDER BY timestamp DESC
                "#,
                &[&cutoff_time],
            )
            .await?;

        Ok(rows
            .into_iter()
            .map(|row| AuditLogEntry {
                id: row.get("id"),
                actor_id: row.get("actor_id"),
                action: row.get("action"),
                resource: row.get("resource"),
                resource_id: row.get("resource_id"),
                metadata: row
                    .try_get::<_, Option<serde_json::Value>>("metadata")
                    .ok()
                    .flatten(),
                timestamp: row.get("timestamp"),
                ip_address: row.get("ip_address"),
                user_agent: row.get("user_agent"),
            })
            .collect())
    }
}