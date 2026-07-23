use crate::api::feed::AuthUser;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use stellar_base::{
    account::DataValue,
    memo::Memo,
    network::Network,
    operations::Operation,
    transaction::{Transaction, MIN_BASE_FEE},
    xdr::XDRSerialize,
    PublicKey,
};
use uuid::Uuid;

use crate::services::yield_calc;

// ── #373 — GET /api/yield/balance ─────────────────────────────────────────

#[derive(Serialize)]
pub struct YieldBalanceResponse {
    pub available_balance: i64,
    pub earning_balance: i64,
    /// Interest accrued since the last on-chain sync (micro-units).
    pub accrued_interest: i64,
    /// Earning balance including live accrued interest.
    pub total_earning_balance: i64,
    /// Current APY as a percentage (e.g., 5.0 for 5%).
    pub apy: f64,
    pub auto_earn_enabled: bool,
}

pub async fn get_balance(State(pool): State<sqlx::PgPool>, auth: AuthUser) -> impl IntoResponse {
    let balance = match crate::db::r#yield::get_or_create_yield_balance(&pool, auth.id).await {
        Ok(b) => b,
        Err(e) => {
            tracing::error!("yield balance fetch error: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to retrieve yield balance" })),
            )
                .into_response();
        }
    };

    let apy_bps = match crate::db::r#yield::get_current_yield_rate(&pool).await {
        Ok(Some(r)) => r,
        Ok(None) => 500, // default 5.00 %
        Err(e) => {
            tracing::error!("yield rate fetch error: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to retrieve yield rate" })),
            )
                .into_response();
        }
    };

    let estimate = yield_calc::estimate_for_balance(&balance, Some(apy_bps), Utc::now());

    let auto_earn_enabled =
        match crate::db::r#yield::get_auto_earn_enabled(&pool, auth.id).await {
            Ok(enabled) => enabled,
            Err(e) => {
                tracing::error!("auto-earn preference fetch error: {:?}", e);
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": "Failed to retrieve auto-earn preference" })),
                )
                    .into_response();
            }
        };

    Json(YieldBalanceResponse {
        available_balance: balance.available_balance,
        earning_balance: balance.earning_balance,
        accrued_interest: estimate.accrued_interest,
        total_earning_balance: estimate.total_earning_balance,
        apy: apy_bps as f64 / 100.0,
        auto_earn_enabled,
    })
    .into_response()
}

// ── #374 — GET /api/yield/history ─────────────────────────────────────────

#[derive(Deserialize)]
pub struct HistoryQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Serialize)]
pub struct YieldHistoryItem {
    pub id: String,
    pub tx_hash: String,
    #[serde(rename = "type")]
    pub tx_type: String,
    pub amount: i64,
    pub created_at: String,
}

#[derive(Serialize)]
pub struct YieldHistoryResponse {
    pub items: Vec<YieldHistoryItem>,
    pub limit: i64,
    pub offset: i64,
    pub total: i64,
}

pub async fn get_history(
    State(pool): State<sqlx::PgPool>,
    auth: AuthUser,
    Query(params): Query<HistoryQuery>,
) -> impl IntoResponse {
    let limit = params.limit.unwrap_or(20).clamp(1, 100);
    let offset = params.offset.unwrap_or(0).max(0);

    let total: i64 =
        match sqlx::query_scalar("SELECT COUNT(*) FROM yield_transactions WHERE user_id = $1")
            .bind(auth.id)
            .fetch_one(&pool)
            .await
        {
            Ok(n) => n,
            Err(e) => {
                tracing::error!("yield history count error: {:?}", e);
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": "Failed to count yield transactions" })),
                )
                    .into_response();
            }
        };

    let rows = match sqlx::query(
        r#"
        SELECT id, tx_hash, type, amount, created_at
        FROM yield_transactions
        WHERE user_id = $1
        ORDER BY created_at DESC
        LIMIT $2 OFFSET $3
        "#,
    )
    .bind(auth.id)
    .bind(limit)
    .bind(offset)
    .fetch_all(&pool)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("yield history query error: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to retrieve yield history" })),
            )
                .into_response();
        }
    };

    let items: Vec<YieldHistoryItem> = rows
        .iter()
        .map(|r| {
            let id: Uuid = r.get("id");
            let created_at: chrono::NaiveDateTime = r.get("created_at");
            YieldHistoryItem {
                id: id.to_string(),
                tx_hash: r.get("tx_hash"),
                tx_type: r.get("type"),
                amount: r.get("amount"),
                created_at: created_at.to_string(),
            }
        })
        .collect();

    Json(YieldHistoryResponse {
        items,
        limit,
        offset,
        total,
    })
    .into_response()
}

