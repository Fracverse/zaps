use crate::job_types::{JobPayload, JobResult, DeadLetterJob};
use anyhow::{Context, Result};
use async_trait::async_trait;
use bb8_redis::{bb8::Pool, redis::AsyncCommands, RedisConnectionManager};
use chrono::{DateTime, Utc};
use serde_json;
use std::time::Duration;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

const DEFAULT_QUEUE: &str = "zaps:jobs:queue";
const PROCESSING_QUEUE: &str = "zaps:jobs:processing";
const DEAD_LETTER_QUEUE: &str = "zaps:jobs:dead_letter";
const RETRY_QUEUE: &str = "zaps:jobs:retry";

#[derive(Debug, Clone)]
pub struct QueueConfig {
    pub max_retries: u32,
    pub visibility_timeout: Duration,
    pub backoff_multiplier: f64,
    pub max_backoff: Duration,
    pub dead_letter_max_size: usize,
}

impl Default for QueueConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            visibility_timeout: Duration::from_secs(300), // 5 minutes
            backoff_multiplier: 2.0,
            max_backoff: Duration::from_secs(3600), // 1 hour
            dead_letter_max_size: 10000,
        }
    }
}

#[async_trait]
pub trait JobProcessor: Send + Sync {
    async fn process(&self, job: &JobPayload) -> Result<JobResult>;
}

pub struct JobQueue {
    pool: Pool<RedisConnectionManager>,
    config: QueueConfig,
}

impl JobQueue {
    pub async fn new(redis_url: &str, config: QueueConfig) -> Result<Self> {
        let manager = RedisConnectionManager::new(redis_url)
            .context("Failed to create Redis connection manager")?;
        let pool = Pool::builder()
            .max_size(20)
            .build(manager)
            .await
            .context("Failed to create Redis connection pool")?;

        Ok(Self { pool, config })
    }

    pub async fn enqueue(&self, job: JobPayload) -> Result<()> {
        let mut conn = self.pool.get().await?;
        let job_json = serde_json::to_string(&job)
            .context("Failed to serialize job")?;

        let score = job.scheduled_at
            .unwrap_or_else(|| Utc::now())
            .timestamp();

        conn.zadd(DEFAULT_QUEUE, &job_json, score)
            .await
            .context("Failed to enqueue job")?;

        info!("Enqueued job {} of type {:?}", job.id, job.job_type);
        Ok(())
    }

    pub async fn dequeue(&self) -> Result<Option<JobPayload>> {
        let mut conn = self.pool.get().await?;
        
        // Get the earliest ready job
        let now = Utc::now().timestamp();
        let jobs: Vec<String> = conn
            .zrangebyscore(DEFAULT_QUEUE, "-inf", now)
            .limit(1)
            .collect::<Vec<String>>()
            .await
            .context("Failed to fetch jobs from queue")?;

        if jobs.is_empty() {
            return Ok(None);
        }

        let job_json = &jobs[0];
        let job: JobPayload = serde_json::from_str(job_json)
            .context("Failed to deserialize job")?;

        // Move job to processing queue
        let _: () = conn
            .zrem(DEFAULT_QUEUE, job_json)
            .await
            .context("Failed to remove job from main queue")?;

        let processing_score = (Utc::now() + chrono::Duration::from_std(self.config.visibility_timeout)
            .unwrap()).timestamp();
        
        conn.zadd(PROCESSING_QUEUE, job_json, processing_score)
            .await
            .context("Failed to add job to processing queue")?;

        debug!("Dequeued job {} for processing", job.id);
        Ok(Some(job))
    }

    pub async fn complete_job(&self, job_id: Uuid, result: JobResult) -> Result<()> {
        let mut conn = self.pool.get().await?;
        
        // Remove from processing queue
        let jobs: Vec<String> = conn
            .zrange(PROCESSING_QUEUE, 0, -1)
            .collect::<Vec<String>>()
            .await
            .context("Failed to fetch processing jobs")?;

        for job_json in jobs {
            let job: JobPayload = serde_json::from_str(&job_json)
                .context("Failed to deserialize processing job")?;
            
            if job.id == job_id {
                conn.zrem(PROCESSING_QUEUE, &job_json)
                    .await
                    .context("Failed to remove completed job from processing queue")?;

                if result.success {
                    info!("Successfully completed job {}", job_id);
                } else {
                    warn!("Job {} completed with errors: {:?}", job_id, result.error);
                }
                break;
            }
        }

        Ok(())
    }

    pub async fn retry_job(&self, job: JobPayload, error: String) -> Result<()> {
        let current_attempt = job.retries.unwrap_or(0) + 1;
        
        if current_attempt >= self.config.max_retries {
            return self.send_to_dead_letter(job, error, current_attempt).await;
        }

        let backoff_delay = self.calculate_backoff_delay(current_attempt);
        let retry_job = JobPayload {
            id: job.id,
            job_type: job.job_type,
            payload: job.payload,
            retries: Some(current_attempt),
            created_at: job.created_at,
            scheduled_at: Some(Utc::now() + backoff_delay),
        };

        let mut conn = self.pool.get().await?;
        let job_json = serde_json::to_string(&retry_job)
            .context("Failed to serialize retry job")?;

        let score = retry_job.scheduled_at.unwrap().timestamp();
        conn.zadd(RETRY_QUEUE, &job_json, score)
            .await
            .context("Failed to add job to retry queue")?;

        // Remove from processing queue
        let original_json = serde_json::to_string(&job)
            .context("Failed to serialize original job")?;
        conn.zrem(PROCESSING_QUEUE, &original_json)
            .await
            .context("Failed to remove job from processing queue")?;

        warn!("Retrying job {} (attempt {}/{}) in {:?}", 
              job.id, current_attempt, self.config.max_retries, backoff_delay);
        
        Ok(())
    }

