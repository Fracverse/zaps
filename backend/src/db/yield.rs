use super::models::{UserYieldBalance, YieldRateHistory, YieldTransaction};
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

/// Get a user's yield balance or create one with zero balance if it doesn't exist
pub async fn get_or_create_yield_balance(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<UserYieldBalance, sqlx::Error> {
    let row = sqlx::query(
        r#"
        INSERT INTO user_yield_balances (user_id, available_balance, earning_balance, updated_at)
        VALUES ($1, 0, 0, NOW())
        ON CONFLICT (user_id) DO UPDATE SET updated_at = NOW()
        RETURNING user_id, available_balance, earning_balance, last_yield_sync_at, updated_at
        "#,
    )
    .bind(user_id)
    .fetch_one(pool)
    .await?;

    Ok(UserYieldBalance {
        user_id: row.get("user_id"),
        available_balance: row.get("available_balance"),
        earning_balance: row.get("earning_balance"),
        last_yield_sync_at: row.get("last_yield_sync_at"),
        updated_at: row.get("updated_at"),
    })
}

/// Apply a deposit securely (decreases available, increases earning)
pub async fn process_yield_deposit(
    pool: &PgPool,
    user_id: Uuid,
    amount: i64,
    tx_hash: &str,
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;

    process_yield_deposit_tx(&mut tx, user_id, amount, tx_hash).await?;

    tx.commit().await?;
    Ok(())
}

/// Same as above, but accepts an existing transaction to be composed in a larger transaction
pub async fn process_yield_deposit_tx(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    amount: i64,
    tx_hash: &str,
) -> Result<(), sqlx::Error> {
    // Record the transaction first to prevent duplicate processing via the tx_hash UNIQUE constraint
    sqlx::query(
        r#"
        INSERT INTO yield_transactions (user_id, tx_hash, type, amount, created_at)
        VALUES ($1, $2, 'DEPOSIT', $3, NOW())
        "#,
    )
    .bind(user_id)
    .bind(tx_hash)
    .bind(amount)
    .execute(&mut **tx)
    .await?;

    // Lock the balance row to prevent race conditions and apply atomic updates
    sqlx::query(
        r#"
        INSERT INTO user_yield_balances (user_id, available_balance, earning_balance, updated_at)
        VALUES ($1, 0, $2, NOW())
        ON CONFLICT (user_id) DO UPDATE 
        SET earning_balance = user_yield_balances.earning_balance + $2,
            last_yield_sync_at = NOW(),
            updated_at = NOW()
        "#,
    )
    .bind(user_id)
    .bind(amount)
    .execute(&mut **tx)
    .await?;

    Ok(())
}

/// Apply a withdrawal securely (decreases earning, increases available)
pub async fn process_yield_withdrawal(
    pool: &PgPool,
    user_id: Uuid,
    amount: i64,
    tx_hash: &str,
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;

    process_yield_withdrawal_tx(&mut tx, user_id, amount, tx_hash).await?;

    tx.commit().await?;
    Ok(())
}

/// Same as above, but accepts an existing transaction to be composed in a larger transaction
pub async fn process_yield_withdrawal_tx(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    amount: i64,
    tx_hash: &str,
) -> Result<(), sqlx::Error> {
    // Record the transaction first to prevent duplicate processing via the tx_hash UNIQUE constraint
    sqlx::query(
        r#"
        INSERT INTO yield_transactions (user_id, tx_hash, type, amount, created_at)
        VALUES ($1, $2, 'WITHDRAW', $3, NOW())
        "#,
    )
    .bind(user_id)
    .bind(tx_hash)
    .bind(amount)
    .execute(&mut **tx)
    .await?;

    // Lock the balance row to prevent race conditions and apply atomic updates
    // For withdraw, the check constraint (earning_balance >= 0) ensures we don't go negative
    sqlx::query(
        r#"
        UPDATE user_yield_balances
        SET earning_balance = earning_balance - $2,
            available_balance = available_balance + $2,
            last_yield_sync_at = NOW(),
            updated_at = NOW()
        WHERE user_id = $1
        "#,
    )
    .bind(user_id)
    .bind(amount)
    .execute(&mut **tx)
    .await?;

    Ok(())
}

/// Log an APY update
pub async fn log_yield_rate_update(pool: &PgPool, apy: i32) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    log_yield_rate_update_tx(&mut tx, apy).await?;
    tx.commit().await?;
    Ok(())
}

/// Same as above, but accepts an existing transaction so the rate update can
/// participate in a larger atomic batch.
pub async fn log_yield_rate_update_tx(
    tx: &mut Transaction<'_, Postgres>,
    apy: i32,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO yield_rates_history (apy, created_at)
        VALUES ($1, NOW())
        "#,
    )
    .bind(apy)
    .execute(&mut **tx)
    .await?;

    Ok(())
}

/// Get the current (latest) APY
pub async fn get_current_yield_rate(pool: &PgPool) -> Result<Option<i32>, sqlx::Error> {
    let rate = sqlx::query_scalar(
        r#"
        SELECT apy FROM yield_rates_history
        ORDER BY created_at DESC
        LIMIT 1
        "#,
    )
    .fetch_optional(pool)
    .await?;

    Ok(rate)
}