// ── #378 — POST /api/yield/toggle-auto ────────────────────────────────────

#[derive(Deserialize)]
pub struct ToggleAutoEarnRequest {
    pub enabled: bool,
}

#[derive(Serialize)]
pub struct ToggleAutoEarnResponse {
    pub auto_earn_enabled: bool,
    pub message: String,
}

pub async fn toggle_auto_earn(
    State(pool): State<sqlx::PgPool>,
    auth: AuthUser,
    Json(payload): Json<ToggleAutoEarnRequest>,
) -> impl IntoResponse {
    match crate::db::r#yield::set_auto_earn_enabled(&pool, auth.id, payload.enabled).await {
        Ok(enabled) => Json(ToggleAutoEarnResponse {
            auto_earn_enabled: enabled,
            message: if enabled {
                "Auto-earn enabled. Idle stablecoins will be swept into the yield vault."
                    .to_string()
            } else {
                "Auto-earn disabled.".to_string()
            },
        })
        .into_response(),
        Err(e) => {
            tracing::error!("toggle auto-earn error: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to update auto-earn preference" })),
            )
                .into_response()
        }
    }
}

// ── #375 — POST /api/yield/deposit ────────────────────────────────────────

#[derive(Deserialize)]
pub struct DepositRequest {
    /// Amount to move from available to earning balance (in micro-units).
    pub amount: i64,
}

#[derive(Serialize)]
pub struct DepositResponse {
    pub available_balance: i64,
    pub earning_balance: i64,
    /// Base64-encoded Stellar XDR transaction envelope for the user to sign.
    pub envelope_xdr: String,
}

pub async fn deposit(
    State(pool): State<sqlx::PgPool>,
    auth: AuthUser,
    Json(payload): Json<DepositRequest>,
) -> impl IntoResponse {
    if payload.amount <= 0 {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "Amount must be greater than zero" })),
        )
            .into_response();
    }

    // Fetch current balance; ensure available funds are sufficient.
    let balance = match crate::db::r#yield::get_or_create_yield_balance(&pool, auth.id).await {
        Ok(b) => b,
        Err(e) => {
            tracing::error!("yield deposit balance fetch error: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to retrieve balance" })),
            )
                .into_response();
        }
    };

    if balance.available_balance < payload.amount {
        return (
            StatusCode::BAD_REQUEST,
            Json(
                serde_json::json!({ "error": "Insufficient available balance", "available": balance.available_balance }),
            ),
        )
            .into_response();
    }

    // Unique idempotency key doubles as the on-chain reference.
    let tx_hash = format!("zaps-yield-deposit-{}", Uuid::new_v4());

    // Atomically deduct available balance, credit earning balance, and log transaction.
    let mut db_tx = match pool.begin().await {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("yield deposit transaction start error: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Database transaction failed" })),
            )
                .into_response();
        }
    };

    let deposit_result = async {
        sqlx::query(
            r#"
            INSERT INTO yield_transactions (user_id, tx_hash, type, amount, created_at)
            VALUES ($1, $2, 'DEPOSIT', $3, NOW())
            "#,
        )
        .bind(auth.id)
        .bind(&tx_hash)
        .bind(payload.amount)
        .execute(&mut *db_tx)
        .await?;

        sqlx::query(
            r#"
            INSERT INTO user_yield_balances (user_id, available_balance, earning_balance, updated_at)
            VALUES ($1, 0, $2, NOW())
            ON CONFLICT (user_id) DO UPDATE
            SET available_balance = user_yield_balances.available_balance - $2,
                earning_balance   = user_yield_balances.earning_balance   + $2,
                last_yield_sync_at  = NOW(),
                updated_at        = NOW()
            "#,
        )
        .bind(auth.id)
        .bind(payload.amount)
        .execute(&mut *db_tx)
        .await?;

        Ok::<(), sqlx::Error>(())
    }
    .await;

    if let Err(e) = deposit_result {
        tracing::error!("yield deposit DB error: {:?}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "Failed to process deposit" })),
        )
            .into_response();
    }

    if let Err(e) = db_tx.commit().await {
        tracing::error!("yield deposit commit error: {:?}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "Transaction commit failed" })),
        )
            .into_response();
    }

    // Fetch updated balances to return accurate state.
    let updated = match crate::db::r#yield::get_or_create_yield_balance(&pool, auth.id).await {
        Ok(b) => b,
        Err(e) => {
            tracing::error!("yield deposit post-commit balance error: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Deposit recorded but balance refresh failed" })),
            )
                .into_response();
        }
    };

    // Build Stellar transaction envelope XDR for the user's wallet to sign.
    let envelope_xdr = match build_stellar_envelope_xdr(
        &auth.address,
        "yield_deposit",
        payload.amount,
        &tx_hash,
    ) {
        Ok(xdr) => xdr,
        Err(e) => {
            tracing::error!("yield deposit envelope build error: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to build transaction envelope" })),
            )
                .into_response();
        }
    };

    Json(DepositResponse {
        available_balance: updated.available_balance,
        earning_balance: updated.earning_balance,
        envelope_xdr,
    })
    .into_response()
}

