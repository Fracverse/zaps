use crate::api::feed::AuthUser;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::Row;
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

    let auto_earn_enabled = match crate::db::r#yield::get_auto_earn_enabled(&pool, auth.id).await {
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

/// Friendly categories for yield transactions logged in history.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum YieldTransactionType {
    Deposit,
    Withdraw,
    Earned,
    Sweep,
    Reward,
}

impl YieldTransactionType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Deposit => "DEPOSIT",
            Self::Withdraw => "WITHDRAW",
            Self::Earned => "EARNED",
            Self::Sweep => "SWEEP",
            Self::Reward => "REWARD",
        }
    }
}

impl std::fmt::Display for YieldTransactionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for YieldTransactionType {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.to_uppercase().as_str() {
            "DEPOSIT" => Self::Deposit,
            "WITHDRAW" => Self::Withdraw,
            "EARNED" => Self::Earned,
            "SWEEP" | "AUTO_SWEEP" => Self::Sweep,
            "REWARD" | "YIELD_REWARD" => Self::Reward,
            _ => Self::Deposit,
        })
    }
}

impl From<&str> for YieldTransactionType {
    fn from(s: &str) -> Self {
        s.parse().unwrap_or(Self::Deposit)
    }
}

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
    pub tx_type: YieldTransactionType,
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
            let raw_type: String = r.get("type");
            YieldHistoryItem {
                id: id.to_string(),
                tx_hash: r.get("tx_hash"),
                tx_type: raw_type.as_str().into(),
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
    let envelope_xdr =
        build_stellar_envelope_xdr(&auth.address, "yield_deposit", payload.amount, &tx_hash);

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

    let envelope_xdr =
        build_stellar_envelope_xdr(&auth.address, "yield_withdraw", payload.amount, &tx_hash);

    Json(WithdrawResponse {
        available_balance: updated.available_balance,
        earning_balance: updated.earning_balance,
        envelope_xdr,
    })
    .into_response()
}

// ── Stellar envelope builder ───────────────────────────────────────────────

/// Build a base64-encoded Stellar XDR transaction envelope describing the
/// yield operation. The client wallet signs and submits this to the network.
fn build_stellar_envelope_xdr(
    source_account: &str,
    operation: &str,
    amount: i64,
    reference: &str,
) -> String {
    // Construct a JSON representation of the operation parameters.
    // In production this would be a proper XDR-encoded TransactionEnvelope
    // built via the Stellar SDK.
    let payload = serde_json::json!({
        "source_account": source_account,
        "operation": operation,
        "amount": amount,
        "reference": reference,
        "network": "Stellar Mainnet",
        "memo": format!("Zaps Yield: {}", reference),
    });
    BASE64.encode(payload.to_string().as_bytes())
}