/// BE-547: age of the most recent APY checkpoint, in seconds.
///
/// `None` when no checkpoint has ever been recorded. Used by the hourly
/// checkpoint worker to decide whether a tick is actually due: `tokio::interval`
/// restarts its clock on process start, so without this a crash-looping or
/// frequently-redeployed process would write a checkpoint on every boot.
pub async fn seconds_since_last_yield_rate(pool: &PgPool) -> Result<Option<i64>, sqlx::Error> {
    let age: Option<f64> = sqlx::query_scalar(
        r#"
        SELECT EXTRACT(EPOCH FROM (NOW() - created_at))
          FROM yield_rates_history
         ORDER BY created_at DESC
         LIMIT 1
        "#,
    )
    .fetch_optional(pool)
    .await?;

    Ok(age.map(|secs| secs as i64))
}

/// Read whether the user has auto-earn (auto-sweep) enabled.
pub async fn get_auto_earn_enabled(pool: &PgPool, user_id: Uuid) -> Result<bool, sqlx::Error> {
    let enabled = sqlx::query_scalar(
        r#"
        SELECT auto_earn_enabled FROM users WHERE id = $1
        "#,
    )
    .bind(user_id)
    .fetch_one(pool)
    .await?;

    Ok(enabled)
}

/// Persist auto-earn preference on the user's profile row.
pub async fn set_auto_earn_enabled(
    pool: &PgPool,
    user_id: Uuid,
    enabled: bool,
) -> Result<bool, sqlx::Error> {
    let updated = sqlx::query_scalar(
        r#"
        UPDATE users
        SET auto_earn_enabled = $2
        WHERE id = $1
        RETURNING auto_earn_enabled
        "#,
    )
    .bind(user_id)
    .bind(enabled)
    .fetch_one(pool)
    .await?;

    Ok(updated)
}

/// Users with auto-earn on and idle available balance above `min_amount`.
pub async fn list_auto_sweep_candidates(
    pool: &PgPool,
    min_amount: i64,
    limit: i64,
) -> Result<Vec<UserYieldBalance>, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT b.user_id, b.available_balance, b.earning_balance, b.last_yield_sync_at, b.updated_at
        FROM user_yield_balances b
        JOIN users u ON u.id = b.user_id
        WHERE u.auto_earn_enabled = true
          AND b.available_balance >= $1
        ORDER BY b.updated_at ASC
        LIMIT $2
        "#,
    )
    .bind(min_amount)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| UserYieldBalance {
            user_id: row.get("user_id"),
            available_balance: row.get("available_balance"),
            earning_balance: row.get("earning_balance"),
            last_yield_sync_at: row.get("last_yield_sync_at"),
            updated_at: row.get("updated_at"),
        })
        .collect())
}

/// Move idle available balance into earning (off-chain auto-sweep).
pub async fn process_internal_sweep_deposit(
    pool: &PgPool,
    user_id: Uuid,
    amount: i64,
    tx_hash: &str,
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;

    sqlx::query(
        r#"
        INSERT INTO yield_transactions (user_id, tx_hash, type, amount, created_at)
        VALUES ($1, $2, 'SWEEP', $3, NOW())
        "#,
    )
    .bind(user_id)
    .bind(tx_hash)
    .bind(amount)
    .execute(&mut *tx)
    .await?;

    let updated = sqlx::query(
        r#"
        UPDATE user_yield_balances
        SET available_balance = available_balance - $2,
            earning_balance = earning_balance + $2,
            last_yield_sync_at = NOW(),
            updated_at = NOW()
        WHERE user_id = $1
          AND available_balance >= $2
        "#,
    )
    .bind(user_id)
    .bind(amount)
    .execute(&mut *tx)
    .await?;

    if updated.rows_affected() == 0 {
        return Err(sqlx::Error::RowNotFound);
    }

    tx.commit().await?;
    Ok(())
}

/// Touch the yield sync timestamp after an on-chain balance update.
pub async fn touch_yield_sync_at(pool: &PgPool, user_id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        UPDATE user_yield_balances
        SET last_yield_sync_at = NOW(), updated_at = NOW()
        WHERE user_id = $1
        "#,
    )
    .bind(user_id)
    .execute(pool)
    .await?;

    Ok(())
}

/// BE-053: Users currently in sweep-failure backoff (excluded from this cycle).
pub async fn list_sweep_backoff_excluded_users(
    pool: &PgPool,
) -> Result<Vec<Uuid>, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT user_id FROM sweep_failure_history
        WHERE next_retry_at > NOW()
        "#,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(|row| row.get("user_id")).collect())
}

/// BE-053: Record a sweep failure and push the user's next retry back with
/// exponential backoff (60s * 2^failures, capped at 1 hour) so repeated
/// failures don't flood the logs every cycle.
pub async fn record_sweep_failure(
    pool: &PgPool,
    user_id: Uuid,
    error: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO sweep_failure_history (user_id, failure_count, last_error, last_failed_at, next_retry_at)
        VALUES ($1, 1, $2, NOW(), NOW() + INTERVAL '60 seconds')
        ON CONFLICT (user_id) DO UPDATE
        SET failure_count = sweep_failure_history.failure_count + 1,
            last_error = EXCLUDED.last_error,
            last_failed_at = NOW(),
            next_retry_at = NOW() + (
                INTERVAL '60 seconds' * POWER(2, LEAST(sweep_failure_history.failure_count + 1, 6))
            )
        "#,
    )
    .bind(user_id)
    .bind(error)
    .execute(pool)
    .await?;

    Ok(())
}

/// BE-053: Clear a user's failure history after a successful sweep.
pub async fn clear_sweep_failure(pool: &PgPool, user_id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM sweep_failure_history WHERE user_id = $1")
        .bind(user_id)
        .execute(pool)
        .await?;

    Ok(())
}
