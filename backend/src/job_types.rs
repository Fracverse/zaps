use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum JobType {
    Email,
    Notification,
    Sync,
    BlockchainTx,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobPayload {
    pub id: Uuid,
    pub job_type: JobType,
    pub payload: HashMap<String, serde_json::Value>,
    pub retries: Option<u32>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub scheduled_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl JobPayload {
    pub fn new(
        job_type: JobType,
        payload: HashMap<String, serde_json::Value>,
        retries: Option<u32>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            job_type,
            payload,
            retries,
            created_at: chrono::Utc::now(),
            scheduled_at: None,
        }
    }

    pub fn with_delay(
        job_type: JobType,
        payload: HashMap<String, serde_json::Value>,
        retries: Option<u32>,
        delay: chrono::Duration,
    ) -> Self {
        let mut job = Self::new(job_type, payload, retries);
        job.scheduled_at = Some(chrono::Utc::now() + delay);
        job
    }

    pub fn is_ready(&self) -> bool {
        if let Some(scheduled_at) = self.scheduled_at {
            chrono::Utc::now() >= scheduled_at
        } else {
            true
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobResult {
    pub job_id: Uuid,
    pub success: bool,
    pub error: Option<String>,
    pub processed_at: chrono::DateTime<chrono::Utc>,
    pub attempt: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeadLetterJob {
    pub original_job: JobPayload,
    pub error: String,
    pub failed_at: chrono::DateTime<chrono::Utc>,
    pub total_attempts: u32,
}