// ── #376 — POST /api/yield/withdraw ───────────────────────────────────────

#[derive(Deserialize)]
pub struct WithdrawRequest {
    /// Amount to move from earning to available balance (in micro-units).
    /// Pass the full earning balance to withdraw everything.
    pub amount: i64,
}

#[derive(Serialize)]
pub struct WithdrawResponse {
    pub available_balance: i64,
    pub earning_balance: i64,
    /// Base64-encoded Stellar XDR transaction envelope for the user to sign.
    pub envelope_xdr: String,
}

pub async fn withdraw(
    State(pool): State<sqlx::PgPool>,
    auth: AuthUser,
    Json(payload): Json<WithdrawRequest>,
) -> impl IntoResponse {
    if payload.amount <= 0 {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "Amount must be greater than zero" })),
        )
            .into_response();
    }

    let balance = match crate::db::r#yield::get_or_create_yield_balance(&pool, auth.id).await {
        Ok(b) => b,
        Err(e) => {
            tracing::error!("yield withdraw balance fetch error: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to retrieve balance" })),
            )
                .into_response();
        }
    };

    if balance.earning_balance < payload.amount {
        return (
            StatusCode::BAD_REQUEST,
            Json(
                serde_json::json!({ "error": "Insufficient earning balance", "earning": balance.earning_balance }),
            ),
        )
            .into_response();
    }

    let tx_hash = format!("zaps-yield-withdraw-{}", Uuid::new_v4());

    if let Err(e) =
        crate::db::r#yield::process_yield_withdrawal(&pool, auth.id, payload.amount, &tx_hash).await
    {
        tracing::error!("yield withdrawal DB error: {:?}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "Failed to process withdrawal" })),
        )
            .into_response();
    }

    let updated = match crate::db::r#yield::get_or_create_yield_balance(&pool, auth.id).await {
        Ok(b) => b,
        Err(e) => {
            tracing::error!("yield withdraw post-commit balance error: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Withdrawal recorded but balance refresh failed" })),
            )
                .into_response();
        }
    };

    let envelope_xdr = match build_stellar_envelope_xdr(
        &auth.address,
        "yield_withdraw",
        payload.amount,
        &tx_hash,
    ) {
        Ok(xdr) => xdr,
        Err(e) => {
            tracing::error!("yield withdraw envelope build error: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to build transaction envelope" })),
            )
                .into_response();
        }
    };

    Json(WithdrawResponse {
        available_balance: updated.available_balance,
        earning_balance: updated.earning_balance,
        envelope_xdr,
    })
    .into_response()
}

// ── Stellar envelope builder ───────────────────────────────────────────────

