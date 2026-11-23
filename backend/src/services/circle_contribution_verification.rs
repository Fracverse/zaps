//! #1207 — Contribution verification.
//!
//! Confirms that a submitted contribution really happened on Stellar. The
//! transaction is read from Horizon, reduced to the value transfers it made
//! (classic `payment` operations and the `transfer` balance changes of
//! `invoke_host_function` operations), and compared with what the
//! contribution pinned at initiation: payer, destination, asset, exact
//! amount and the reference memo.
//!
//! * Only an explicit match turns a contribution `confirmed`, and only then
//!   are the circle and member totals credited (in one transaction, under a
//!   row lock, so repeating the call can never credit twice).
//! * A transaction that does not match is rejected without touching the
//!   contribution (`mismatch`): anyone's hash can be wrong, and a wrong hash
//!   must not be able to close someone else's contribution. Only when the
//!   transaction already on file shows as failed on the network does the
//!   contribution become `failed`; it never counts.
//! * Only the contributing member can submit or replace the transaction hash.
//!   Owners / admins can re-check the hash on file.
//! * A transaction Horizon cannot find (yet), or an unreachable Horizon,
//!   leaves the contribution `submitted`: unconfirmed, not successful.
//! * A transaction hash backs at most one live contribution.
//! * Every attempt is persisted in `circle_contribution_verifications`.
//!
//! Configuration (fails closed): verification needs both `STELLAR_NETWORK`
//! (`mainnet` | `public` | `testnet`, the variable the rest of the backend
//! uses) and `STELLAR_HORIZON_URL` (https, or http on localhost). If either is
//! missing or invalid, every verification returns `unavailable` and nothing is
//! ever confirmed. The Horizon server's network passphrase is also checked
//! against `STELLAR_NETWORK`, so a mainnet deployment cannot be verified
//! against a testnet Horizon (or the other way round).

use crate::services::circle_contributions::{fetch, Contribution};
use crate::services::circle_membership::{self, CirclePermission};
use serde::Serialize;
use serde_json::Value;
use sqlx::PgPool;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use uuid::Uuid;

const MAINNET_PASSPHRASE: &str = "Public Global Stellar Network ; September 2015";
const TESTNET_PASSPHRASE: &str = "Test SDF Network ; September 2015";
/// Reasons shown to clients. Details of a failed lookup (which can name the
/// Horizon URL or internal errors) are only logged.
const NOT_CONFIGURED_REASON: &str = "verification_not_configured";
const UNAVAILABLE_REASON: &str = "horizon_unavailable";
const STROOPS_PER_UNIT: i64 = 10_000_000;

// ── Chain model ───────────────────────────────────────────────────────────────

/// One movement of value inside a transaction.
#[derive(Debug, Clone, PartialEq)]
pub struct Transfer {
    pub from: Option<String>,
    pub to: String,
    /// `native` or `CODE:ISSUER`.
    pub asset: String,
    /// Stroops.
    pub amount: i64,
}

/// The parts of a Stellar transaction verification needs.
#[derive(Debug, Clone, PartialEq)]
pub struct ChainTx {
    pub hash: String,
    pub successful: bool,
    pub ledger: Option<i64>,
    pub memo_type: Option<String>,
    pub memo: Option<String>,
    pub source_account: Option<String>,
    pub transfers: Vec<Transfer>,
}

/// What a contribution pinned at initiation.
#[derive(Debug, Clone, Copy)]
pub struct Expected<'a> {
    pub reference: &'a str,
    pub destination: &'a str,
    pub asset: &'a str,
    pub amount: i64,
    pub payer: &'a str,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    Confirmed,
    /// The transaction exists but is not a valid payment for the contribution.
    Failed(&'static str),
    /// The network does not know the transaction (yet).
    NotFound,
    /// The network could not be queried. Carries a generic reason code.
    Unavailable(&'static str),
}

impl Outcome {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Confirmed => "confirmed",
            Self::Failed(_) => "failed",
            Self::NotFound => "not_found",
            Self::Unavailable(_) => "unavailable",
        }
    }

    pub fn reason(&self) -> Option<String> {
        match self {
            Self::Failed(reason) | Self::Unavailable(reason) => Some((*reason).to_string()),
            _ => None,
        }
    }
}

// ── Pure logic ────────────────────────────────────────────────────────────────

/// 64 hex characters, lower-cased.
pub fn normalize_tx_hash(raw: &str) -> Option<String> {
    let raw = raw.trim();
    (raw.len() == 64 && raw.bytes().all(|b| b.is_ascii_hexdigit()))
        .then(|| raw.to_ascii_lowercase())
}

