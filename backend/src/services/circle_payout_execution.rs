//! #1212 — Circle payout execution service.
//!
//! Executes an approved payout only after re-validating the request against the
//! circle's current state (approval threshold, asset and available funds). The
//! execution ledger row is claimed atomically with the payout state change, so
//! concurrent or repeated submissions can never produce a second transfer.

use crate::services::stellar::StellarClient;
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::json;
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PayoutError {
    InvalidTxHash,
    PayoutNotFound,
    NotApproved,
    AlreadyExecuted,
    InsufficientFunds,
    AssetMismatch,
    Internal,
}

impl PayoutError {
    pub fn code(self) -> &'static str {
        match self {
            Self::InvalidTxHash => "invalid_tx_hash",
            Self::PayoutNotFound => "payout_not_found",
            Self::NotApproved => "payout_not_approved",
            Self::AlreadyExecuted => "already_executed",
            Self::InsufficientFunds => "insufficient_funds",
            Self::AssetMismatch => "asset_mismatch",
            Self::Internal => "internal_error",
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PayoutExecution {
    pub id: Uuid,
    pub circle_id: Uuid,
    pub request_id: i64,
    pub tx_hash: String,
    pub status: String,
    pub verified_at: Option<DateTime<Utc>>,
}

/// Stellar transaction hashes are 64 hexadecimal characters.
pub fn is_valid_tx_hash(hash: &str) -> bool {
    hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Claim and record an approved payout execution.
///
/// Validation and the state transition happen in a single transaction. On any
/// failure nothing is committed, so a caller may safely retry after fixing the
/// underlying cause (e.g. additional funding).
pub async fn execute_approved_payout(
    pool: &PgPool,
    circle_id: Uuid,
    request_id: i64,
    actor: &str,
    tx_hash: &str,
) -> Result<PayoutExecution, PayoutError> {
    let tx_hash = tx_hash.trim().to_ascii_lowercase();
    if !is_valid_tx_hash(&tx_hash) {
        return Err(PayoutError::InvalidTxHash);
    }

    let mut tx = pool.begin().await.map_err(|_| PayoutError::Internal)?;

    let row = sqlx::query(
        r#"
        SELECT p.amount, p.to_address, p.asset_address, p.status, p.is_executed,
               p.threshold_at_creation, c.balance, c.reserved, c.token_address,
               (SELECT COUNT(*) FROM circle_payout_approvals a
                 WHERE a.circle_id = p.circle_id AND a.request_id = p.request_id) AS approvals
          FROM circle_payout_requests p
          JOIN circles c ON c.id = p.circle_id
         WHERE p.circle_id = $1 AND p.request_id = $2
         FOR UPDATE OF p, c
        "#,
    )
    .bind(circle_id)
    .bind(request_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| PayoutError::Internal)?
    .ok_or(PayoutError::PayoutNotFound)?;

    if row.get::<bool, _>("is_executed") {
        return Err(PayoutError::AlreadyExecuted);
    }
    if row.get::<String, _>("status") != "approved" {
        return Err(PayoutError::NotApproved);
    }
    let approvals: i64 = row.get("approvals");
    let threshold: i32 = row.get("threshold_at_creation");
    if approvals < threshold as i64 {
        return Err(PayoutError::NotApproved);
    }

    let amount: i64 = row.get("amount");
    if amount <= 0 {
        return Err(PayoutError::NotApproved);
    }
    let balance: i64 = row.get("balance");
    let reserved: i64 = row.get("reserved");
    if balance - reserved < amount {
        return Err(PayoutError::InsufficientFunds);
    }
    let asset: String = row.get("asset_address");
    let token: String = row.get("token_address");
    if !token.is_empty() && asset != token {
        return Err(PayoutError::AssetMismatch);
    }

    let execution_id: Option<Uuid> = sqlx::query_as(
        r#"
        INSERT INTO circle_payout_executions
            (circle_id, request_id, tx_hash, expected_to, expected_amount,
             expected_asset, executed_by)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        ON CONFLICT (circle_id, request_id) DO NOTHING
        RETURNING id
        "#,
    )
    .bind(circle_id)
    .bind(request_id)
    .bind(&tx_hash)
    .bind(row.get::<String, _>("to_address"))
    .bind(amount)
    .bind(&asset)
    .bind(actor)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| PayoutError::Internal)?;
    let id = execution_id.ok_or(PayoutError::AlreadyExecuted)?;

    let claimed = sqlx::query(
        r#"
        UPDATE circle_payout_requests
           SET status = 'executed', is_executed = TRUE, tx_hash = $3
         WHERE circle_id = $1 AND request_id = $2 AND is_executed = FALSE
        "#,
    )
    .bind(circle_id)
    .bind(request_id)
    .bind(&tx_hash)
    .execute(&mut *tx)
    .await
    .map_err(|_| PayoutError::Internal)?;
    if claimed.rows_affected() == 0 {
        return Err(PayoutError::AlreadyExecuted);
    }

    // Reserve the funds until the on-chain verification confirms settlement.
    sqlx::query(
        r#"
        UPDATE circles
           SET reserved = reserved + $1, total_paid = total_paid + $1,
               updated_at = NOW()
         WHERE id = $2
        "#,
    )
    .bind(amount)
    .bind(circle_id)
    .execute(&mut *tx)
    .await
    .map_err(|_| PayoutError::Internal)?;

    tx.commit().await.map_err(|_| PayoutError::Internal)?;

    Ok(PayoutExecution {
        id,
        circle_id,
        request_id,
        tx_hash,
        status: "submitted".to_string(),
        verified_at: None,
    })
}

/// Verify a submitted transaction on-chain and persist the result.
/// A non-success transaction is recorded as `failed` so operators can retry or
/// investigate without ever creating a second execution row.
pub async fn verify_on_chain(
    pool: &PgPool,
    execution_id: Uuid,
    tx_hash: &str,
) -> Result<bool, String> {
    let rpc_url = std::env::var("STELLAR_RPC_URL").unwrap_or_default();
    if rpc_url.trim().is_empty() {
        return Ok(false); // No RPC configured: keep the submission record as-is.
    }

    let client = StellarClient::new(rpc_url);
    let response = client
        .send_rpc_request("getTransaction", json!({ "hash": tx_hash }))
        .await
        .map_err(|e| e.to_string())?;
    let succeeded = response
        .get("result")
        .and_then(|r| r.get("status"))
        .and_then(|s| s.as_str())
        == Some("SUCCESS");

    let status = if succeeded { "verified" } else { "failed" };
    let reason = if succeeded {
        None
    } else {
        Some("transaction was not confirmed on-chain".to_string())
    };
    sqlx::query(
        r#"
        UPDATE circle_payout_executions
           SET status = $1, failure_reason = $2,
               verified_at = CASE WHEN $3 THEN NOW() ELSE NULL END
         WHERE id = $4
        "#,
    )
    .bind(status)
    .bind(reason)
    .bind(succeeded)
    .bind(execution_id)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(succeeded)
}

/// Test-only helper: map an error to the API error string used by handlers.
impl From<PayoutError> for &'static str {
    fn from(value: PayoutError) -> Self {
        value.code()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stellar_tx_hash_validation() {
        let valid = "a".repeat(64);
        assert!(is_valid_tx_hash(&valid));
        assert!(is_valid_tx_hash(&valid.to_ascii_uppercase()));
        assert!(!is_valid_tx_hash("abc"));
        assert!(!is_valid_tx_hash(&format!("{valid}z")));
    }

    #[test]
    fn errors_map_to_stable_codes() {
        assert_eq!(PayoutError::AlreadyExecuted.code(), "already_executed");
        assert_eq!(PayoutError::InsufficientFunds.code(), "insufficient_funds");
    }
}
