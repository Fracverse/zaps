use crate::api_error::ApiError;
use deadpool_postgres::Pool;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio_postgres::Row;
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum ScheduleType {
    ONE_TIME,
    RECURRING,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PaymentSchedule {
    pub id: Uuid,
    pub merchant_id: String,
    pub from_address: String,
    pub to_address: String,
    pub send_asset: String,
    pub send_amount: i64,
    pub memo: Option<String>,
    pub schedule_type: String,
    pub interval_seconds: Option<i64>,
    pub next_run: chrono::DateTime<chrono::Utc>,
    pub status: String,
    pub retries: i32,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

pub struct ScheduleService {
    db_pool: Arc<Pool>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ScheduleRun {
    pub id: Uuid,
    pub schedule_id: Uuid,
    pub attempted_at: chrono::DateTime<chrono::Utc>,
    pub success: bool,
    pub error: Option<String>,
    pub external_payment_id: Option<String>,
}

impl ScheduleService {
    pub fn new(db_pool: Arc<Pool>) -> Self {
        Self { db_pool }
    }

    pub async fn create_one_time_schedule(
        &self,
        merchant_id: &str,
        from_address: &str,
        to_address: &str,
        send_asset: &str,
        send_amount: i64,
        memo: Option<&str>,
        run_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<PaymentSchedule, ApiError> {
        let client = self.db_pool.get().await?;
        let id = Uuid::new_v4();

        client
            .execute(
                "INSERT INTO payment_schedules (id, merchant_id, from_address, to_address, send_asset, send_amount, memo, schedule_type, next_run, status) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)",
                &[&id, &merchant_id, &from_address, &to_address, &send_asset, &send_amount, &memo, &"ONE_TIME", &run_at, &"ACTIVE"],
            )
            .await?;

        Ok(self.get_schedule(id).await?)
    }

    pub async fn create_recurring_schedule(
        &self,
        merchant_id: &str,
        from_address: &str,
        to_address: &str,
        send_asset: &str,
        send_amount: i64,
        memo: Option<&str>,
        interval_seconds: i64,
        first_run: chrono::DateTime<chrono::Utc>,
    ) -> Result<PaymentSchedule, ApiError> {
        let client = self.db_pool.get().await?;
        let id = Uuid::new_v4();

        client
            .execute(
                "INSERT INTO payment_schedules (id, merchant_id, from_address, to_address, send_asset, send_amount, memo, schedule_type, interval_seconds, next_run, status) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)",
                &[&id, &merchant_id, &from_address, &to_address, &send_asset, &send_amount, &memo, &"RECURRING", &interval_seconds, &first_run, &"ACTIVE"],
            )
            .await?;

        Ok(self.get_schedule(id).await?)
    }

    pub async fn get_schedule(&self, id: Uuid) -> Result<PaymentSchedule, ApiError> {
        let client = self.db_pool.get().await?;

        let row = client
            .query_one(
                "SELECT id, merchant_id, from_address, to_address, send_asset, send_amount, memo, schedule_type, interval_seconds, next_run, status, retries, created_at, updated_at FROM payment_schedules WHERE id = $1",
                &[&id],
            )
            .await
            .map_err(|_| ApiError::NotFound("Schedule not found".to_string()))?;

        Ok(row_to_schedule(&row))
    }

    pub async fn list_due_schedules(
        &self,
        limit: i64,
    ) -> Result<Vec<PaymentSchedule>, anyhow::Error> {
        let client = self.db_pool.get().await?;

        let now = chrono::Utc::now();

        let rows = client
            .query(
                "SELECT id, merchant_id, from_address, to_address, send_asset, send_amount, memo, schedule_type, interval_seconds, next_run, status, retries, created_at, updated_at FROM payment_schedules WHERE status = 'ACTIVE' AND next_run <= $1 ORDER BY next_run ASC LIMIT $2",
                &[&now, &limit],
            )
            .await?;

        Ok(rows.into_iter().map(row_to_schedule).collect())
    }

    pub async fn mark_run_result(
        &self,
        schedule_id: Uuid,
        success: bool,
        error: Option<String>,
        external_payment_id: Option<String>,
    ) -> Result<(), anyhow::Error> {
        let client = self.db_pool.get().await?;

        let run_id = Uuid::new_v4();

        client
            .execute(
                "INSERT INTO payment_schedule_runs (id, schedule_id, success, error, external_payment_id) VALUES ($1,$2,$3,$4,$5)",
                &[&run_id, &schedule_id, &success, &error, &external_payment_id],
            )
            .await?;

        let schedule = self.get_schedule(schedule_id).await?;
        let now = chrono::Utc::now();
        let retry_delay = chrono::Duration::seconds(60);
        let max_retries = 3;

        if success {
            if schedule.schedule_type == "ONE_TIME" {
                client
                    .execute(
                        "UPDATE payment_schedules SET status = 'COMPLETED', retries = 0, updated_at = NOW() WHERE id = $1",
                        &[&schedule_id],
                    )
                    .await?;
            } else if let Some(interval) = schedule.interval_seconds {
                let next = now + chrono::Duration::seconds(interval);
                client
                    .execute(
                        "UPDATE payment_schedules SET next_run = $1, retries = 0, updated_at = NOW() WHERE id = $2",
                        &[&next, &schedule_id],
                    )
                    .await?;
            }
        } else {
            let next_run = now + retry_delay;
            let retries = schedule.retries + 1;
            let status = if retries >= max_retries { "FAILED" } else { "ACTIVE" };

            client
                .execute(
                    "UPDATE payment_schedules SET next_run = $1, retries = $2, status = $3, updated_at = NOW() WHERE id = $4",
                    &[&next_run, &retries, &status, &schedule_id],
                )
                .await?;
        }

        Ok(())
    }

    pub async fn list_schedule_runs(&self, schedule_id: Uuid) -> Result<Vec<ScheduleRun>, anyhow::Error> {
        let client = self.db_pool.get().await?;
        let rows = client
            .query(
                "SELECT id, schedule_id, attempted_at, success, error, external_payment_id FROM payment_schedule_runs WHERE schedule_id = $1 ORDER BY attempted_at DESC",
                &[&schedule_id],
            )
            .await?;

        Ok(rows
            .into_iter()
            .map(|row| ScheduleRun {
                id: row.get(0),
                schedule_id: row.get(1),
                attempted_at: row.get(2),
                success: row.get(3),
                error: row.get(4),
                external_payment_id: row.get(5),
            })
            .collect())
    }

    pub async fn modify_schedule(
        &self,
        id: Uuid,
        interval_seconds: Option<i64>,
        next_run: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Result<PaymentSchedule, ApiError> {
        let client = self.db_pool.get().await?;

        let res = client
            .execute(
                "UPDATE payment_schedules SET interval_seconds = COALESCE($1, interval_seconds), next_run = COALESCE($2, next_run), updated_at = NOW() WHERE id = $3 AND status = 'ACTIVE'",
                &[&interval_seconds, &next_run, &id],
            )
            .await?;

        if res == 0 {
            return Err(ApiError::NotFound("Schedule not found or not active".to_string()));
        }

        Ok(self.get_schedule(id).await?)
    }

    pub async fn cancel_schedule(&self, id: Uuid) -> Result<(), ApiError> {
        let client = self.db_pool.get().await?;

        let res = client
            .execute(
                "UPDATE payment_schedules SET status = 'CANCELLED', updated_at = NOW() WHERE id = $1",
                &[&id],
            )
            .await?;

        if res == 0 {
            return Err(ApiError::NotFound("Schedule not found".to_string()));
        }

        Ok(())
    }
}

fn row_to_schedule(row: &Row) -> PaymentSchedule {
    PaymentSchedule {
        id: row.get(0),
        merchant_id: row.get(1),
        from_address: row.get(2),
        to_address: row.get(3),
        send_asset: row.get(4),
        send_amount: row.get(5),
        memo: row.get(6),
        schedule_type: row.get(7),
        interval_seconds: row.get(8),
        next_run: row.get(9),
        status: row.get(10),
        retries: row.get(11),
        created_at: row.get(12),
        updated_at: row.get(13),
    }
}
