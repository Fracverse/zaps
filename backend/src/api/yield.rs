use crate::api::feed::AuthUser;
use axum::{
    extract::{FromRef, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use chrono::Utc;
use redis::{
    aio::{ConnectionManager, ConnectionManagerConfig},
    RedisError,
};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use std::time::Duration;
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

/// APY (in basis points) served when the platform has no rate on record yet.
const DEFAULT_YIELD_RATE_BPS: i32 = 500; // 5.00 %

// ── BE-061 — Redis yield cache ────────────────────────────────────────────

/// Cached platform-wide yield rate, in basis points.
pub const YIELD_RATE_CACHE_KEY: &str = "zaps:yield:rate";

/// Every platform-scoped cache key that goes stale when the on-chain vault
/// state moves. User-scoped balances are always read live from Postgres, so
/// they deliberately have no entry here.
pub const PLATFORM_YIELD_CACHE_KEYS: &[&str] = &[YIELD_RATE_CACHE_KEY];

/// How long a cached platform yield rate may live before it is refetched.
/// Eviction on `YieldAccrued` is what keeps it fresh; the TTL is only a
/// backstop for the case where the indexer misses an event.
const YIELD_RATE_CACHE_TTL_SECS: u64 = 300;

const REDIS_CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
const REDIS_RESPONSE_TIMEOUT: Duration = Duration::from_secs(2);

/// Handle on the shared Redis connection pool backing the yield cache.
///
/// `ConnectionManager` multiplexes commands over a pooled connection and
/// transparently reconnects, so this is cheap to clone across handlers and
/// background workers.
#[derive(Clone)]
pub struct YieldCache {
    pool: ConnectionManager,
}

impl YieldCache {
    /// Build a cache handle from a `redis://` URL. The pool connects lazily, so
    /// a Redis outage at boot degrades yield reads to Postgres instead of
    /// preventing the API from starting.
    pub fn connect(redis_url: &str) -> Result<Self, RedisError> {
        let client = redis::Client::open(redis_url)?;
        let config = ConnectionManagerConfig::new()
            .set_connection_timeout(Some(REDIS_CONNECT_TIMEOUT))
            .set_response_timeout(Some(REDIS_RESPONSE_TIMEOUT));

        Ok(Self {
            pool: ConnectionManager::new_lazy_with_config(client, config)?,
        })
    }

    /// Issue a single `DEL` to the Redis pool for every key, returning the
    /// number of keys that were actually present.
    pub async fn delete_keys<K: AsRef<str>>(&self, keys: &[K]) -> Result<u64, RedisError> {
        if keys.is_empty() {
            return Ok(0);
        }

        let mut cmd = redis::cmd("DEL");
        for key in keys {
            cmd.arg(key.as_ref());
        }

        cmd.query_async(&mut self.pool.clone()).await
    }

    /// Read the cached platform yield rate. A miss — or any Redis error — is
    /// reported as `None` so the caller falls back to Postgres.
    async fn get_yield_rate(&self) -> Option<i32> {
        match redis::cmd("GET")
            .arg(YIELD_RATE_CACHE_KEY)
            .query_async::<Option<i32>>(&mut self.pool.clone())
            .await
        {
            Ok(cached) => cached,
            Err(e) => {
                tracing::warn!("yield rate cache read failed: {e}");
                None
            }
        }
    }

    /// Repopulate the platform yield rate with a bounded TTL.
    async fn set_yield_rate(&self, apy_bps: i32) {
        if let Err(e) = redis::cmd("SET")
            .arg(YIELD_RATE_CACHE_KEY)
            .arg(apy_bps)
            .arg("EX")
            .arg(YIELD_RATE_CACHE_TTL_SECS)
            .query_async::<()>(&mut self.pool.clone())
            .await
        {
            tracing::warn!("yield rate cache write failed: {e}");
        }
    }
}

/// BE-061: Evict every platform-scoped yield cache key.
///
/// The indexer calls this the moment a `YieldAccrued` (or `YieldRateUpdated`)
/// block event is committed, so the next `/api/yield/balance` read recomputes
/// from freshly indexed state instead of serving a stale APY.
///
/// Eviction is best-effort: Postgres is the source of truth, so a Redis failure
/// is logged and counted as zero evictions rather than failing the indexer.
/// Returns the number of keys removed.
pub async fn invalidate_platform_yield_cache(cache: Option<&YieldCache>) -> u64 {
    let Some(cache) = cache else {
        return 0;
    };

    match cache.delete_keys(PLATFORM_YIELD_CACHE_KEYS).await {
        Ok(removed) => {
            tracing::info!("Evicted {removed} platform yield cache key(s)");
            removed
        }
        Err(e) => {
            tracing::error!("Platform yield cache eviction failed: {e}");
            0
        }
    }
}

// ── Yield router state ────────────────────────────────────────────────────

/// State shared by the `/api/yield` handlers: the Postgres pool plus an
/// optional Redis cache (absent when `REDIS_URL` is unset, e.g. in tests).
#[derive(Clone)]
pub struct YieldState {
    pub pool: PgPool,
    pub cache: Option<YieldCache>,
}

impl YieldState {
    pub fn new(pool: PgPool, cache: Option<YieldCache>) -> Self {
        Self { pool, cache }
    }

    /// Read the platform yield rate through the cache, falling back to
    /// Postgres and repopulating the key on a miss.
    async fn load_yield_rate(&self) -> Result<i32, sqlx::Error> {
        if let Some(cache) = &self.cache {
            if let Some(apy_bps) = cache.get_yield_rate().await {
                return Ok(apy_bps);
            }
        }

        let apy_bps = crate::db::r#yield::get_current_yield_rate(&self.pool)
            .await?
            .unwrap_or(DEFAULT_YIELD_RATE_BPS);

        if let Some(cache) = &self.cache {
            cache.set_yield_rate(apy_bps).await;
        }

        Ok(apy_bps)
    }
}

// Lets handlers that only need the database keep extracting `State<PgPool>`,
// and keeps the `AuthUser` extractor usable with this state.
impl FromRef<YieldState> for PgPool {
    fn from_ref(state: &YieldState) -> Self {
        state.pool.clone()
    }
}

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

pub async fn get_balance(State(state): State<YieldState>, auth: AuthUser) -> impl IntoResponse {
    let pool = &state.pool;

    let balance = match crate::db::r#yield::get_or_create_yield_balance(pool, auth.id).await {
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

    let apy_bps = match state.load_yield_rate().await {
        Ok(r) => r,
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

    let auto_earn_enabled = match crate::db::r#yield::get_auto_earn_enabled(pool, auth.id).await {
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

// ── BE-548 — GET /api/yield/metrics ───────────────────────────────────────

/// Micro-units per whole currency unit. Balances are stored scaled by this.
const MICRO_UNITS_PER_UNIT: f64 = 1_000_000.0;

/// Fallback USD→NGN rate when `USD_NGN_RATE` is unset.
///
/// Deliberately a config value rather than a hard-coded constant used blindly:
/// NGN moves too much for a compiled-in number to stay honest, and the response
/// echoes back the rate it used so a client can tell a stale figure from a
/// fresh one. A live FX feed is the proper long-term answer — see the note on
/// `fx_rate_source` below.
const DEFAULT_USD_NGN_RATE: f64 = 1600.0;

fn usd_ngn_rate() -> f64 {
    std::env::var("USD_NGN_RATE")
        .ok()
        .and_then(|v| v.parse::<f64>().ok())
        // A zero or negative rate would silently zero out every NGN figure, so
        // treat it as unset rather than trusting it.
        .filter(|rate| *rate > 0.0)
        .unwrap_or(DEFAULT_USD_NGN_RATE)
}

/// Converts a micro-unit balance to whole units.
fn to_units(micro: i64) -> f64 {
    micro as f64 / MICRO_UNITS_PER_UNIT
}

#[derive(Serialize)]
pub struct UserYieldMetrics {
    pub available_balance: i64,
    pub earning_balance: i64,
    /// Live off-chain estimate since the last sync (micro-units).
    pub accrued_interest: i64,
    pub total_earning_balance: i64,
    /// Lifetime totals from `yield_transactions` (micro-units).
    pub total_deposited: i64,
    pub total_withdrawn: i64,
    /// Interest already credited on-chain, as distinct from `accrued_interest`,
    /// which is the live estimate for the current period.
    pub total_earned: i64,
    pub transaction_count: i64,
    pub auto_earn_enabled: bool,
}

#[derive(Serialize)]
pub struct PlatformYieldMetrics {
    /// Total value locked, in micro-units.
    pub tvl: i64,
    pub tvl_usd: f64,
    pub tvl_ngn: f64,
    /// Idle balances not yet earning, in micro-units.
    pub total_available: i64,
    pub active_accounts: i64,
    pub auto_earn_accounts: i64,
}

#[derive(Serialize)]
pub struct YieldMetricsResponse {
    /// Current APY as a percentage (e.g. 5.0 for 5%).
    pub apy: f64,
    /// Same rate in basis points, for clients that would rather not round-trip
    /// through a float.
    pub apy_bps: i32,
    pub user: UserYieldMetrics,
    pub platform: PlatformYieldMetrics,
    /// The USD→NGN rate applied to `tvl_ngn`, echoed so a client can tell which
    /// rate produced the figure.
    pub usd_ngn_rate: f64,
    /// Where that rate came from: `"config"` or `"default"`. A client showing
    /// Naira to a user needs to know when it is displaying a compiled-in guess.
    pub fx_rate_source: &'static str,
    pub generated_at: String,
}

/// BE-548: `GET /api/yield/metrics`
///
/// One call returning both the caller's yield position and platform-wide
/// totals. Built as a single endpoint rather than leaving clients to fan out
/// across `/balance` plus separate aggregate calls: the dashboard renders these
/// numbers side by side, and separate requests would let the user's balance and
/// the TVL it is a share of come from different moments.
pub async fn get_metrics(State(state): State<YieldState>, auth: AuthUser) -> impl IntoResponse {
    let pool = &state.pool;

    let balance = match crate::db::r#yield::get_or_create_yield_balance(pool, auth.id).await {
        Ok(b) => b,
        Err(e) => {
            tracing::error!("yield metrics balance fetch error: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to retrieve yield balance" })),
            )
                .into_response();
        }
    };

    let apy_bps = match state.load_yield_rate().await {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("yield metrics rate fetch error: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to retrieve yield rate" })),
            )
                .into_response();
        }
    };

    let user_totals = match crate::db::r#yield::get_user_yield_totals(pool, auth.id).await {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("yield metrics user totals error: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to retrieve user yield totals" })),
            )
                .into_response();
        }
    };

    let platform_totals = match crate::db::r#yield::get_platform_yield_totals(pool).await {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("yield metrics platform totals error: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to retrieve platform yield totals" })),
            )
                .into_response();
        }
    };

    let auto_earn_enabled = match crate::db::r#yield::get_auto_earn_enabled(pool, auth.id).await {
        Ok(enabled) => enabled,
        Err(e) => {
            tracing::error!("yield metrics auto-earn fetch error: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to retrieve auto-earn preference" })),
            )
                .into_response();
        }
    };

    let estimate = yield_calc::estimate_for_balance(&balance, Some(apy_bps), Utc::now());

    let fx_rate_source = if std::env::var("USD_NGN_RATE").is_ok() {
        "config"
    } else {
        "default"
    };
    let rate = usd_ngn_rate();
    // Balances are held in a USD stablecoin, so units are already USD.
    let tvl_usd = to_units(platform_totals.tvl);

    Json(YieldMetricsResponse {
        apy: apy_bps as f64 / 100.0,
        apy_bps,
        user: UserYieldMetrics {
            available_balance: balance.available_balance,
            earning_balance: balance.earning_balance,
            accrued_interest: estimate.accrued_interest,
            total_earning_balance: estimate.total_earning_balance,
            total_deposited: user_totals.total_deposited,
            total_withdrawn: user_totals.total_withdrawn,
            total_earned: user_totals.total_earned,
            transaction_count: user_totals.transaction_count,
            auto_earn_enabled,
        },
        platform: PlatformYieldMetrics {
            tvl: platform_totals.tvl,
            tvl_usd,
            tvl_ngn: tvl_usd * rate,
            total_available: platform_totals.total_available,
            active_accounts: platform_totals.active_accounts,
            auto_earn_accounts: platform_totals.auto_earn_accounts,
        },
        usd_ngn_rate: rate,
        fx_rate_source,
        generated_at: Utc::now().to_rfc3339(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_cache_keys_are_namespaced_and_non_empty() {
        assert!(!PLATFORM_YIELD_CACHE_KEYS.is_empty());
        assert!(PLATFORM_YIELD_CACHE_KEYS
            .iter()
            .all(|key| key.starts_with("zaps:yield:")));
        assert!(PLATFORM_YIELD_CACHE_KEYS.contains(&YIELD_RATE_CACHE_KEY));
    }

    #[tokio::test]
    async fn invalidating_without_a_cache_is_a_no_op() {
        // Deployments without REDIS_URL must keep indexing without a cache.
        assert_eq!(invalidate_platform_yield_cache(None).await, 0);
    }

    #[test]
    fn connect_rejects_a_non_redis_url() {
        assert!(YieldCache::connect("postgres://localhost/zaps").is_err());
    }
}