/// Horizon amount (`"12.3400000"`) to stroops, without floating point.
pub fn parse_stroops(raw: &str) -> Option<i64> {
    let (whole, frac) = raw.trim().split_once('.').unwrap_or((raw.trim(), ""));
    if whole.is_empty()
        || frac.len() > 7
        || !whole.bytes().all(|b| b.is_ascii_digit())
        || !frac.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let frac = format!("{frac:0<7}");
    whole
        .parse::<i64>()
        .ok()?
        .checked_mul(STROOPS_PER_UNIT)?
        .checked_add(frac.parse::<i64>().ok()?)
}

fn asset_id(record: &Value) -> Option<String> {
    match record.get("asset_type")?.as_str()? {
        "native" => Some("native".to_string()),
        "credit_alphanum4" | "credit_alphanum12" => Some(format!(
            "{}:{}",
            record.get("asset_code")?.as_str()?,
            record.get("asset_issuer")?.as_str()?
        )),
        _ => None,
    }
}

fn transfer_of(record: &Value) -> Result<Option<Transfer>, String> {
    let Some(asset) = asset_id(record) else {
        return Ok(None);
    };
    let to = record.get("to").and_then(Value::as_str);
    let amount = record.get("amount").and_then(Value::as_str);
    let (Some(to), Some(amount)) = (to, amount) else {
        return Ok(None);
    };
    Ok(Some(Transfer {
        from: record.get("from").and_then(Value::as_str).map(String::from),
        to: to.to_string(),
        asset,
        amount: parse_stroops(amount).ok_or_else(|| format!("unreadable amount {amount:?}"))?,
    }))
}

