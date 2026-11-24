//! #1205 contribution initiation and #1208 contribution history.
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
use sqlx::postgres::{PgPool, PgRow, Postgres};
use sqlx::{QueryBuilder, Row};
use uuid::Uuid;

/// How long an initiation waits for its payment before it expires.
pub const INITIATION_TTL_HOURS: i64 = 24;
pub const STATUSES: &[&str] = &["pending", "submitted", "confirmed", "failed", "expired"];
pub const DEFAULT_PER_PAGE: i64 = 20;
pub const MAX_PER_PAGE: i64 = 100;

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

// ── History ───────────────────────────────────────────────────────────────────

/// Raw query-string values of `GET /contributions`.
#[derive(Debug, Default, Deserialize)]
pub struct HistoryQuery {
    pub member: Option<String>,
    /// RFC 3339, inclusive.
    pub from: Option<String>,
    /// RFC 3339, inclusive.
    pub to: Option<String>,
    pub asset: Option<String>,
    pub status: Option<String>,
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

/// Validated history filters.
#[derive(Debug, Clone, PartialEq)]
pub struct HistoryFilter {
    pub member: Option<String>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub asset: Option<String>,
    pub status: Option<String>,
    pub page: i64,
    pub per_page: i64,
}

fn parse_timestamp(raw: &str) -> Result<DateTime<Utc>, &'static str> {
    DateTime::parse_from_rfc3339(raw.trim())
        .map(|d| d.with_timezone(&Utc))
        .map_err(|_| "invalid_date")
}

impl HistoryFilter {
    pub fn from_query(q: HistoryQuery) -> Result<Self, &'static str> {
        let member = match q.member.as_deref().map(str::trim) {
            None | Some("") => None,
            Some(m) if m.len() <= 128 && !m.chars().any(char::is_whitespace) => Some(m.to_string()),
            Some(_) => return Err("invalid_member"),
        };
        let from = q.from.as_deref().map(parse_timestamp).transpose()?;
        let to = q.to.as_deref().map(parse_timestamp).transpose()?;
        if matches!((from, to), (Some(f), Some(t)) if f > t) {
            return Err("invalid_date_range");
        }
        let asset = q
            .asset
            .as_deref()
            .map(|a| normalize_asset(a).ok_or("invalid_asset"))
            .transpose()?;
        let status = match q.status.as_deref().map(|s| s.trim().to_lowercase()) {
            None => None,
            Some(s) if STATUSES.contains(&s.as_str()) => Some(s),
            Some(_) => return Err("invalid_status"),
        };
        let page = q.page.unwrap_or(1);
        let per_page = q.per_page.unwrap_or(DEFAULT_PER_PAGE);
        if page < 1 || !(1..=MAX_PER_PAGE).contains(&per_page) {
            return Err("invalid_page");
        }
        Ok(Self {
            member,
            from,
            to,
            asset,
            status,
            page,
            per_page,
        })
    }
}

#[derive(Debug, Serialize)]
pub struct HistoryPage {
    pub contributions: Vec<Contribution>,
    /// Records matching the filters (and visible to the caller).
    pub total: i64,
    /// Sum of the matching `confirmed` contributions.
    pub confirmed_total: String,
    pub page: i64,
    pub per_page: i64,
}

fn push_filters<'a>(
    qb: &mut QueryBuilder<'a, Postgres>,
    circle_id: Uuid,
    viewer: &'a str,
    sees_all: bool,
    f: &'a HistoryFilter,
) {
    qb.push(" WHERE circle_id = ").push_bind(circle_id);
    if !sees_all {
        // Members see every confirmed contribution plus all of their own.
        qb.push(" AND (status = 'confirmed' OR member_address = ")
            .push_bind(viewer)
            .push(")");
    }
    if let Some(member) = &f.member {
        qb.push(" AND member_address = ").push_bind(member.as_str());
    }
    if let Some(from) = f.from {
        qb.push(" AND created_at >= ").push_bind(from);
    }
    if let Some(to) = f.to {
        qb.push(" AND created_at <= ").push_bind(to);
    }
    if let Some(asset) = &f.asset {
        qb.push(" AND asset = ").push_bind(asset.as_str());
    }
    if let Some(status) = &f.status {
        qb.push(" AND status = ").push_bind(status.as_str());
    }
}

