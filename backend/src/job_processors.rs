use crate::job_types::{JobPayload, JobResult, JobType};
use crate::queue::JobProcessor;
use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde_json::Value;
use std::collections::HashMap;
use tracing::{debug, error, info};

pub struct EmailProcessor {
    http_client: Client,
}

impl EmailProcessor {
    pub fn new() -> Self {
        Self {
            http_client: Client::new(),
        }
    }

    async fn send_email(&self, to: &str, subject: &str, body: &str) -> Result<()> {
        debug!("Sending email to: {} | Subject: {}", to, subject);
        
        // Simulate email sending
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        
        // Example: SendGrid API call
        /*
        let response = self.http_client
            .post("https://api.sendgrid.com/v3/mail/send")
            .header("Authorization", "Bearer YOUR_SENDGRID_API_KEY")
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "personalizations": [{
                    "to": [{"email": to}],
                    "subject": subject
                }],
                "from": {"email": "noreply@zaps.com"},
                "content": [{
                    "type": "text/plain",
                    "value": body
                }]
            }))
            .send()
            .await
            .context("Failed to send email via SendGrid")?;

        if !response.status().is_success() {
            let error_text = response.text().await
                .context("Failed to read error response")?;
            anyhow::bail!("Email service returned error: {}", error_text);
        }
        */

        info!("Email sent successfully to: {}", to);
        Ok(())
    }
}

#[async_trait]
impl JobProcessor for EmailProcessor {
    async fn process(&self, job: &JobPayload) -> Result<JobResult> {
        let to = job.payload.get("to")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'to' field in email job payload"))?;
        
        let subject = job.payload.get("subject")
            .and_then(|v| v.as_str())
            .unwrap_or("No Subject");
        
        let body = job.payload.get("body")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        match self.send_email(to, subject, body).await {
            Ok(_) => {
                info!("Email job {} completed successfully", job.id);
                Ok(JobResult {
                    job_id: job.id,
                    success: true,
                    error: None,
                    processed_at: chrono::Utc::now(),
                    attempt: job.retries.unwrap_or(0) + 1,
                })
            }
            Err(e) => {
                error!("Email job {} failed: {}", job.id, e);
                Ok(JobResult {
                    job_id: job.id,
                    success: false,
                    error: Some(e.to_string()),
                    processed_at: chrono::Utc::now(),
                    attempt: job.retries.unwrap_or(0) + 1,
                })
            }
        }
    }
}

pub struct NotificationProcessor {
    http_client: Client,
}

impl NotificationProcessor {
    pub fn new() -> Self {
        Self {
            http_client: Client::new(),
        }
    }

    async fn send_notification(&self, user_id: &str, message: &str, notification_type: &str) -> Result<()> {
        debug!("Sending notification to user: {} | Type: {}", user_id, notification_type);
        
        // Simulate notification sending
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        info!("Notification sent successfully to user: {}", user_id);
        Ok(())
    }
}

#[async_trait]
impl JobProcessor for NotificationProcessor {
    async fn process(&self, job: &JobPayload) -> Result<JobResult> {
        let user_id = job.payload.get("user_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'user_id' field in notification job payload"))?;
        
        let message = job.payload.get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        
        let notification_type = job.payload.get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("info");

        match self.send_notification(user_id, message, notification_type).await {
            Ok(_) => {
                info!("Notification job {} completed successfully", job.id);
                Ok(JobResult {
                    job_id: job.id,
                    success: true,
                    error: None,
                    processed_at: chrono::Utc::now(),
                    attempt: job.retries.unwrap_or(0) + 1,
                })
            }
            Err(e) => {
                error!("Notification job {} failed: {}", job.id, e);
                Ok(JobResult {
                    job_id: job.id,
                    success: false,
                    error: Some(e.to_string()),
                    processed_at: chrono::Utc::now(),
                    attempt: job.retries.unwrap_or(0) + 1,
                })
            }
        }
    }
}

pub struct SyncProcessor {
    http_client: Client,
}

impl SyncProcessor {
    pub fn new() -> Self {
        Self {
            http_client: Client::new(),
        }
    }

