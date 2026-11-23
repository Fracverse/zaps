//! #1205 contribution initiation.
//!
//! Initiating a contribution validates the member, asset, amount and the
//! circle's active rule, then records a `pending` row that pins everything the
//! later on-chain check has to match (amount, asset, destination, rule
//! version) together with a unique reference the payer attaches to the
//! Stellar payment as a text memo. Nothing is credited to the circle until
//! `circle_contribution_verification` confirms the payment.
//!
//! Lifecycle: `pending` → `submitted` (hash received, unconfirmed) →
//! `confirmed` | `failed`; an unpaid `pending` initiation turns `expired`.

use crate::services::circle_contribution_rules::{
    self as rules, evaluate_contribution, normalize_asset, RuleSpec,
};
use crate::services::circle_goals::MAX_TARGET_AMOUNT;
use crate::services::circle_membership::{self, CirclePermission};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::postgres::{PgPool, PgRow};
use sqlx::Row;
use uuid::Uuid;

/// How long an initiation waits for its payment before it expires.
pub const INITIATION_TTL_HOURS: i64 = 24;

// ── Models ────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InitiateRequest {
    /// Amount in stroops, as a decimal integer string.
    pub amount: String,
    /// `native` or `CODE:ISSUER`; defaults to the rule's first allowed asset.
    #[serde(default)]
    pub asset: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Contribution {
    pub id: Uuid,
    pub circle_id: Uuid,
    pub member_address: String,
    pub amount: String,
    /// `None` for contributions recorded before assets were tracked.
    pub asset: Option<String>,
    pub status: String,
    /// Text memo the payment must carry.
    pub reference: Option<String>,
    pub tx_hash: Option<String>,
    pub destination: Option<String>,
    pub rule_version: Option<i32>,
    pub is_late: bool,
    pub late_fee: String,
    pub due_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub verified_at: Option<DateTime<Utc>>,
    pub failure_reason: Option<String>,
}

/// What the client needs in order to make the Stellar payment.
#[derive(Debug, Serialize)]
pub struct PaymentInstructions {
    pub destination: String,
    pub asset: String,
    pub amount: String,
    pub memo_type: &'static str,
    pub memo: String,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct Initiated {
    pub contribution: Contribution,
    pub payment: PaymentInstructions,
}

#[derive(Debug)]
pub enum InitiateError {
    Rejected(&'static str),
    /// The member already has the same open initiation; it is returned so the
    /// client can carry on with it.
    Duplicate(Box<Contribution>),
}

impl From<&'static str> for InitiateError {
    fn from(code: &'static str) -> Self {
        Self::Rejected(code)
    }
}

pub(crate) const CONTRIBUTION_COLUMNS: &str = "id, circle_id, member_address, amount, asset, \
    status, reference, tx_hash, destination, rule_version, is_late, late_fee, due_at, \
    expires_at, created_at, verified_at, failure_reason";

pub(crate) fn row_to_contribution(row: &PgRow) -> Result<Contribution, sqlx::Error> {
    let amount: i64 = row.try_get("amount")?;
    let late_fee: i64 = row.try_get("late_fee")?;
    Ok(Contribution {
        id: row.try_get("id")?,
        circle_id: row.try_get("circle_id")?,
        member_address: row.try_get("member_address")?,
        amount: amount.to_string(),
        asset: row.try_get("asset")?,
        status: row.try_get("status")?,
        reference: row.try_get("reference")?,
        tx_hash: row.try_get("tx_hash")?,
        destination: row.try_get("destination")?,
        rule_version: row.try_get("rule_version")?,
        is_late: row.try_get("is_late")?,
        late_fee: late_fee.to_string(),
        due_at: row.try_get("due_at")?,
        expires_at: row.try_get("expires_at")?,
        created_at: row.try_get("created_at")?,
        verified_at: row.try_get("verified_at")?,
        failure_reason: row.try_get("failure_reason")?,
    })
}

// ── Pure helpers ──────────────────────────────────────────────────────────────

/// Parse a positive stroop amount from a decimal integer string.
pub fn parse_amount(raw: &str) -> Result<i64, &'static str> {
    match raw.trim().parse::<i64>() {
        Ok(v) if v > 0 && v <= MAX_TARGET_AMOUNT => Ok(v),
        _ => Err("invalid_amount"),
    }
}

/// Unique, unguessable payment reference: `zc` + 24 hex characters (26 bytes,
/// inside the 28 byte Stellar text memo limit).
pub fn generate_reference(circle_id: Uuid, member: &str, nonce: Uuid) -> String {
    let digest = Sha256::digest(format!("{circle_id}|{member}|{nonce}").as_bytes());
    format!("zc{}", &hex::encode(digest)[..24])
}

// ── Initiation ────────────────────────────────────────────────────────────────