/// Paginated, filtered history of a circle's contributions. Members see all
/// confirmed contributions and their own records of any status; owners and
/// admins see everything.
pub async fn list_history(
    pool: &PgPool,
    circle_id: Uuid,
    viewer: &str,
    filter: &HistoryFilter,
) -> Result<HistoryPage, &'static str> {
    let role = circle_membership::require_circle_permission(
        pool,
        circle_id,
        viewer,
        CirclePermission::ViewFinancialInfo,
    )
    .await?;
    let sees_all = role.has_permission(CirclePermission::ManagePayouts);

    let mut count = QueryBuilder::<Postgres>::new(
        "SELECT COUNT(*)::BIGINT AS total, \
                COALESCE(SUM(amount) FILTER (WHERE status = 'confirmed'), 0)::BIGINT AS confirmed \
           FROM circle_contributions",
    );
    push_filters(&mut count, circle_id, viewer, sees_all, filter);
    let totals = count
        .build()
        .fetch_one(pool)
        .await
        .map_err(|_| "internal_error")?;

    let mut list = QueryBuilder::<Postgres>::new(format!(
        "SELECT {CONTRIBUTION_COLUMNS} FROM circle_contributions"
    ));
    push_filters(&mut list, circle_id, viewer, sees_all, filter);
    // Stable order: records never shuffle between pages.
    list.push(" ORDER BY created_at DESC, id DESC LIMIT ")
        .push_bind(filter.per_page)
        .push(" OFFSET ")
        .push_bind((filter.page - 1) * filter.per_page);
    let rows = list
        .build()
        .fetch_all(pool)
        .await
        .map_err(|_| "internal_error")?;

    Ok(HistoryPage {
        contributions: rows
            .iter()
            .map(row_to_contribution)
            .collect::<Result<_, _>>()
            .map_err(|_| "internal_error")?,
        total: totals.try_get("total").map_err(|_| "internal_error")?,
        confirmed_total: totals
            .try_get::<i64, _>("confirmed")
            .map_err(|_| "internal_error")?
            .to_string(),
        page: filter.page,
        per_page: filter.per_page,
    })
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

    #[test]
    fn history_filters_are_validated() {
        let q = |f: fn(&mut HistoryQuery)| {
            let mut q = HistoryQuery::default();
            f(&mut q);
            HistoryFilter::from_query(q)
        };
        let ok = q(|q| {
            q.member = Some(" GMEMBER ".into());
            q.from = Some("2026-12-01T00:00:00Z".into());
            q.to = Some("2026-12-31T23:59:59+01:00".into());
            q.asset = Some("XLM".into());
            q.status = Some("Confirmed".into());
            q.page = Some(2);
            q.per_page = Some(50);
        })
        .unwrap();
        assert_eq!(ok.member.as_deref(), Some("GMEMBER"));
        assert_eq!(ok.asset.as_deref(), Some("native"));
        assert_eq!(ok.status.as_deref(), Some("confirmed"));
        assert_eq!((ok.page, ok.per_page), (2, 50));

        let defaults = q(|_| {}).unwrap();
        assert_eq!((defaults.page, defaults.per_page), (1, DEFAULT_PER_PAGE));

        assert_eq!(
            q(|q| q.from = Some("yesterday".into())),
            Err("invalid_date")
        );
        assert_eq!(
            q(|q| {
                q.from = Some("2026-12-02T00:00:00Z".into());
                q.to = Some("2026-12-01T00:00:00Z".into());
            }),
            Err("invalid_date_range")
        );
        assert_eq!(q(|q| q.status = Some("done".into())), Err("invalid_status"));
        assert_eq!(q(|q| q.asset = Some("USDC".into())), Err("invalid_asset"));
        assert_eq!(q(|q| q.member = Some("a b".into())), Err("invalid_member"));
        assert_eq!(q(|q| q.page = Some(0)), Err("invalid_page"));
        assert_eq!(q(|q| q.per_page = Some(101)), Err("invalid_page"));
        assert_eq!(q(|q| q.per_page = Some(0)), Err("invalid_page"));
    }
}