    async fn perform_sync(&self, sync_type: &str, data: &HashMap<String, Value>) -> Result<()> {
        debug!("Performing sync operation: {}", sync_type);
        
        // Simulate sync operation
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

        match sync_type {
            "user_data" => {
                let user_id = data.get("user_id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("Missing user_id in sync data"))?;
                
                debug!("Syncing user data for: {}", user_id);
            }
            "analytics" => {
                debug!("Syncing analytics data");
            }
            "backup" => {
                debug!("Performing data backup");
            }
            _ => {
                anyhow::bail!("Unknown sync type: {}", sync_type);
            }
        }

        info!("Sync operation {} completed successfully", sync_type);
        Ok(())
    }
}

#[async_trait]
impl JobProcessor for SyncProcessor {
    async fn process(&self, job: &JobPayload) -> Result<JobResult> {
        let sync_type = job.payload.get("sync_type")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'sync_type' field in sync job payload"))?;

        let mut data = HashMap::new();
        for (key, value) in &job.payload {
            if key != "sync_type" {
                data.insert(key.clone(), value.clone());
            }
        }

        match self.perform_sync(sync_type, &data).await {
            Ok(_) => {
                info!("Sync job {} completed successfully", job.id);
                Ok(JobResult {
                    job_id: job.id,
                    success: true,
                    error: None,
                    processed_at: chrono::Utc::now(),
                    attempt: job.retries.unwrap_or(0) + 1,
                })
            }
            Err(e) => {
                error!("Sync job {} failed: {}", job.id, e);
                Ok(JobResult {
                    job_id: job.id,
                    success: false,
                    error: Some(e.to_string()),
                    processed_at: chrono::Utc::now(),
                    attempt: job.retries.unwrap_or(0) + 1,
                })
            }
        }
    }
}

pub struct BlockchainTxProcessor {
    http_client: Client,
}

impl BlockchainTxProcessor {
    pub fn new() -> Self {
        Self {
            http_client: Client::new(),
        }
    }

    async fn process_transaction(&self, tx_data: &HashMap<String, Value>) -> Result<String> {
        let network = tx_data.get("network")
            .and_then(|v| v.as_str())
            .unwrap_or("stellar");

        let from_address = tx_data.get("from_address")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'from_address' in transaction data"))?;

        let to_address = tx_data.get("to_address")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'to_address' in transaction data"))?;

        let amount = tx_data.get("amount")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'amount' in transaction data"))?;

        debug!("Processing {} transaction from {} to {} for amount {}", 
               network, from_address, to_address, amount);

        // Simulate blockchain transaction
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        // For demo purposes, return a mock transaction hash
        let tx_hash = format!("tx_{}", uuid::Uuid::new_v4().to_string().replace("-", ""));
        
        info!("Blockchain transaction processed successfully. Hash: {}", tx_hash);
        Ok(tx_hash)
    }
}

#[async_trait]
impl JobProcessor for BlockchainTxProcessor {
    async fn process(&self, job: &JobPayload) -> Result<JobResult> {
        let mut tx_data = HashMap::new();
        for (key, value) in &job.payload {
            tx_data.insert(key.clone(), value.clone());
        }

        match self.process_transaction(&tx_data).await {
            Ok(tx_hash) => {
                info!("Blockchain transaction job {} completed successfully. Hash: {}", job.id, tx_hash);
                Ok(JobResult {
                    job_id: job.id,
                    success: true,
                    error: None,
                    processed_at: chrono::Utc::now(),
                    attempt: job.retries.unwrap_or(0) + 1,
                })
            }
            Err(e) => {
                error!("Blockchain transaction job {} failed: {}", job.id, e);
                Ok(JobResult {
                    job_id: job.id,
                    success: false,
                    error: Some(e.to_string()),
                    processed_at: chrono::Utc::now(),
                    attempt: job.retries.unwrap_or(0) + 1,
                })
            }
        }
    }
}

pub struct JobProcessorRegistry {
    processors: HashMap<JobType, Box<dyn JobProcessor>>,
}

impl JobProcessorRegistry {
    pub fn new() -> Self {
        let mut processors: HashMap<JobType, Box<dyn JobProcessor>> = HashMap::new();
        
        processors.insert(JobType::Email, Box::new(EmailProcessor::new()));
        processors.insert(JobType::Notification, Box::new(NotificationProcessor::new()));
        processors.insert(JobType::Sync, Box::new(SyncProcessor::new()));
        processors.insert(JobType::BlockchainTx, Box::new(BlockchainTxProcessor::new()));

        Self { processors }
    }

    pub fn get_processor(&self, job_type: &JobType) -> Option<&dyn JobProcessor> {
        self.processors.get(job_type).map(|p| p.as_ref())
    }
}

impl Default for JobProcessorRegistry {
    fn default() -> Self {
        Self::new()
    }
}