/// Initiate a contribution for `member`. `now` drives rule evaluation only.
pub async fn initiate(
    pool: &PgPool,
    circle_id: Uuid,
    member: &str,
    req: InitiateRequest,
    now: DateTime<Utc>,
) -> Result<Initiated, InitiateError> {
    let amount = parse_amount(&req.amount)?;

    // Only members allowed to contribute.
    circle_membership::require_circle_permission(
        pool,
        circle_id,
        member,
        CirclePermission::Contribute,
    )
    .await?;

    // The circle's contract address is where payments have to go.
    let destination: Option<String> =
        sqlx::query_scalar("SELECT contract_address FROM circles WHERE id = $1")
            .bind(circle_id)
            .fetch_optional(pool)
            .await
            .map_err(|_| "internal_error")?
            .ok_or("circle_not_found")?;
    let destination = destination
        .map(|d| d.trim().to_string())
        .filter(|d| !d.is_empty())
        .ok_or("circle_not_accepting_contributions")?;

    let stored = rules::load_active(pool, circle_id)
        .await
        .map_err(|_| "internal_error")?;
    let spec: RuleSpec = stored.as_ref().map(|s| s.rule.clone()).unwrap_or_default();

    let asset = match req.asset.as_deref() {
        Some(raw) => normalize_asset(raw).ok_or("invalid_asset")?,
        None => spec.effective_assets().remove(0),
    };
    let evaluation = evaluate_contribution(&spec, member, amount, &asset, now)?;

    let id = Uuid::new_v4();
    let reference = generate_reference(circle_id, member, id);

    // An abandoned initiation must not block a fresh one.
    sqlx::query(
        "UPDATE circle_contributions SET status = 'expired', updated_at = NOW() \
          WHERE circle_id = $1 AND member_address = $2 \
            AND status = 'pending' AND expires_at <= NOW()",
    )
    .bind(circle_id)
    .bind(member)
    .execute(pool)
    .await
    .map_err(|_| "internal_error")?;

    let insert = format!(
        "INSERT INTO circle_contributions \
             (id, circle_id, member_address, amount, status, reference, asset, destination, \
              rule_id, rule_version, is_late, late_fee, due_at, expires_at) \
         VALUES ($1, $2, $3, $4, 'pending', $5, $6, $7, $8, $9, $10, $11, $12, \
                 NOW() + make_interval(hours => $13::int)) \
         ON CONFLICT (circle_id, member_address, asset, amount) WHERE status = 'pending' \
         DO NOTHING RETURNING {CONTRIBUTION_COLUMNS}"
    );
    let inserted = sqlx::query(&insert)
        .bind(id)
        .bind(circle_id)
        .bind(member)
        .bind(amount)
        .bind(&reference)
        .bind(&asset)
        .bind(&destination)
        .bind(stored.as_ref().map(|s| s.rule_id))
        .bind(stored.as_ref().map(|s| s.version))
        .bind(evaluation.is_late)
        .bind(evaluation.late_fee)
        .bind(evaluation.due_at)
        .bind(INITIATION_TTL_HOURS as i32)
        .fetch_optional(pool)
        .await
        .map_err(|_| "internal_error")?;

    let row = match inserted {
        Some(row) => row,
        None => {
            let existing = sqlx::query(&format!(
                "SELECT {CONTRIBUTION_COLUMNS} FROM circle_contributions \
                  WHERE circle_id = $1 AND member_address = $2 AND asset = $3 \
                    AND amount = $4 AND status = 'pending'"
            ))
            .bind(circle_id)
            .bind(member)
            .bind(&asset)
            .bind(amount)
            .fetch_optional(pool)
            .await
            .map_err(|_| "internal_error")?
            .ok_or("internal_error")?;
            let existing = row_to_contribution(&existing).map_err(|_| "internal_error")?;
            return Err(InitiateError::Duplicate(Box::new(existing)));
        }
    };

    let contribution = row_to_contribution(&row).map_err(|_| "internal_error")?;
    let payment = PaymentInstructions {
        destination,
        asset,
        amount: contribution.amount.clone(),
        memo_type: "text",
        memo: reference,
        expires_at: contribution
            .expires_at
            .unwrap_or_else(|| Utc::now() + Duration::hours(INITIATION_TTL_HOURS)),
    };
    Ok(Initiated {
        contribution,
        payment,
    })
}

/// Load one contribution of a circle.
pub async fn fetch(
    pool: &PgPool,
    circle_id: Uuid,
    contribution_id: Uuid,
) -> Result<Option<Contribution>, sqlx::Error> {
    sqlx::query(&format!(
        "SELECT {CONTRIBUTION_COLUMNS} FROM circle_contributions \
          WHERE id = $1 AND circle_id = $2"
    ))
    .bind(contribution_id)
    .bind(circle_id)
    .fetch_optional(pool)
    .await?
    .as_ref()
    .map(row_to_contribution)
    .transpose()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn amounts_must_be_positive_integers_within_the_cap() {
        assert_eq!(parse_amount("10000000"), Ok(10_000_000));
        assert_eq!(parse_amount(" 5 "), Ok(5));
        for bad in ["0", "-5", "1.5", "abc", "", "100000000000000001"] {
            assert_eq!(parse_amount(bad), Err("invalid_amount"), "{bad}");
        }
    }

    #[test]
    fn references_are_unique_stable_and_memo_sized() {
        let circle = Uuid::new_v4();
        let nonce = Uuid::new_v4();
        let a = generate_reference(circle, "GMEMBER", nonce);
        assert_eq!(a, generate_reference(circle, "GMEMBER", nonce));
        assert_ne!(a, generate_reference(circle, "GMEMBER", Uuid::new_v4()));
        assert_ne!(a, generate_reference(circle, "GOTHER", nonce));
        // Stellar text memos hold at most 28 bytes.
        assert_eq!(a.len(), 26);
        assert!(a.starts_with("zc") && a[2..].bytes().all(|b| b.is_ascii_hexdigit()));
    }
}
