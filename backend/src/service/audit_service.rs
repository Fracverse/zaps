use crate::{api_error::ApiError, config::Config, models::{AuditLogEntry, AuditLogQueryParams}};
use chrono::Utc;
use deadpool_postgres::Pool;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone)]
pub struct AuditService {
    db_pool: Arc<Pool>,
    config: Config,
}

impl AuditService {
    pub fn new(db_pool: Arc<Pool>, config: Config) -> Self {
        Self { db_pool, config }
    }

    /// Create a new audit log entry (immutable)
    pub async fn create_audit_log(
        &self,
        actor_id: String,
        action: String,
        resource: String,
        resource_id: Option<String>,
        metadata: Option<serde_json::Value>,
        ip_address: Option<String>,
        user_agent: Option<String>,
    ) -> Result<AuditLogEntry, ApiError> {
        let client = self.db_pool.get().await?;

        let id = Uuid::new_v4().to_string();
        let timestamp = Utc::now();

        let row = client
            .query_one(
                "INSERT INTO audit_logs (id, actor_id, action, resource, resource_id, metadata, timestamp, ip_address, user_agent)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
                 RETURNING id, actor_id, action, resource, resource_id, metadata, timestamp, ip_address, user_agent",
                &[&id, &actor_id, &action, &resource, &resource_id, &metadata, &timestamp, &ip_address, &user_agent],
            )
            .await?;

        Ok(AuditLogEntry {
            id: row.get("id"),
            actor_id: row.get("actor_id"),
            action: row.get("action"),
            resource: row.get("resource"),
            resource_id: row.get("resource_id"),
            metadata: row.try_get::<_, Option<serde_json::Value>>("metadata").ok().flatten(),
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
            metadata: row.try_get::<_, Option<serde_json::Value>>("metadata").ok().flatten(),
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
        let limit = params.limit.min(100).max(1);
        let offset = params.offset.max(0);
        
        query.push_str(&format!(" LIMIT ${} OFFSET ${}", param_index, param_index + 1));
        params_vec.push(Box::new(limit));
        params_vec.push(Box::new(offset));

        let param_refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> =
            params_vec.iter().map(|p| p.as_ref() as &(dyn tokio_postgres::types::ToSql + Sync)).collect();

        let rows = client.query(&query, &param_refs[..]).await?;

        let logs = rows
            .into_iter()
            .map(|row| AuditLogEntry {
                id: row.get("id"),
                actor_id: row.get("actor_id"),
                action: row.get("action"),
                resource: row.get("resource"),
                resource_id: row.get("resource_id"),
                metadata: row.try_get::<_, Option<serde_json::Value>>("metadata").ok().flatten(),
                timestamp: row.get("timestamp"),
                ip_address: row.get("ip_address"),
                user_agent: row.get("user_agent"),
            })
            .collect();

        Ok(logs)
    }

    /// Count audit logs for pagination
    pub async fn count_audit_logs(
        &self,
        params: &AuditLogQueryParams,
    ) -> Result<i64, ApiError> {
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
            param_index += 1;
        }

        let param_refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> =
            params_vec.iter().map(|p| p.as_ref() as &(dyn tokio_postgres::types::ToSql + Sync)).collect();

        let row = client.query_one(&query, &param_refs[..]).await?;
        let count: i64 = row.get(0);

        Ok(count)
    }
}