    pub async fn process_retry_queue(&self) -> Result<()> {
        let mut conn = self.pool.get().await?;
        
        let now = Utc::now().timestamp();
        let retry_jobs: Vec<String> = conn
            .zrangebyscore(RETRY_QUEUE, "-inf", now)
            .collect::<Vec<String>>()
            .await
            .context("Failed to fetch retry jobs")?;

        for job_json in retry_jobs {
            let job: JobPayload = serde_json::from_str(&job_json)
                .context("Failed to deserialize retry job")?;

            // Move back to main queue
            conn.zrem(RETRY_QUEUE, &job_json)
                .await
                .context("Failed to remove job from retry queue")?;

            let score = job.scheduled_at.unwrap().timestamp();
            conn.zadd(DEFAULT_QUEUE, &job_json, score)
                .await
                .context("Failed to move job back to main queue")?;

            debug!("Moved retry job {} back to main queue", job.id);
        }

        Ok(())
    }

    async fn send_to_dead_letter(&self, job: JobPayload, error: String, total_attempts: u32) -> Result<()> {
        let dead_letter_job = DeadLetterJob {
            original_job: job.clone(),
            error,
            failed_at: Utc::now(),
            total_attempts,
        };

        let mut conn = self.pool.get().await?;
        let dlq_json = serde_json::to_string(&dead_letter_job)
            .context("Failed to serialize dead letter job")?;

        // Check dead letter queue size
        let current_size: usize = conn.llen(DEAD_LETTER_QUEUE)
            .await
            .context("Failed to check dead letter queue size")?;

        if current_size >= self.config.dead_letter_max_size {
            // Remove oldest job if queue is full
            let _: () = conn.lpop(DEAD_LETTER_QUEUE, None)
                .await
                .context("Failed to remove oldest dead letter job")?;
        }

        conn.rpush(DEAD_LETTER_QUEUE, &dlq_json)
            .await
            .context("Failed to add job to dead letter queue")?;

        // Remove from processing queue
        let original_json = serde_json::to_string(&job)
            .context("Failed to serialize original job")?;
        conn.zrem(PROCESSING_QUEUE, &original_json)
            .await
            .context("Failed to remove job from processing queue")?;

        error!("Job {} sent to dead letter queue after {} attempts", job.id, total_attempts);
        Ok(())
    }

    fn calculate_backoff_delay(&self, attempt: u32) -> chrono::Duration {
        let base_delay = Duration::from_secs(1);
        let multiplier = self.config.backoff_multiplier.powi(attempt as i32);
        let delay = base_delay * multiplier as u32;
        
        let max_delay = std::cmp::min(delay, self.config.max_backoff);
        chrono::Duration::from_std(max_delay).unwrap()
    }

    pub async fn get_queue_stats(&self) -> Result<QueueStats> {
        let mut conn = self.pool.get().await?;
        
        let main_queue_size: usize = conn.zcard(DEFAULT_QUEUE)
            .await
            .context("Failed to get main queue size")?;
        
        let processing_size: usize = conn.zcard(PROCESSING_QUEUE)
            .await
            .context("Failed to get processing queue size")?;
        
        let retry_size: usize = conn.zcard(RETRY_QUEUE)
            .await
            .context("Failed to get retry queue size")?;
        
        let dead_letter_size: usize = conn.llen(DEAD_LETTER_QUEUE)
            .await
            .context("Failed to get dead letter queue size")?;

        Ok(QueueStats {
            main_queue_size,
            processing_size,
            retry_size,
            dead_letter_size,
        })
    }

    pub async fn reclaim_stalled_jobs(&self) -> Result<usize> {
        let mut conn = self.pool.get().await?;
        
        let now = Utc::now().timestamp();
        let stalled_jobs: Vec<String> = conn
            .zrangebyscore(PROCESSING_QUEUE, "-inf", now)
            .collect::<Vec<String>>()
            .await
            .context("Failed to fetch stalled jobs")?;

        let mut reclaimed_count = 0;

        for job_json in stalled_jobs {
            let job: JobPayload = serde_json::from_str(&job_json)
                .context("Failed to deserialize stalled job")?;

            // Remove from processing queue
            conn.zrem(PROCESSING_QUEUE, &job_json)
                .await
                .context("Failed to remove stalled job from processing queue")?;

            // Add back to main queue
            let score = job.scheduled_at.unwrap_or_else(|| Utc::now()).timestamp();
            conn.zadd(DEFAULT_QUEUE, &job_json, score)
                .await
                .context("Failed to requeue stalled job")?;

            reclaimed_count += 1;
            warn!("Reclaimed stalled job {}", job.id);
        }

        if reclaimed_count > 0 {
            info!("Reclaimed {} stalled jobs", reclaimed_count);
        }

        Ok(reclaimed_count)
    }
}

#[derive(Debug, Clone)]
pub struct QueueStats {
    pub main_queue_size: usize,
    pub processing_size: usize,
    pub retry_size: usize,
    pub dead_letter_size: usize,
}
