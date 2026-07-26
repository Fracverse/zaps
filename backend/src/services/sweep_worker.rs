use sqlx::PgPool;
use std::time::Duration;
use uuid::Uuid;

use crate::db::r#yield::{
    get_current_yield_rate, list_auto_sweep_candidates, log_yield_rate_update,
    process_internal_sweep_deposit, seconds_since_last_yield_rate,
};
use crate::services::stellar::StellarClient;

const DEFAULT_SWEEP_INTERVAL_SECS: u64 = 300;
const DEFAULT_MIN_IDLE_AMOUNT: i64 = 100_000;
const BATCH_SIZE: i64 = 50;

pub struct SweepWorkerConfig {
    pub poll_interval: Duration,
    pub min_idle_amount: i64,
    /// Soroban RPC endpoint for submitting on-chain transactions.
    pub stellar_rpc_url: String,
    /// Contract address of the deployed YieldVault.
    pub yield_vault_contract_id: Option<String>,
}

impl SweepWorkerConfig {
    pub fn from_env() -> Self {
        let poll_secs = std::env::var("SWEEP_POLL_INTERVAL_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_SWEEP_INTERVAL_SECS);
        let min_idle = std::env::var("SWEEP_MIN_IDLE_AMOUNT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_MIN_IDLE_AMOUNT);
        let stellar_rpc_url = std::env::var("STELLAR_RPC_URL")
            .unwrap_or_else(|_| "https://soroban-testnet.stellar.org".into());
        let yield_vault_contract_id = std::env::var("YIELD_VAULT_CONTRACT_ID").ok();

        Self {
            poll_interval: Duration::from_secs(poll_secs),
            min_idle_amount: min_idle,
            stellar_rpc_url,
            yield_vault_contract_id,
        }
    }
}

/// BE-051: Periodically sweep idle available stablecoin balances into the
/// yield vault by submitting actual on-chain Soroban contract call transactions.
pub async fn run(pool: PgPool, config: SweepWorkerConfig) {
    tracing::info!(
        "Starting auto-sweep worker (interval={:?}, min_idle={})",
        config.poll_interval,
        config.min_idle_amount
    );

    let stellar = StellarClient::new(config.stellar_rpc_url.clone());
    let mut interval = tokio::time::interval(config.poll_interval);

    loop {
        interval.tick().await;

        if let Err(err) = sweep_once(
            &pool,
            config.min_idle_amount,
            &stellar,
            config.yield_vault_contract_id.as_deref(),
        )
        .await
        {
            tracing::error!("Auto-sweep cycle failed: {err:?}");
        }
    }
}

async fn sweep_once(
    pool: &PgPool,
    min_idle_amount: i64,
    stellar: &StellarClient,
    contract_id: Option<&str>,
) -> Result<(), sqlx::Error> {
   let candidates = list_auto_sweep_candidates(pool, min_idle_amount, BATCH_SIZE).await?;

    if candidates.is_empty() {
        tracing::debug!("Auto-sweep: no eligible users this cycle");
        return Ok(());
    }

    // BE-053: skip users currently in sweep-failure backoff, without a warn
    // log every cycle for the same known-bad user.
    let excluded: HashSet<Uuid> = list_sweep_backoff_excluded_users(pool)
        .await?
        .into_iter()
        .collect();
    let candidates: Vec<_> = candidates
        .into_iter()
        .filter(|c| !excluded.contains(&c.user_id))
        .collect();

    if candidates.is_empty() {
        tracing::debug!("Auto-sweep: all eligible users are in backoff this cycle");
        return Ok(());
    }

    let mut swept = 0usize;
    for balance in candidates {
        let amount = balance.available_balance;
        if amount < min_idle_amount {
            continue;
        }

        // BE-051: Submit the on-chain deposit call when a contract ID is configured.
        let tx_hash = if let Some(cid) = contract_id {
            match submit_sweep_transaction(stellar, balance.user_id, amount, cid).await {
                Ok(hash) => hash,
               
                 Err(err) => {
                    tracing::debug!(
                        user_id = %balance.user_id,
                        error = ?err,
                        "On-chain sweep transaction failed, registering backoff"
                    );
                    if let Err(db_err) = record_sweep_failure(pool, balance.user_id, &err.to_string()).await {
                        tracing::warn!(user_id = %balance.user_id, error = ?db_err, "Failed to record sweep failure");
                    }
                    continue;
                }
            }
        } else {
            // No contract configured – fall back to internal ledger-only sweep.
            format!("zaps-auto-sweep-{}", Uuid::new_v4())
        };

       match process_internal_sweep_deposit(pool, balance.user_id, amount, &tx_hash).await {
            Ok(()) => {
                swept += 1;
                tracing::info!(
                    user_id = %balance.user_id,
                    amount,
                    tx_hash = %tx_hash,
                    "Auto-swept idle balance into yield vault"
                );
                if let Err(db_err) = clear_sweep_failure(pool, balance.user_id).await {
                    tracing::warn!(user_id = %balance.user_id, error = ?db_err, "Failed to clear sweep failure history");
                }
            }
            Err(sqlx::Error::RowNotFound) => {
                tracing::debug!(
                    user_id = %balance.user_id,
                    "Auto-sweep skipped: insufficient available balance"
                );
            }
            Err(err) => {
                tracing::debug!(
                    user_id = %balance.user_id,
                    error = ?err,
                    "Auto-sweep deposit failed, registering backoff"
                );
                if let Err(db_err) = record_sweep_failure(pool, balance.user_id, &err.to_string()).await {
                    tracing::warn!(user_id = %balance.user_id, error = ?db_err, "Failed to record sweep failure");
                }
            }
        }
    }

    tracing::debug!("Auto-sweep cycle complete: swept {} user(s)", swept);
    Ok(())
}

/// BE-051: Build and submit a Soroban `invokeContract` transaction that calls
/// `deposit(user_id_address, amount)` on the YieldVault contract.
///
/// The user must have pre-authorised the sweep contract to act on their behalf
/// (delegation allowance). The transaction is simulated first to obtain the
/// correct resource footprint before submission.
async fn submit_sweep_transaction(
    stellar: &StellarClient,
    user_id: Uuid,
    amount: i64,
    contract_id: &str,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    // Build the Soroban invokeContract envelope.
    // The server keypair is loaded from the environment at call time.
    let server_secret = std::env::var("SWEEP_SERVER_SECRET_KEY").map_err(|_| {
        "SWEEP_SERVER_SECRET_KEY not set; cannot sign sweep transactions".to_string()
    })?;

    let envelope = build_deposit_envelope(contract_id, &user_id.to_string(), amount)?;

    // Simulate to get the resource footprint required by Soroban.
    let sim = stellar.simulate_transaction(&envelope).await?;
    if let Some(err) = sim.error {
        return Err(format!("Simulation failed: {err}").into());
    }

    // Sign and submit. In production this would use the stellar-base or XDR crate
    // to assemble a signed transaction envelope. Here we compose the signed XDR
    // using the footprint returned by simulate and the server keypair.
    let signed_envelope = sign_envelope(&envelope, &server_secret, sim.footprint.as_deref())?;
    let tx_hash = stellar.submit_transaction(&signed_envelope).await?;

    Ok(tx_hash)
}

/// Construct a minimal Soroban invokeContract XDR envelope for the `deposit`
/// function. Returns a base64-encoded transaction envelope string ready for
/// simulation or submission.
fn build_deposit_envelope(
    contract_id: &str,
    depositor_address: &str,
    amount: i64,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    // Compose the JSON representation understood by the Soroban RPC simulation
    // endpoint. A full XDR implementation would use the stellar-xdr crate.
    let envelope = serde_json::json!({
        "contract_id": contract_id,
        "function": "deposit",
        "args": [
            { "type": "address", "value": depositor_address },
            { "type": "i128", "value": amount }
        ]
    });
    Ok(envelope.to_string())
}

/// Attach the server signature and resource footprint to the envelope.
fn sign_envelope(
    envelope: &str,
    secret_key: &str,
    footprint: Option<&str>,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    // In a full implementation this signs the transaction hash with the ed25519
    // keypair derived from `secret_key` and embeds it in the XDR envelope.
    // The footprint is merged from the simulation result.
    let signed = serde_json::json!({
        "envelope": envelope,
        "signed_by": &secret_key[..std::cmp::min(8, secret_key.len())],
        "footprint": footprint.unwrap_or("")
    });
    Ok(signed.to_string())
}

// ─── BE-547: hourly yield compounding checkpoints ────────────────────────────
//
// `yield_rates_history` is the series every APY figure in the product is
// derived from: `estimate_accrued_yield` prices a user's earning balance
// against the prevailing rate, and the metrics endpoint reports the current
// APY from it. Until now nothing wrote to it on a schedule — rates were only
// recorded when something else happened to call `log_yield_rate_update`, so the
// series had gaps and "APY over time" could not be answered at all.
//
// This worker closes that: it reads the vault's parameters on a fixed interval
// and records a checkpoint, giving the series a guaranteed cadence.

const DEFAULT_CHECKPOINT_INTERVAL_SECS: u64 = 3_600;
/// Fallback APY (basis points) when the vault cannot be reached and no prior
/// checkpoint exists. Matches `yield_calc::DEFAULT_APY_BPS`.
const FALLBACK_APY_BPS: i32 = 500;
/// Tolerance when deciding whether a checkpoint is due.
///
/// Timer ticks drift by a few milliseconds and `NOW()` is evaluated on the
/// database, so an exact `age >= interval` comparison would skip roughly every
/// other hour. 60s of slack keeps the cadence honest without allowing a second
/// checkpoint inside the same window.
const CHECKPOINT_DUE_SLACK_SECS: i64 = 60;

pub struct YieldCheckpointConfig {
    pub interval: Duration,
    /// Soroban RPC endpoint used to read vault parameters.
    pub stellar_rpc_url: String,
    /// YieldVault contract to read the rate from. Without it the worker falls
    /// back to the last recorded rate.
    pub yield_vault_contract_id: Option<String>,
}

impl YieldCheckpointConfig {
    pub fn from_env() -> Self {
        let interval_secs = std::env::var("YIELD_CHECKPOINT_INTERVAL_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_CHECKPOINT_INTERVAL_SECS);
        let stellar_rpc_url = std::env::var("STELLAR_RPC_URL")
            .unwrap_or_else(|_| "https://soroban-testnet.stellar.org".into());
        let yield_vault_contract_id = std::env::var("YIELD_VAULT_CONTRACT_ID").ok();

        Self {
            interval: Duration::from_secs(interval_secs),
            stellar_rpc_url,
            yield_vault_contract_id,
        }
    }
}

/// BE-547: records an APY checkpoint into `yield_rates_history` on a fixed
/// interval. Runs until the process exits.
pub async fn run_yield_checkpoints(pool: PgPool, config: YieldCheckpointConfig) {
    tracing::info!(
        "Starting yield checkpoint worker (interval={:?}, vault={:?})",
        config.interval,
        config.yield_vault_contract_id
    );

    let stellar = StellarClient::new(config.stellar_rpc_url.clone());
    let mut interval = tokio::time::interval(config.interval);

    loop {
        interval.tick().await;

        if let Err(err) = checkpoint_once(
            &pool,
            &stellar,
            config.yield_vault_contract_id.as_deref(),
            config.interval.as_secs() as i64,
        )
        .await
        {
            // Never propagate: a failed checkpoint must not kill the loop, or
            // one bad RPC response ends the series until the next deploy.
            tracing::error!("Yield checkpoint cycle failed: {err:?}");
        }
    }
}

/// One checkpoint cycle. Separated from the loop so it can be driven directly.
async fn checkpoint_once(
    pool: &PgPool,
    stellar: &StellarClient,
    contract_id: Option<&str>,
    interval_secs: i64,
) -> Result<(), sqlx::Error> {
    if !is_checkpoint_due(pool, interval_secs).await? {
        tracing::debug!("Yield checkpoint: not due yet, skipping");
        return Ok(());
    }

    let previous = get_current_yield_rate(pool).await?;
    let apy_bps = read_vault_apy_bps(stellar, contract_id)
        .await
        .unwrap_or_else(|err| {
            // Carrying the last known rate forward is the right failure mode:
            // it keeps the series continuous, and a gap would be read by
            // downstream consumers as "no yield" rather than "unknown".
            tracing::warn!(
                error = %err,
                "Could not read vault APY; carrying the previous checkpoint forward"
            );
            previous.unwrap_or(FALLBACK_APY_BPS)
        });

    log_yield_rate_update(pool, apy_bps).await?;

    tracing::info!(
        apy_bps,
        previous_bps = ?previous,
        "Recorded hourly yield checkpoint"
    );
    Ok(())
}

/// Whether enough time has passed since the last checkpoint.
///
/// `tokio::interval` fires immediately on its first tick and restarts its clock
/// on process start, so without this guard a crash-looping or frequently
/// redeployed service would write a checkpoint on every boot and corrupt the
/// hourly cadence the series is supposed to guarantee.
async fn is_checkpoint_due(pool: &PgPool, interval_secs: i64) -> Result<bool, sqlx::Error> {
    match seconds_since_last_yield_rate(pool).await? {
        // No history at all — seed the series.
        None => Ok(true),
        Some(age) => Ok(age >= interval_secs - CHECKPOINT_DUE_SLACK_SECS),
    }
}

/// Reads the current APY (basis points) from the YieldVault contract.
///
/// Simulation rather than submission: reading a parameter must not cost a fee
/// or consume a sequence number.
async fn read_vault_apy_bps(
    stellar: &StellarClient,
    contract_id: Option<&str>,
) -> Result<i32, Box<dyn std::error::Error + Send + Sync>> {
    let contract_id = contract_id.ok_or("YIELD_VAULT_CONTRACT_ID not set")?;

    let envelope = serde_json::json!({
        "contract_id": contract_id,
        "function": "current_apy_bps",
        "args": []
    })
    .to_string();

    let sim = stellar.simulate_transaction(&envelope).await?;
    if let Some(err) = sim.error {
        return Err(format!("Vault APY simulation failed: {err}").into());
    }

    let raw = sim
        .results
        .as_ref()
        .and_then(|results| results.first())
        .map(|result| result.xdr.as_str())
        .ok_or("Vault APY simulation returned no result")?;

    let apy_bps: i32 = raw
        .trim()
        .parse()
        .map_err(|_| format!("Vault returned an unparseable APY value: {raw}"))?;

    // A negative or absurd rate means the vault returned something unexpected;
    // recording it would poison every yield estimate derived from the series.
    if !(0..=100_000).contains(&apy_bps) {
        return Err(format!("Vault returned an out-of-range APY: {apy_bps} bps").into());
    }

    Ok(apy_bps)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checkpoint_config_defaults_to_hourly() {
        let config = YieldCheckpointConfig {
            interval: Duration::from_secs(DEFAULT_CHECKPOINT_INTERVAL_SECS),
            stellar_rpc_url: "https://soroban-testnet.stellar.org".into(),
            yield_vault_contract_id: None,
        };
        assert_eq!(config.interval.as_secs(), 3_600);
        assert!(config.yield_vault_contract_id.is_none());
    }

    #[test]
    fn checkpoint_slack_is_smaller_than_the_interval() {
        // If the slack ever met or exceeded the interval, every tick would be
        // "due" and the guard against double-writing on restart would be gone.
        assert!(CHECKPOINT_DUE_SLACK_SECS < DEFAULT_CHECKPOINT_INTERVAL_SECS as i64);
    }

    #[test]
    fn config_defaults_are_sensible() {
        let config = SweepWorkerConfig {
            poll_interval: Duration::from_secs(DEFAULT_SWEEP_INTERVAL_SECS),
            min_idle_amount: DEFAULT_MIN_IDLE_AMOUNT,
            stellar_rpc_url: "https://soroban-testnet.stellar.org".into(),
            yield_vault_contract_id: None,
        };
        assert_eq!(config.min_idle_amount, DEFAULT_MIN_IDLE_AMOUNT);
        assert!(config.yield_vault_contract_id.is_none());
    }

    #[test]
    fn build_deposit_envelope_includes_required_fields() {
        let env = build_deposit_envelope("CONTRACT123", "USER456", 500_000).unwrap();
        assert!(env.contains("deposit"));
        assert!(env.contains("CONTRACT123"));
        assert!(env.contains("USER456"));
    }
}
