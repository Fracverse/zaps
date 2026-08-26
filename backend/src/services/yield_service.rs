use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

/// Approximate ledger cadence / compounding base for Zaps yield calculation.
pub const SECONDS_PER_YEAR: i64 = 31_536_000;
/// Default APY rate in basis points (5.00%) if no rate is logged in the database.
pub const DEFAULT_APY_BPS: i32 = 500;
/// Total seconds in a day, representing the daily snapshot cadence.
pub const SECONDS_PER_DAY: i64 = 86_400;

/// Represents a single recorded yield snapshot for audit and reconciliation.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct YieldSnapshot {
    pub id: Uuid,
    pub user_id: Uuid,
    pub earning_balance: i64,
    pub accrued_interest: i64,
    pub apy: i32,
    pub created_at: NaiveDateTime,
}

/// Service for managing daily yield snapshots and audit trails.
pub struct YieldService;

impl YieldService {
    /// Captures a daily yield snapshot for all users with an active earning balance.
    ///
    /// # Architecture & Design
    /// This service leverages a single, optimized PostgreSQL transaction to execute the calculations
    /// and bulk-inserts directly inside the database engine.
    ///
    /// # Complexity Analysis
    /// - **Time Complexity**: O(N) where N is the number of active yield vault users. By shifting the
    ///   linear calculation directly to PostgreSQL, we avoid the network latency of pulling N rows
    ///   to the application layer and pushing N INSERT statements back.
    /// - **Space Complexity**: O(1) on the application server. The memory footprint remains constant
    ///   regardless of user base size because database rows are processed entirely in-engine.
    ///
    /// # Calculation Details
    /// - APY is read dynamically from the latest `yield_rates_history` record, defaulting to 500 bps (5.00%).
    /// - The daily interest is calculated using double-precision/arbitrary-precision casting to prevent
    ///   overflow of 64-bit integers:
    ///   `accrued_interest = (earning_balance * apy_bps * SECONDS_PER_DAY) / (10,000 * SECONDS_PER_YEAR)`
    pub async fn create_daily_snapshots(pool: &PgPool) -> Result<u64, sqlx::Error> {
        let mut tx = pool.begin().await?;

        // 1. Fetch current APY rate inside the transaction block (defaulting to 500 basis points if empty)
        let apy_bps: i32 = sqlx::query_scalar(
            r#"
            SELECT apy FROM yield_rates_history
            ORDER BY created_at DESC
            LIMIT 1
            "#,
        )
        .fetch_optional(&mut *tx)
        .await?
        .unwrap_or(DEFAULT_APY_BPS);

        // 2. Perform overflow-safe batch interest calculation and snapshot insert inside transaction block.
        // We cast parameters to NUMERIC during intermediate multiplication to handle arbitrarily large balances
        // safely, then cast back to BIGINT for storage.
        let result = sqlx::query(
            r#"
            INSERT INTO yield_snapshots (user_id, earning_balance, accrued_interest, apy, created_at)
            SELECT 
                user_id,
                earning_balance,
                CAST(
                    (CAST(earning_balance AS NUMERIC) * CAST($1 AS NUMERIC) * CAST($2 AS NUMERIC)) 
                    / (10000 * CAST($3 AS NUMERIC))
                    AS BIGINT
                ) AS accrued_interest,
                $1 AS apy,
                NOW() AS created_at
            FROM user_yield_balances
            WHERE earning_balance > 0
            "#,
        )
        .bind(apy_bps)
        .bind(SECONDS_PER_DAY)
        .bind(SECONDS_PER_YEAR)
        .execute(&mut *tx)
        .await?;

        let rows_affected = result.rows_affected();
        tx.commit().await?;

        tracing::info!(
            users_snapshotted = rows_affected,
            apy_bps = apy_bps,
            "Successfully created daily yield accrual snapshots"
        );

        Ok(rows_affected)
    }

    /// Fetches historical yield snapshots for a given user for audit trails.
    ///
    /// # Complexity Analysis
    /// - **Time Complexity**: O(log S + L) where S is the total number of snapshots and L is the requested limit,
    ///   leveraging the B-Tree index on `(user_id, created_at)`.
    /// - **Space Complexity**: O(L) to buffer the requested subset of results.
    pub async fn get_user_snapshots(
        pool: &PgPool,
        user_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<YieldSnapshot>, sqlx::Error> {
        let snapshots = sqlx::query_as::<_, YieldSnapshot>(
            r#"
            SELECT id, user_id, earning_balance, accrued_interest, apy, created_at
            FROM yield_snapshots
            WHERE user_id = $1
            ORDER BY created_at DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(user_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await?;

        Ok(snapshots)
    }
}
