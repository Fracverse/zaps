use crate::config::Config;
use crate::job_processors::JobProcessorRegistry;
use crate::job_types::{JobPayload, JobType};
use crate::queue::{JobQueue, QueueConfig};
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::interval;
use tracing::{debug, error, info, warn};

pub struct JobWorker {
    queue: Arc<JobQueue>,
    processor_registry: Arc<JobProcessorRegistry>,
    config: Config,
}

impl JobWorker {
    pub async fn new(config: Config) -> Result<Self> {
        let queue_config = QueueConfig {
            max_retries: config.queue_config.max_retries,
            visibility_timeout: Duration::from_secs(config.queue_config.visibility_timeout_seconds),
            backoff_multiplier: config.queue_config.backoff_multiplier,
            max_backoff: Duration::from_secs(config.queue_config.max_backoff_seconds),
            dead_letter_max_size: config.queue_config.dead_letter_max_size,
        };

        let queue = Arc::new(
            JobQueue::new(&config.queue_config.redis_url, queue_config)
                .await
                .context("Failed to create job queue")?,
        );

        let processor_registry = Arc::new(JobProcessorRegistry::new());

        Ok(Self {
            queue,
            processor_registry,
            config,
        })
    }

    pub async fn start_workers(&self) -> Result<()> {
        info!(
            "Starting {} job workers",
            self.config.queue_config.worker_count
        );

        let mut handles = vec![];

        // Spawn worker tasks
        for i in 0..self.config.queue_config.worker_count {
            let queue = Arc::clone(&self.queue);
            let processor_registry = Arc::clone(&self.processor_registry);

            let handle = tokio::spawn(async move {
                let worker_id = i + 1;
                info!("Job worker {} started", worker_id);

                loop {
                    match Self::process_next_job(&queue, &processor_registry).await {
                        Ok(Some(())) => {
                            // Successfully processed a job
                        }
                        Ok(None) => {
                            // No jobs available, wait a bit
                            tokio::time::sleep(Duration::from_millis(100)).await;
                        }
                        Err(e) => {
                            error!("Worker {} encountered error: {}", worker_id, e);
                            tokio::time::sleep(Duration::from_secs(1)).await;
                        }
                    }
                }
            });

            handles.push(handle);
        }

        // Spawn retry queue processor
        let retry_queue = Arc::clone(&self.queue);
        let retry_handle = tokio::spawn(async move {
            info!("Retry queue processor started");
            let mut interval = interval(Duration::from_secs(30)); // Check every 30 seconds

            loop {
                interval.tick().await;
                if let Err(e) = retry_queue.process_retry_queue().await {
                    error!("Failed to process retry queue: {}", e);
                }
            }
        });
        handles.push(retry_handle);

        // Spawn stalled job reclaimer
        let reclaim_queue = Arc::clone(&self.queue);
        let reclaim_interval =
            Duration::from_secs(self.config.queue_config.reclaim_interval_seconds);
        let reclaim_handle = tokio::spawn(async move {
            info!("Stalled job reclaimer started");
            let mut interval = interval(reclaim_interval);

            loop {
                interval.tick().await;
                match reclaim_queue.reclaim_stalled_jobs().await {
                    Ok(count) if count > 0 => {
                        info!("Reclaimed {} stalled jobs", count);
                    }
                    Err(e) => {
                        error!("Failed to reclaim stalled jobs: {}", e);
                    }
                    _ => {}
                }
            }
        });
        handles.push(reclaim_handle);

        // Wait for all workers (they run indefinitely)
        tokio::select! {
            _ = futures::future::join_all(handles) => {
                warn!("All job workers have stopped");
            }
        }

        Ok(())
    }

    async fn process_next_job(
        queue: &JobQueue,
        processor_registry: &JobProcessorRegistry,
    ) -> Result<Option<()>> {
        let job = match queue.dequeue().await? {
            Some(job) => job,
            None => return Ok(None),
        };

        debug!("Processing job {} of type {:?}", job.id, job.job_type);

        let processor = match processor_registry.get_processor(&job.job_type) {
            Some(processor) => processor,
            None => {
                let error = format!("No processor found for job type: {:?}", job.job_type);
                error!("{}", error);
                queue.retry_job(job, error).await?;
                return Ok(Some(()));
            }
        };

        match processor.process(&job).await {
            Ok(result) => {
                if result.success {
                    queue.complete_job(job.id, result).await?;
                    info!("Job {} completed successfully", job.id);
                } else {
                    let error = result.error.unwrap_or_else(|| "Unknown error".to_string());
                    warn!("Job {} failed, retrying: {}", job.id, error);
                    queue.retry_job(job, error).await?;
                }
            }
            Err(e) => {
                error!("Job {} processing error: {}", job.id, e);
                queue.retry_job(job, e.to_string()).await?;
            }
        }

        Ok(Some(()))
    }

    pub async fn enqueue_job(
        &self,
        job_type: JobType,
        payload: HashMap<String, serde_json::Value>,
    ) -> Result<()> {
        let job = JobPayload::new(job_type, payload, None);
        self.queue.enqueue(job).await?;
        Ok(())
    }

    pub async fn enqueue_delayed_job(
        &self,
        job_type: JobType,
        payload: HashMap<String, serde_json::Value>,
        delay: chrono::Duration,
    ) -> Result<()> {
        let job = JobPayload::with_delay(job_type, payload, None, delay);
        self.queue.enqueue(job).await?;
        Ok(())
    }

    pub async fn get_queue_stats(&self) -> Result<crate::queue::QueueStats> {
        self.queue.get_queue_stats().await
    }
}

// HTTP API endpoints for job management
pub async fn enqueue_email_job(
    worker: Arc<JobWorker>,
    to: String,
    subject: String,
    body: String,
) -> Result<()> {
    let mut payload = HashMap::new();
    payload.insert("to".to_string(), serde_json::Value::String(to));
    payload.insert("subject".to_string(), serde_json::Value::String(subject));
    payload.insert("body".to_string(), serde_json::Value::String(body));

    worker.enqueue_job(JobType::Email, payload).await
}

pub async fn enqueue_notification_job(
    worker: Arc<JobWorker>,
    user_id: String,
    message: String,
    notification_type: String,
) -> Result<()> {
    let mut payload = HashMap::new();
    payload.insert("user_id".to_string(), serde_json::Value::String(user_id));
    payload.insert("message".to_string(), serde_json::Value::String(message));
    payload.insert(
        "type".to_string(),
        serde_json::Value::String(notification_type),
    );

    worker.enqueue_job(JobType::Notification, payload).await
}

pub async fn enqueue_sync_job(
    worker: Arc<JobWorker>,
    sync_type: String,
    data: HashMap<String, serde_json::Value>,
) -> Result<()> {
    let mut payload = HashMap::new();
    payload.insert(
        "sync_type".to_string(),
        serde_json::Value::String(sync_type),
    );
    payload.extend(data);

    worker.enqueue_job(JobType::Sync, payload).await
}

pub async fn enqueue_blockchain_tx_job(
    worker: Arc<JobWorker>,
    from_address: String,
    to_address: String,
    amount: String,
    network: Option<String>,
) -> Result<()> {
    let mut payload = HashMap::new();
    payload.insert(
        "from_address".to_string(),
        serde_json::Value::String(from_address),
    );
    payload.insert(
        "to_address".to_string(),
        serde_json::Value::String(to_address),
    );
    payload.insert("amount".to_string(), serde_json::Value::String(amount));

    if let Some(network) = network {
        payload.insert("network".to_string(), serde_json::Value::String(network));
    }

    worker.enqueue_job(JobType::BlockchainTx, payload).await
}
