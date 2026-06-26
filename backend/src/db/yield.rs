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
        RETURNING user_id, available_balance, earning_balance, updated_at
        "#,
    )
    .bind(user_id)
    .fetch_one(pool)
    .await?;

    Ok(UserYieldBalance {
        user_id: row.get("user_id"),
        available_balance: row.get("available_balance"),
        earning_balance: row.get("earning_balance"),
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
    sqlx::query(
        r#"
        INSERT INTO yield_rates_history (apy, created_at)
        VALUES ($1, NOW())
        "#,
    )
    .bind(apy)
    .execute(pool)
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
