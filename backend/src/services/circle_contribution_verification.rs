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
//! * A transaction that failed or does not match marks the contribution
//!   `failed`; it never counts.
//! * A transaction Horizon cannot find (yet), or an unreachable Horizon,
//!   leaves the contribution `submitted`: unconfirmed, not successful.
//! * A transaction hash backs at most one live contribution.
//! * Every attempt is persisted in `circle_contribution_verifications`.

use crate::services::circle_contributions::{fetch, Contribution};
use crate::services::circle_membership::{self, CirclePermission};
use serde::Serialize;
use serde_json::Value;
use sqlx::PgPool;
use std::time::Duration;
use uuid::Uuid;

const DEFAULT_HORIZON_URL: &str = "https://horizon-testnet.stellar.org";
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
    /// The network could not be queried.
    Unavailable(String),
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
            Self::Failed(reason) => Some((*reason).to_string()),
            Self::Unavailable(reason) => Some(reason.chars().take(500).collect()),
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

/// Status a contribution moves to after a verification attempt. `None` means
/// the contribution is closed (`failed` / `expired`) and cannot be verified.
pub fn next_status(current: &str, outcome: &Outcome) -> Option<&'static str> {
    match (current, outcome) {
        ("confirmed", _) => Some("confirmed"),
        ("pending" | "submitted", Outcome::Confirmed) => Some("confirmed"),
        ("pending" | "submitted", Outcome::Failed(_)) => Some("failed"),
        ("pending" | "submitted", Outcome::NotFound | Outcome::Unavailable(_)) => Some("submitted"),
        _ => None,
    }
}

// ── Chain access ──────────────────────────────────────────────────────────────

/// Where verification reads transactions from. Horizon in production; tests
/// substitute their own.
#[axum::async_trait]
pub trait ChainSource: Send + Sync {
    /// `Ok(None)` when the network does not know the transaction.
    async fn fetch_transaction(&self, hash: &str) -> Result<Option<ChainTx>, String>;
}

pub struct HorizonClient {
    base_url: String,
    http: reqwest::Client,
}

impl HorizonClient {
    pub fn new(base_url: String) -> Self {
        Self {
            base_url: base_url.trim().trim_end_matches('/').to_string(),
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .expect("valid reqwest client configuration"),
        }
    }

    /// `STELLAR_HORIZON_URL`, defaulting to the public testnet Horizon.
    pub fn from_env() -> Self {
        Self::new(
            std::env::var("STELLAR_HORIZON_URL")
                .ok()
                .filter(|u| !u.trim().is_empty())
                .unwrap_or_else(|| DEFAULT_HORIZON_URL.to_string()),
        )
    }

    async fn get(&self, path: &str) -> Result<Option<Value>, String> {
        let response = self
            .http
            .get(format!("{}{}", self.base_url, path))
            .send()
            .await
            .map_err(|e| format!("horizon request failed: {e}"))?;
        match response.status() {
            s if s == reqwest::StatusCode::NOT_FOUND => Ok(None),
            s if s.is_success() => response
                .json()
                .await
                .map(Some)
                .map_err(|e| format!("horizon returned invalid JSON: {e}")),
            s => Err(format!("horizon returned HTTP {s}")),
        }
    }
}

#[axum::async_trait]
impl ChainSource for HorizonClient {
    async fn fetch_transaction(&self, hash: &str) -> Result<Option<ChainTx>, String> {
        // `hash` is validated hex, so it is safe to put in the path.
        let Some(tx) = self.get(&format!("/transactions/{hash}")).await? else {
            return Ok(None);
        };
        let operations = self
            .get(&format!(
                "/transactions/{hash}/operations?limit=200&order=asc"
            ))
            .await?
            .ok_or("horizon has the transaction but not its operations")?;
        parse_horizon(&tx, &operations).map(Some)
    }
}

// ── Verification ──────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct VerifyResult {
    pub contribution: Contribution,
    /// `confirmed` | `failed` | `not_found` | `unavailable`.
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
/// hash is re-checked. Callable by the contributing member and by circle
/// owners / admins.
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

    let hash = match tx_hash {
        Some(raw) => normalize_tx_hash(raw).ok_or("invalid_tx_hash")?,
        None => contribution
            .tx_hash
            .as_deref()
            .and_then(normalize_tx_hash)
            .ok_or("tx_hash_required")?,
    };

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
        Err(e) => (Outcome::Unavailable(e), None),
        Ok(None) => (Outcome::NotFound, None),
        Ok(Some(tx)) if tx.hash != hash => (
            Outcome::Unavailable("horizon returned a different transaction".to_string()),
            None,
        ),
        Ok(Some(tx)) => (match_contribution(&expected, &tx), tx.ledger),
    };

    let mut db = pool.begin().await.map_err(|_| "internal_error")?;
    let current: String =
        sqlx::query_scalar("SELECT status FROM circle_contributions WHERE id = $1 FOR UPDATE")
            .bind(contribution_id)
            .fetch_one(&mut *db)
            .await
            .map_err(|_| "internal_error")?;
    let next = next_status(&current, &outcome).ok_or("contribution_closed")?;
    let newly_confirmed = next == "confirmed" && current != "confirmed";

    if current != "confirmed" {
        let update = sqlx::query(
            "UPDATE circle_contributions \
                SET status = $2, tx_hash = $3, failure_reason = $4, updated_at = NOW(), \
                    verified_at = CASE WHEN $2 = 'confirmed' THEN NOW() ELSE verified_at END \
              WHERE id = $1",
        )
        .bind(contribution_id)
        .bind(next)
        .bind(&hash)
        .bind(match &outcome {
            Outcome::Failed(reason) => Some(*reason),
            _ => None,
        })
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
        let failed = Outcome::Failed("memo_mismatch");
        let unconfirmed = [Outcome::NotFound, Outcome::Unavailable("down".into())];

        assert_eq!(
            next_status("pending", &Outcome::Confirmed),
            Some("confirmed")
        );
        assert_eq!(
            next_status("submitted", &Outcome::Confirmed),
            Some("confirmed")
        );
        assert_eq!(next_status("pending", &failed), Some("failed"));
        for outcome in &unconfirmed {
            assert_eq!(next_status("pending", outcome), Some("submitted"));
            assert_eq!(next_status("submitted", outcome), Some("submitted"));
        }
        // Confirmed is final; failed / expired cannot be revived.
        assert_eq!(next_status("confirmed", &failed), Some("confirmed"));
        assert_eq!(next_status("failed", &Outcome::Confirmed), None);
        assert_eq!(next_status("expired", &Outcome::Confirmed), None);
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