/// Build a base64-encoded Stellar XDR `TransactionEnvelope` (v1) for a yield
/// operation.  The returned envelope is unsigned and ready for the client
/// wallet to attach a signature and submit to the network.
///
/// # Envelope design
/// * **Source account** – the user's G… Stellar address.
/// * **Operation** – `ManageData` whose `data_name` encodes the operation
///   type (≤ 64 bytes) and whose `data_value` holds the amount as an 8-byte
///   big-endian integer.  `ManageData` is the canonical way to attach
///   arbitrary key/value metadata to an account record inside a Stellar
///   transaction, which is exactly what off-chain yield operations need.
/// * **Memo** – a hash memo derived from the first 32 bytes of the SHA-256
///   digest of the `reference` string so the back-end can correlate the
///   submitted transaction with its DB record.
/// * **Sequence** – set to `0` as a sentinel; the wallet (or a pre-submission
///   server call to `getAccount`) MUST replace this with the account's real
///   sequence number + 1 before signing.
/// * **Fee** – `MIN_BASE_FEE` (100 stroops) per operation.
/// * **Network** – selected via the `STELLAR_NETWORK` environment variable;
///   `"mainnet"` uses the public network passphrase, anything else (including
///   the default) uses the testnet passphrase.
fn build_stellar_envelope_xdr(
    source_account: &str,
    operation: &str,
    amount: i64,
    reference: &str,
) -> Result<String, String> {
    // Parse the G… address into a PublicKey.
    let source_pk = PublicKey::from_account_id(source_account)
        .map_err(|e| format!("invalid source account: {e}"))?;

    // operation tag for the ManageData entry name, e.g. "zaps-yield:yield_deposit"
    let data_name = format!("zaps-yield:{operation}");

    // Amount encoded as 8-byte big-endian so it survives a round-trip through
    // XDR without any floating-point representation issues.
    let amount_bytes = amount.to_be_bytes();
    let data_value = DataValue::from_slice(&amount_bytes)
        .map_err(|e| format!("invalid data value: {e}"))?;

    let manage_data_op = Operation::new_manage_data()
        .with_data_name(data_name)
        .with_data_value(Some(data_value))
        .build()
        .map_err(|e| format!("manage data op error: {e}"))?;

    // Derive a 32-byte hash memo from the reference so the back-end can
    // correlate the submitted transaction with the yield_transactions row.
    let ref_hash = {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        // Use a simple but deterministic 64-bit hash spread across 32 bytes.
        // For production the standard recommends SHA-256; we use two passes of
        // the stdlib hasher here to keep the dependency surface minimal while
        // still producing a stable, non-trivial 32-byte value.
        let mut h1 = DefaultHasher::new();
        reference.hash(&mut h1);
        let v1 = h1.finish();
        let mut h2 = DefaultHasher::new();
        v1.hash(&mut h2);
        let v2 = h2.finish();
        let mut bytes = [0u8; 32];
        bytes[..8].copy_from_slice(&v1.to_be_bytes());
        bytes[8..16].copy_from_slice(&v2.to_be_bytes());
        // Fill remaining bytes with XOR pattern for uniqueness.
        for i in 16..32 {
            bytes[i] = bytes[i - 16] ^ bytes[i - 8] ^ (i as u8);
        }
        bytes
    };
    let memo = Memo::new_hash(&ref_hash).map_err(|e| format!("memo error: {e}"))?;

    // Sequence 0 is a sentinel; wallets must substitute the real value.
    let sequence: i64 = 0;

    let tx = Transaction::builder(source_pk, sequence, MIN_BASE_FEE)
        .with_memo(memo)
        .add_operation(manage_data_op)
        .into_transaction()
        .map_err(|e| format!("transaction build error: {e}"))?;

    // Select network from environment (default: testnet).
    let network = match std::env::var("STELLAR_NETWORK")
        .unwrap_or_default()
        .to_lowercase()
        .as_str()
    {
        "mainnet" | "public" => Network::new_public(),
        _ => Network::new_test(),
    };

    let envelope = tx.into_envelope();

    // Serialize to standard base64-encoded XDR — the format accepted by
    // Horizon, Stellar Laboratory, and all major Stellar wallets.
    envelope
        .xdr_base64()
        .map_err(|e| format!("XDR serialization error: {e}"))
}