/// Build a [`ChainTx`] from Horizon's `/transactions/{hash}` and
/// `/transactions/{hash}/operations` documents. Operations that move no
/// attributable value are ignored.
pub fn parse_horizon(tx: &Value, operations: &Value) -> Result<ChainTx, String> {
    let successful = tx
        .get("successful")
        .and_then(Value::as_bool)
        .ok_or("transaction has no `successful` flag")?;
    let hash = tx
        .get("hash")
        .and_then(Value::as_str)
        .ok_or("transaction has no hash")?
        .to_ascii_lowercase();
    let text = |key: &str| tx.get(key).and_then(Value::as_str).map(String::from);

    let records = operations
        .pointer("/_embedded/records")
        .and_then(Value::as_array)
        .ok_or("operations response has no records")?;
    let mut transfers = Vec::new();
    for record in records {
        match record.get("type").and_then(Value::as_str) {
            Some("payment") => {
                if let Some(transfer) = transfer_of(record)? {
                    transfers.push(transfer);
                }
            }
            Some("invoke_host_function") => {
                let changes = record
                    .get("asset_balance_changes")
                    .and_then(Value::as_array)
                    .map(Vec::as_slice)
                    .unwrap_or_default();
                for change in changes {
                    if change.get("type").and_then(Value::as_str) == Some("transfer") {
                        if let Some(transfer) = transfer_of(change)? {
                            transfers.push(transfer);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    Ok(ChainTx {
        hash,
        successful,
        ledger: tx.get("ledger").and_then(Value::as_i64),
        memo_type: text("memo_type"),
        memo: text("memo"),
        source_account: text("source_account"),
        transfers,
    })
}

/// Decide whether `tx` is the payment `expected` describes. Returns
/// [`Outcome::Confirmed`] or [`Outcome::Failed`] with the first mismatch.
pub fn match_contribution(expected: &Expected, tx: &ChainTx) -> Outcome {
    if !tx.successful {
        return Outcome::Failed("transaction_failed");
    }
    if tx.memo_type.as_deref() != Some("text") || tx.memo.as_deref() != Some(expected.reference) {
        return Outcome::Failed("memo_mismatch");
    }
    let to_destination: Vec<&Transfer> = tx
        .transfers
        .iter()
        .filter(|t| t.to == expected.destination)
        .collect();
    if to_destination.is_empty() {
        return Outcome::Failed("destination_mismatch");
    }
    let in_asset: Vec<&Transfer> = to_destination
        .into_iter()
        .filter(|t| t.asset == expected.asset)
        .collect();
    if in_asset.is_empty() {
        return Outcome::Failed("asset_mismatch");
    }
    let from_payer: Vec<&Transfer> = in_asset
        .into_iter()
        .filter(|t| t.from.as_deref().or(tx.source_account.as_deref()) == Some(expected.payer))
        .collect();
    if from_payer.is_empty() {
        return Outcome::Failed("payer_mismatch");
    }
    let paid = from_payer
        .iter()
        .try_fold(0i64, |sum, t| sum.checked_add(t.amount));
    if paid == Some(expected.amount) {
        Outcome::Confirmed
    } else {
        Outcome::Failed("amount_mismatch")
    }
}

/// Where the transaction hash being checked came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HashSource {
    /// Already stored on the contribution.
    OnFile,
    /// Supplied with this request.
    Supplied,
}

/// Status a contribution moves to after a verification attempt. `None` means
/// the contribution is closed (`failed` / `expired`) and cannot be verified.
pub fn next_status(current: &str, source: HashSource, outcome: &Outcome) -> Option<&'static str> {
    match (current, outcome) {
        ("confirmed", _) => Some("confirmed"),
        ("pending" | "submitted", Outcome::Confirmed) => Some("confirmed"),
        // The transaction on file itself failed on the network: closed.
        ("pending" | "submitted", Outcome::Failed("transaction_failed"))
            if source == HashSource::OnFile =>
        {
            Some("failed")
        }
        // Any other mismatch (or a supplied hash that is not a valid payment)
        // is rejected; the contribution stays as it was.
        ("pending", Outcome::Failed(_)) => Some("pending"),
        ("submitted", Outcome::Failed(_)) => Some("submitted"),
        ("pending" | "submitted", Outcome::NotFound | Outcome::Unavailable(_)) => Some("submitted"),
        _ => None,
    }
}

// ── Chain access ──────────────────────────────────────────────────────────────

/// Why a transaction could not be looked up.
#[derive(Debug)]
pub enum ChainError {
    /// No usable network / Horizon configuration: verification is switched off.
    NotConfigured,
    /// The lookup failed; the text is for operators.
    Unavailable(String),
}

/// Where verification reads transactions from. Horizon in production; tests
/// substitute their own.
#[axum::async_trait]
pub trait ChainSource: Send + Sync {
    /// `Ok(None)` when the network does not know the transaction.
    async fn fetch_transaction(&self, hash: &str) -> Result<Option<ChainTx>, ChainError>;
}

/// A validated network + Horizon pair.
#[derive(Debug, Clone, PartialEq)]
pub struct HorizonConfig {
    pub base_url: String,
    /// The network passphrase the Horizon server has to report.
    pub passphrase: &'static str,
}

impl HorizonConfig {
    /// Both values are required; anything unset, unknown or malformed yields
    /// `None` (verification stays off) rather than a guessed default.
    pub fn from_values(network: Option<&str>, horizon_url: Option<&str>) -> Option<Self> {
        let passphrase = match network?.trim().to_ascii_lowercase().as_str() {
            "mainnet" | "public" => MAINNET_PASSPHRASE,
            "testnet" => TESTNET_PASSPHRASE,
            _ => return None,
        };
        let url = reqwest::Url::parse(horizon_url?.trim()).ok()?;
        let local = matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
        let allowed = url.scheme() == "https" || (url.scheme() == "http" && local);
        if !allowed || url.host_str().is_none() {
            return None;
        }
        Some(Self {
            base_url: url.as_str().trim_end_matches('/').to_string(),
            passphrase,
        })
    }
}

/// Does Horizon's root document report the passphrase of the expected network?
pub fn network_matches(expected: &str, root: &Value) -> bool {
    root.get("network_passphrase").and_then(Value::as_str) == Some(expected)
}

pub struct HorizonClient {
    config: Option<HorizonConfig>,
    http: reqwest::Client,
    /// Set once Horizon has reported the expected network passphrase.
    network_checked: AtomicBool,
}

impl HorizonClient {
    pub fn new(config: Option<HorizonConfig>) -> Self {
        Self {
            config,
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .expect("valid reqwest client configuration"),
            network_checked: AtomicBool::new(false),
        }
    }

    /// Reads `STELLAR_NETWORK` and `STELLAR_HORIZON_URL`. With either missing
    /// or invalid, the client reports every lookup as not configured.
    pub fn from_env() -> Self {
        let config = HorizonConfig::from_values(
            std::env::var("STELLAR_NETWORK").ok().as_deref(),
            std::env::var("STELLAR_HORIZON_URL").ok().as_deref(),
        );
        if config.is_none() {
            tracing::warn!(
                "STELLAR_NETWORK (mainnet|testnet) and STELLAR_HORIZON_URL (https) must both be \
                 set: contribution verification is disabled"
            );
        }
        Self::new(config)
    }

    async fn get(&self, base_url: &str, path: &str) -> Result<Option<Value>, String> {
        let response = self
            .http
            .get(format!("{base_url}{path}"))
            .send()
            .await
            .map_err(|e| format!("horizon request failed: {}", e.without_url()))?;
        match response.status() {
            s if s == reqwest::StatusCode::NOT_FOUND => Ok(None),
            s if s.is_success() => response
                .json()
                .await
                .map(Some)
                .map_err(|e| format!("horizon returned invalid JSON: {}", e.without_url())),
            s => Err(format!("horizon returned HTTP {s}")),
        }
    }

    /// Make sure the configured Horizon serves the configured network.
    async fn ensure_network(&self, config: &HorizonConfig) -> Result<(), ChainError> {
        if self.network_checked.load(Ordering::Acquire) {
            return Ok(());
        }
        let root = self
            .get(&config.base_url, "/")
            .await
            .map_err(ChainError::Unavailable)?
            .ok_or_else(|| ChainError::Unavailable("horizon root not found".to_string()))?;
        if !network_matches(config.passphrase, &root) {
            return Err(ChainError::Unavailable(
                "horizon serves a different network than STELLAR_NETWORK".to_string(),
            ));
        }
        self.network_checked.store(true, Ordering::Release);
        Ok(())
    }
}

#[axum::async_trait]
impl ChainSource for HorizonClient {
    async fn fetch_transaction(&self, hash: &str) -> Result<Option<ChainTx>, ChainError> {
        let config = self.config.as_ref().ok_or(ChainError::NotConfigured)?;
        self.ensure_network(config).await?;
        let base = config.base_url.as_str();
        // `hash` is validated hex, so it is safe to put in the path.
        let Some(tx) = self
            .get(base, &format!("/transactions/{hash}"))
            .await
            .map_err(ChainError::Unavailable)?
        else {
            return Ok(None);
        };
        let operations = self
            .get(
                base,
                &format!("/transactions/{hash}/operations?limit=200&order=asc"),
            )
            .await
            .map_err(ChainError::Unavailable)?
            .ok_or_else(|| {
                ChainError::Unavailable(
                    "horizon has the transaction but not its operations".to_string(),
                )
            })?;
        parse_horizon(&tx, &operations)
            .map(Some)
            .map_err(ChainError::Unavailable)
    }
}

// ── Verification ──────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct VerifyResult {
    pub contribution: Contribution,
    /// What this attempt found: `confirmed` | `failed` (the contribution is
    /// now closed) | `mismatch` (this transaction does not pay the
    /// contribution, which stays open) | `not_found` | `unavailable`.
    pub outcome: &'static str,
    pub reason: Option<String>,
    /// True only for the call that moved the contribution to `confirmed`.
    pub newly_confirmed: bool,
}

fn is_unique_violation(e: &sqlx::Error) -> bool {
    e.as_database_error()
        .and_then(|d| d.code())
        .is_some_and(|c| c == "23505")
}

/// Verify `contribution_id` of `circle_id`. `tx_hash` supplies (or replaces,
/// while still unconfirmed) the transaction to check; without it the stored
/// hash is re-checked. The contributing member can do either; circle owners /
/// admins can only re-check the hash on file.
pub async fn verify_contribution(
    pool: &PgPool,
    chain: &dyn ChainSource,
    circle_id: Uuid,
    contribution_id: Uuid,
    actor: &str,
    tx_hash: Option<&str>,
) -> Result<VerifyResult, &'static str> {
    let role = circle_membership::require_circle_permission(
        pool,
        circle_id,
        actor,
        CirclePermission::ViewFinancialInfo,
    )
    .await?;
    let contribution = fetch(pool, circle_id, contribution_id)
        .await
        .map_err(|_| "internal_error")?
        .ok_or("contribution_not_found")?;
    if contribution.member_address != actor && !role.has_permission(CirclePermission::ManagePayouts)
    {
        return Err("permission_denied");
    }

    // Already confirmed: report it, never credit twice.
    if contribution.status == "confirmed" {
        return Ok(VerifyResult {
            contribution,
            outcome: "confirmed",
            reason: None,
            newly_confirmed: false,
        });
    }
    if !matches!(contribution.status.as_str(), "pending" | "submitted") {
        return Err("contribution_closed");
    }

    let on_file = contribution.tx_hash.as_deref().and_then(normalize_tx_hash);
    let hash = match tx_hash {
        Some(raw) => normalize_tx_hash(raw).ok_or("invalid_tx_hash")?,
        None => on_file.clone().ok_or("tx_hash_required")?,
    };
    // Only the contributor decides which transaction pays the contribution.
    if contribution.member_address != actor && on_file.as_ref() != Some(&hash) {
        return Err("permission_denied");
    }

    // A transaction can back only one live contribution (this also covers
    // contributions recorded through the older `/contribute` endpoint).
    let used: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM circle_contributions \
          WHERE lower(tx_hash) = $1 AND id <> $2 AND status IN ('submitted', 'confirmed'))",
    )
    .bind(&hash)
    .bind(contribution_id)
    .fetch_one(pool)
    .await
    .map_err(|_| "internal_error")?;
    if used {
        return Err("tx_already_used");
    }

    let (Some(reference), Some(destination), Some(asset)) = (
        contribution.reference.as_deref(),
        contribution.destination.as_deref(),
        contribution.asset.as_deref(),
    ) else {
        return Err("internal_error");
    };
    let amount: i64 = contribution.amount.parse().map_err(|_| "internal_error")?;
    let expected = Expected {
        reference,
        destination,
        asset,
        amount,
        payer: &contribution.member_address,
    };

    // The network call happens outside any database transaction.
    let (outcome, ledger) = match chain.fetch_transaction(&hash).await {
        Err(ChainError::NotConfigured) => (Outcome::Unavailable(NOT_CONFIGURED_REASON), None),
        Err(ChainError::Unavailable(detail)) => {
            tracing::warn!(%contribution_id, error = %detail, "horizon lookup failed");
            (Outcome::Unavailable(UNAVAILABLE_REASON), None)
        }
        Ok(None) => (Outcome::NotFound, None),
        Ok(Some(tx)) if tx.hash != hash => {
            tracing::warn!(%contribution_id, "horizon returned a different transaction");
            (Outcome::Unavailable(UNAVAILABLE_REASON), None)
        }
        Ok(Some(tx)) => (match_contribution(&expected, &tx), tx.ledger),
    };

    let mut db = pool.begin().await.map_err(|_| "internal_error")?;
    let (current, locked_hash): (String, Option<String>) =
        sqlx::query_as("SELECT status, tx_hash FROM circle_contributions WHERE id = $1 FOR UPDATE")
            .bind(contribution_id)
            .fetch_one(&mut *db)
            .await
            .map_err(|_| "internal_error")?;
    // The hash on file may have been replaced while the network was queried.
    let source = if locked_hash.as_deref().and_then(normalize_tx_hash).as_ref() == Some(&hash) {
        HashSource::OnFile
    } else {
        HashSource::Supplied
    };
    if source == HashSource::Supplied && contribution.member_address != actor {
        return Err("permission_denied");
    }
    let next = next_status(&current, source, &outcome).ok_or("contribution_closed")?;
    let newly_confirmed = next == "confirmed" && current != "confirmed";
    // A supplied hash that does not pay the contribution is not kept.
    let rejected = matches!(outcome, Outcome::Failed(_)) && next != "failed";

    if current != "confirmed" {
        let update = sqlx::query(
            "UPDATE circle_contributions \
                SET status = $2, tx_hash = CASE WHEN $5 THEN tx_hash ELSE $3 END, \
                    failure_reason = $4, updated_at = NOW(), \
                    verified_at = CASE WHEN $2 = 'confirmed' THEN NOW() ELSE verified_at END \
              WHERE id = $1",
        )
        .bind(contribution_id)
        .bind(next)
        .bind(&hash)
        .bind(match &outcome {
            Outcome::Failed(reason) if next == "failed" => Some(*reason),
            _ => None,
        })
        .bind(rejected && source == HashSource::Supplied)
        .execute(&mut *db)
        .await;
        if let Err(e) = update {
            return Err(if is_unique_violation(&e) {
                "tx_already_used"
            } else {
                "internal_error"
            });
        }
    }

    if newly_confirmed {
        sqlx::query(
            "UPDATE circles SET balance = balance + $1, total_contributed = total_contributed + $1 \
              WHERE id = $2",
        )
        .bind(amount)
        .bind(circle_id)
        .execute(&mut *db)
        .await
        .map_err(|_| "internal_error")?;
        sqlx::query(
            "UPDATE circle_members SET total_contributed = total_contributed + $1 \
              WHERE circle_id = $2 AND address = $3",
        )
        .bind(amount)
        .bind(circle_id)
        .bind(&contribution.member_address)
        .execute(&mut *db)
        .await
        .map_err(|_| "internal_error")?;
        // Record the confirmation where savings goals already look for
        // on-chain verified contributions.
        sqlx::query(
            "INSERT INTO circle_reconciliation_records \
                 (id, record_type, local_record_id, circle_id, tx_hash, local_state, \
                  on_chain_state, discrepancy, details, checked_at) \
             VALUES ($1, 'contribution', $2, $3, $4, 'confirmed', 'success', 'none', \
                     'verified by contribution verification', NOW()) \
             ON CONFLICT (record_type, local_record_id) DO UPDATE SET \
                 tx_hash = EXCLUDED.tx_hash, local_state = EXCLUDED.local_state, \
                 on_chain_state = 'success', discrepancy = 'none', \
                 details = EXCLUDED.details, checked_at = NOW()",
        )
        .bind(Uuid::new_v4())
        .bind(contribution_id)
        .bind(circle_id)
        .bind(&hash)
        .execute(&mut *db)
        .await
        .map_err(|_| "internal_error")?;
    }

    // An earlier call may already have confirmed it; report what is true.
    let reported = if current == "confirmed" {
        "confirmed"
    } else if rejected {
        "mismatch"
    } else {
        outcome.as_str()
    };
    sqlx::query(
        "INSERT INTO circle_contribution_verifications \
             (contribution_id, circle_id, tx_hash, outcome, reason, ledger) \
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(contribution_id)
    .bind(circle_id)
    .bind(&hash)
    .bind(reported)
    .bind(outcome.reason())
    .bind(ledger)
    .execute(&mut *db)
    .await
    .map_err(|_| "internal_error")?;
    db.commit().await.map_err(|_| "internal_error")?;

    let contribution = fetch(pool, circle_id, contribution_id)
        .await
        .map_err(|_| "internal_error")?
        .ok_or("contribution_not_found")?;
    Ok(VerifyResult {
        contribution,
        outcome: reported,
        reason: outcome.reason(),
        newly_confirmed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const CIRCLE: &str = "CCIRCLECONTRACT";
    const PAYER: &str = "GPAYER";
    const USDC: &str = "USDC:GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN";

    fn expected(asset: &'static str) -> Expected<'static> {
        Expected {
            reference: "zcabc",
            destination: CIRCLE,
            asset,
            amount: 25_000_000,
            payer: PAYER,
        }
    }

    fn tx(transfers: Vec<Transfer>) -> ChainTx {
        ChainTx {
            hash: "a".repeat(64),
            successful: true,
            ledger: Some(7),
            memo_type: Some("text".into()),
            memo: Some("zcabc".into()),
            source_account: Some(PAYER.into()),
            transfers,
        }
    }

    fn transfer(to: &str, asset: &str, amount: i64) -> Transfer {
        Transfer {
            from: Some(PAYER.into()),
            to: to.into(),
            asset: asset.into(),
            amount,
        }
    }

    #[test]
    fn stroop_amounts_parse_exactly() {
        assert_eq!(parse_stroops("2.5000000"), Some(25_000_000));
        assert_eq!(parse_stroops("0.0000001"), Some(1));
        assert_eq!(parse_stroops("12"), Some(120_000_000));
        assert_eq!(parse_stroops("1.5"), Some(15_000_000));
        for bad in [
            "",
            "-1",
            "1.00000001",
            "1e5",
            "abc",
            ".5",
            "9223372036854775807",
        ] {
            assert_eq!(parse_stroops(bad), None, "{bad}");
        }
        assert_eq!(normalize_tx_hash(&"AB".repeat(32)), Some("ab".repeat(32)));
        assert_eq!(normalize_tx_hash("deadbeef"), None);
    }

    #[test]
    fn exact_payment_is_confirmed() {
        let good = tx(vec![transfer(CIRCLE, "native", 25_000_000)]);
        assert_eq!(
            match_contribution(&expected("native"), &good),
            Outcome::Confirmed
        );
        // Split payments to the destination add up.
        let split = tx(vec![
            transfer(CIRCLE, "native", 10_000_000),
            transfer(CIRCLE, "native", 15_000_000),
            transfer("GSOMEONEELSE", "native", 1),
        ]);
        assert_eq!(
            match_contribution(&expected("native"), &split),
            Outcome::Confirmed
        );
    }

    #[test]
    fn every_mismatch_fails_with_its_own_reason() {
        let want = expected(USDC);
        let reason = |t: ChainTx| match match_contribution(&want, &t) {
            Outcome::Failed(r) => r,
            other => panic!("expected failure, got {other:?}"),
        };
        let paid = || vec![transfer(CIRCLE, USDC, 25_000_000)];

        let mut failed_tx = tx(paid());
        failed_tx.successful = false;
        assert_eq!(reason(failed_tx), "transaction_failed");

        let mut wrong_memo = tx(paid());
        wrong_memo.memo = Some("zcother".into());
        assert_eq!(reason(wrong_memo), "memo_mismatch");
        let mut no_memo = tx(paid());
        no_memo.memo_type = Some("none".into());
        assert_eq!(reason(no_memo), "memo_mismatch");

        assert_eq!(
            reason(tx(vec![transfer("GELSEWHERE", USDC, 25_000_000)])),
            "destination_mismatch"
        );
        assert_eq!(
            reason(tx(vec![transfer(CIRCLE, "native", 25_000_000)])),
            "asset_mismatch"
        );
        assert_eq!(
            reason(tx(vec![transfer(CIRCLE, USDC, 24_999_999)])),
            "amount_mismatch"
        );
        assert_eq!(
            reason(tx(vec![transfer(CIRCLE, USDC, 25_000_001)])),
            "amount_mismatch"
        );

        let mut stranger = tx(paid());
        stranger.transfers[0].from = Some("GSTRANGER".into());
        assert_eq!(reason(stranger), "payer_mismatch");
    }

    #[test]
    fn only_confirmation_credits_and_closed_contributions_stay_closed() {
        use HashSource::{OnFile, Supplied};
        let unconfirmed = [Outcome::NotFound, Outcome::Unavailable(UNAVAILABLE_REASON)];

        for source in [OnFile, Supplied] {
            assert_eq!(
                next_status("pending", source, &Outcome::Confirmed),
                Some("confirmed")
            );
            assert_eq!(
                next_status("submitted", source, &Outcome::Confirmed),
                Some("confirmed")
            );
            for outcome in &unconfirmed {
                assert_eq!(next_status("pending", source, outcome), Some("submitted"));
                assert_eq!(next_status("submitted", source, outcome), Some("submitted"));
            }
            // Confirmed is final; failed / expired cannot be revived.
            let mismatch = Outcome::Failed("memo_mismatch");
            assert_eq!(
                next_status("confirmed", source, &mismatch),
                Some("confirmed")
            );
            assert_eq!(next_status("failed", source, &Outcome::Confirmed), None);
            assert_eq!(next_status("expired", source, &Outcome::Confirmed), None);
        }
    }

    #[test]
    fn a_wrong_hash_cannot_close_a_contribution() {
        use HashSource::{OnFile, Supplied};
        let reasons = [
            "transaction_failed",
            "memo_mismatch",
            "destination_mismatch",
            "asset_mismatch",
            "payer_mismatch",
            "amount_mismatch",
        ];
        for reason in reasons {
            let outcome = Outcome::Failed(reason);
            // A hash supplied with the request that does not pay the
            // contribution (another payment, wrong amount, a failed
            // transaction of someone else) is rejected, nothing more.
            assert_eq!(next_status("pending", Supplied, &outcome), Some("pending"));
            assert_eq!(
                next_status("submitted", Supplied, &outcome),
                Some("submitted"),
                "{reason}"
            );
            // The same holds for a stored hash whose transaction exists but
            // does not match.
            if reason != "transaction_failed" {
                assert_eq!(
                    next_status("submitted", OnFile, &outcome),
                    Some("submitted"),
                    "{reason}"
                );
            }
        }
        // Only the transaction on file failing on the network closes it.
        assert_eq!(
            next_status("submitted", OnFile, &Outcome::Failed("transaction_failed")),
            Some("failed")
        );
        assert_eq!(
            next_status("failed", OnFile, &Outcome::Failed("transaction_failed")),
            None
        );
    }

    #[test]
    fn verification_needs_an_explicit_network_and_horizon_url() {
        let cfg = |n, u| HorizonConfig::from_values(n, u);
        let ok = cfg(
            Some("testnet"),
            Some("https://horizon-testnet.stellar.org/"),
        )
        .unwrap();
        assert_eq!(ok.base_url, "https://horizon-testnet.stellar.org");
        assert_eq!(ok.passphrase, TESTNET_PASSPHRASE);
        for network in ["mainnet", "PUBLIC", " Mainnet "] {
            let c = cfg(Some(network), Some("https://horizon.stellar.org")).unwrap();
            assert_eq!(c.passphrase, MAINNET_PASSPHRASE, "{network}");
        }
        // Local http is fine for development and mocks.
        assert!(cfg(Some("testnet"), Some("http://localhost:8000")).is_some());
        assert!(cfg(Some("testnet"), Some("http://127.0.0.1:8000")).is_some());

        // Nothing is guessed: a missing or unknown value turns verification off.
        let url = Some("https://horizon.stellar.org");
        assert_eq!(cfg(None, url), None);
        assert_eq!(cfg(Some(""), url), None);
        assert_eq!(cfg(Some("futurenet"), url), None);
        assert_eq!(cfg(Some("mainnet"), None), None);
        assert_eq!(cfg(Some("mainnet"), Some("")), None);
        assert_eq!(cfg(Some("mainnet"), Some("not a url")), None);
        assert_eq!(
            cfg(Some("mainnet"), Some("http://horizon.stellar.org")),
            None
        );
        assert_eq!(
            cfg(Some("mainnet"), Some("ftp://horizon.stellar.org")),
            None
        );
    }

    #[test]
    fn horizon_must_serve_the_configured_network() {
        let testnet = json!({"network_passphrase": TESTNET_PASSPHRASE});
        assert!(network_matches(TESTNET_PASSPHRASE, &testnet));
        assert!(!network_matches(MAINNET_PASSPHRASE, &testnet));
        assert!(!network_matches(MAINNET_PASSPHRASE, &json!({})));
        assert!(!network_matches(
            MAINNET_PASSPHRASE,
            &json!({"network_passphrase": 1})
        ));
    }

    #[tokio::test]
    async fn lookup_errors_do_not_expose_the_horizon_url() {
        // Nothing listens on this loopback port, so the lookup fails in transport.
        let config =
            HorizonConfig::from_values(Some("testnet"), Some("http://127.0.0.1:9/internal-path"))
                .unwrap();
        let client = HorizonClient::new(Some(config));
        match client.fetch_transaction(&"a".repeat(64)).await {
            Err(ChainError::Unavailable(detail)) => {
                assert!(
                    !detail.contains("127.0.0.1") && !detail.contains("internal-path"),
                    "{detail}"
                );
            }
            other => panic!("expected an unavailable lookup, got {other:?}"),
        }
    }

    #[test]
    fn clients_only_see_generic_reasons_for_unavailable_lookups() {
        assert_eq!(
            Outcome::Unavailable(UNAVAILABLE_REASON).reason().as_deref(),
            Some("horizon_unavailable")
        );
        assert_eq!(
            Outcome::Unavailable(NOT_CONFIGURED_REASON)
                .reason()
                .as_deref(),
            Some("verification_not_configured")
        );
        assert_eq!(
            Outcome::Unavailable(UNAVAILABLE_REASON).as_str(),
            "unavailable"
        );
    }

    #[tokio::test]
    async fn an_unconfigured_client_never_returns_a_transaction() {
        let client = HorizonClient::new(None);
        let result = client.fetch_transaction(&"a".repeat(64)).await;
        assert!(matches!(result, Err(ChainError::NotConfigured)));
    }

    #[test]
    fn horizon_documents_reduce_to_transfers() {
        let tx_doc = json!({
            "hash": "A".repeat(64), "successful": true, "ledger": 4242,
            "memo_type": "text", "memo": "zcabc", "source_account": PAYER
        });
        let ops = json!({"_embedded": {"records": [
            {"type": "create_account", "funder": PAYER},
            {"type": "payment", "from": PAYER, "to": CIRCLE, "amount": "1.5000000",
             "asset_type": "native"},
            {"type": "payment", "from": PAYER, "to": CIRCLE, "amount": "2.0000000",
             "asset_type": "credit_alphanum4", "asset_code": "USDC",
             "asset_issuer": "GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN"},
            {"type": "invoke_host_function", "asset_balance_changes": [
                {"type": "transfer", "from": PAYER, "to": CIRCLE, "amount": "3.0000000",
                 "asset_type": "native"},
                {"type": "mint", "to": CIRCLE, "amount": "9.0000000", "asset_type": "native"}
            ]}
        ]}});
        let parsed = parse_horizon(&tx_doc, &ops).unwrap();
        assert_eq!(parsed.hash, "a".repeat(64));
        assert_eq!(parsed.ledger, Some(4242));
        assert_eq!(parsed.memo.as_deref(), Some("zcabc"));
        let amounts: Vec<(&str, i64)> = parsed
            .transfers
            .iter()
            .map(|t| (t.asset.as_str(), t.amount))
            .collect();
        assert_eq!(
            amounts,
            vec![
                ("native", 15_000_000),
                (USDC, 20_000_000),
                ("native", 30_000_000)
            ]
        );

        // Malformed documents are errors, never a confirmation.
        assert!(parse_horizon(&json!({"hash": "x"}), &ops).is_err());
        assert!(parse_horizon(&tx_doc, &json!({})).is_err());
    }
}
