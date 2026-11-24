//! #1203 — Circle contribution rules engine.
//!
//! A circle has one *current* rule (the `circle_contribution_rules` row that
//! the circle detail view and the reminder scheduler already read). Every
//! change bumps `version` and appends an immutable snapshot to
//! `circle_contribution_rule_versions`, so history is never rewritten.
//! Contributions pin the version they were initiated under, which means a rule
//! change never alters an obligation that already exists.
//!
//! Validation ([`RuleSpec::validated`]) and evaluation
//! ([`evaluate_contribution`]) are pure functions; persistence lives at the
//! bottom of the file.

use crate::services::circle_goals::MAX_TARGET_AMOUNT;
use crate::services::circle_membership::{self, CirclePermission};
use crate::services::circle_scheduler::calculate_cycle_window;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sqlx::postgres::PgRow;
use sqlx::{PgPool, Row};
use std::collections::BTreeMap;
use uuid::Uuid;

pub const FREQUENCIES: &[&str] = &["none", "daily", "weekly", "monthly"];
pub const MAX_ALLOWED_ASSETS: usize = 10;
pub const MAX_MEMBER_OVERRIDES: usize = 200;
pub const MAX_GRACE_PERIOD_HOURS: i32 = 24 * 30;
/// Asset used when a rule does not list any.
pub const DEFAULT_ASSET: &str = "native";

// ── Amount (de)serialisation ──────────────────────────────────────────────────
// Amounts are stroops. They are written as strings (like the rest of the
// circles API) and read from either a string or an integer.

#[derive(Deserialize)]
#[serde(untagged)]
enum NumOrStr {
    Num(i64),
    Str(String),
}

impl NumOrStr {
    fn into_i64<E: serde::de::Error>(self) -> Result<i64, E> {
        match self {
            Self::Num(n) => Ok(n),
            Self::Str(s) => s
                .trim()
                .parse()
                .map_err(|_| E::custom("amount must be an integer number of stroops")),
        }
    }
}

mod amount_str {
    use super::NumOrStr;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(v: &i64, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&v.to_string())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<i64, D::Error> {
        NumOrStr::deserialize(d)?.into_i64()
    }

    pub mod opt {
        use super::NumOrStr;
        use serde::{Deserialize, Deserializer, Serializer};

        pub fn serialize<S: Serializer>(v: &Option<i64>, s: S) -> Result<S::Ok, S::Error> {
            match v {
                Some(v) => s.serialize_str(&v.to_string()),
                None => s.serialize_none(),
            }
        }

        pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<i64>, D::Error> {
            Option::<NumOrStr>::deserialize(d)?
                .map(NumOrStr::into_i64)
                .transpose()
        }
    }
}

// ── Model ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LatePolicy {
    /// Late contributions are accepted and flagged as late.
    #[default]
    Accept,
    /// Contributions after the deadline (plus grace period) are refused.
    Reject,
    /// Late contributions are accepted but must include a penalty on top of
    /// the required amount (`late_penalty_bps` of the fixed / minimum amount).
    Penalty,
}

impl LatePolicy {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Accept => "accept",
            Self::Reject => "reject",
            Self::Penalty => "penalty",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "accept" => Some(Self::Accept),
            "reject" => Some(Self::Reject),
            "penalty" => Some(Self::Penalty),
            _ => None,
        }
    }
}

/// Amount requirement for one member. When present in
/// [`RuleSpec::member_overrides`] it *replaces* the circle-wide amounts for
/// that member.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemberRequirement {
    #[serde(default, with = "amount_str::opt")]
    pub fixed_amount: Option<i64>,
    #[serde(default, with = "amount_str::opt")]
    pub min_amount: Option<i64>,
    #[serde(default, with = "amount_str::opt")]
    pub max_amount: Option<i64>,
}

/// One version of a circle's contribution rule.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuleSpec {
    /// Smallest accepted contribution (stroops).
    #[serde(with = "amount_str")]
    pub min_amount: i64,
    /// Largest accepted contribution.
    #[serde(default, with = "amount_str::opt")]
    pub max_amount: Option<i64>,
    /// When set, contributions must equal exactly this amount.
    #[serde(default, with = "amount_str::opt")]
    pub fixed_amount: Option<i64>,
    /// `none` | `daily` | `weekly` | `monthly`.
    pub frequency: String,
    pub is_mandatory: bool,
    /// Hard deadline; only for `frequency = none`.
    #[serde(default)]
    pub deadline: Option<DateTime<Utc>>,
    /// Recurring rules: hours after the start of each cycle by which the
    /// contribution is due (default: the end of the cycle).
    #[serde(default)]
    pub due_offset_hours: Option<i32>,
    /// Hours after the deadline during which a contribution is still on time.
    #[serde(default)]
    pub grace_period_hours: i32,
    #[serde(default)]
    pub late_policy: LatePolicy,
    /// Penalty in basis points; required when `late_policy = penalty`.
    #[serde(default)]
    pub late_penalty_bps: i32,
    /// Canonical asset ids (`native` or `CODE:ISSUER`). Empty means native only.
    #[serde(default)]
    pub allowed_assets: Vec<String>,
    /// Per-member amount requirements keyed by member address.
    #[serde(default)]
    pub member_overrides: BTreeMap<String, MemberRequirement>,
}

impl Default for RuleSpec {
    /// The rule that applies to a circle that never configured one.
    fn default() -> Self {
        Self {
            min_amount: 0,
            max_amount: None,
            fixed_amount: None,
            frequency: "none".to_string(),
            is_mandatory: false,
            deadline: None,
            due_offset_hours: None,
            grace_period_hours: 0,
            late_policy: LatePolicy::Accept,
            late_penalty_bps: 0,
            allowed_assets: Vec::new(),
            member_overrides: BTreeMap::new(),
        }
    }
}

/// The amount window a particular member has to respect.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Requirement {
    #[serde(with = "amount_str::opt")]
    pub fixed_amount: Option<i64>,
    #[serde(with = "amount_str")]
    pub min_amount: i64,
    #[serde(with = "amount_str::opt")]
    pub max_amount: Option<i64>,
}

/// Result of checking a contribution against a rule.
#[derive(Debug, Clone, PartialEq)]
pub struct Evaluation {
    pub is_late: bool,
    /// Part of the amount that is the late penalty (0 unless penalised).
    pub late_fee: i64,
    pub due_at: Option<DateTime<Utc>>,
    /// Cycle identifier for recurring rules, e.g. `weekly_2026_w49`.
    pub cycle: Option<String>,
}

// ── Assets ────────────────────────────────────────────────────────────────────

/// Normalise an asset id to `native` or `CODE:ISSUER`. Returns `None` when it
/// is neither (e.g. malformed, or a bare Soroban contract id, which cannot be
/// matched against Horizon operations).
pub fn normalize_asset(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.eq_ignore_ascii_case("native") || raw.eq_ignore_ascii_case("xlm") {
        return Some(DEFAULT_ASSET.to_string());
    }
    let (code, issuer) = raw.split_once(':')?;
    let code_ok = (1..=12).contains(&code.len()) && code.bytes().all(|b| b.is_ascii_alphanumeric());
    let issuer_ok = issuer.len() == 56
        && issuer.starts_with('G')
        && issuer
            .bytes()
            .all(|b| matches!(b, b'A'..=b'Z' | b'2'..=b'7'));
    (code_ok && issuer_ok).then(|| format!("{code}:{issuer}"))
}

// ── Validation ────────────────────────────────────────────────────────────────

fn validate_amounts(fixed: Option<i64>, min: i64, max: Option<i64>) -> Result<(), &'static str> {
    if !(0..=MAX_TARGET_AMOUNT).contains(&min) {
        return Err("invalid_amount");
    }
    if let Some(max) = max {
        if max <= 0 || max > MAX_TARGET_AMOUNT || max < min {
            return Err("invalid_amount");
        }
    }
    if let Some(fixed) = fixed {
        if fixed <= 0 || fixed > MAX_TARGET_AMOUNT || fixed < min || max.is_some_and(|m| fixed > m)
        {
            return Err("invalid_amount");
        }
    }
    Ok(())
}

fn cycle_hours(frequency: &str) -> Option<i32> {
    match frequency {
        "daily" => Some(24),
        "weekly" => Some(24 * 7),
        "monthly" => Some(24 * 31),
        _ => None,
    }
}

impl RuleSpec {
    /// Check the rule and return it in canonical form (lower-case frequency,
    /// normalised and de-duplicated assets). Errors are stable API codes.
    pub fn validated(mut self) -> Result<Self, &'static str> {
        self.frequency = self.frequency.trim().to_lowercase();
        if !FREQUENCIES.contains(&self.frequency.as_str()) {
            return Err("invalid_frequency");
        }
        validate_amounts(self.fixed_amount, self.min_amount, self.max_amount)?;

        match (
            cycle_hours(&self.frequency),
            self.deadline,
            self.due_offset_hours,
        ) {
            // One-off rules may carry a deadline but have no cycle to offset.
            (None, _, Some(_)) => return Err("invalid_due_offset"),
            // Recurring rules derive deadlines from their cycle.
            (Some(_), Some(_), _) => return Err("invalid_deadline"),
            (Some(cycle), _, Some(offset)) if !(0..cycle).contains(&offset) => {
                return Err("invalid_due_offset")
            }
            _ => {}
        }
        if !(0..=MAX_GRACE_PERIOD_HOURS).contains(&self.grace_period_hours) {
            return Err("invalid_grace_period");
        }
        let penalty_ok = match self.late_policy {
            LatePolicy::Penalty => (1..=10_000).contains(&self.late_penalty_bps),
            _ => self.late_penalty_bps == 0,
        };
        if !penalty_ok {
            return Err("invalid_late_penalty");
        }

        let mut assets: Vec<String> = Vec::new();
        for raw in &self.allowed_assets {
            let asset = normalize_asset(raw).ok_or("invalid_asset")?;
            if !assets.contains(&asset) {
                assets.push(asset);
            }
        }
        if assets.len() > MAX_ALLOWED_ASSETS {
            return Err("too_many_assets");
        }
        self.allowed_assets = assets;

        if self.member_overrides.len() > MAX_MEMBER_OVERRIDES {
            return Err("too_many_member_overrides");
        }
        for (member, req) in &self.member_overrides {
            if member.is_empty() || member.len() > 128 || member.chars().any(char::is_whitespace) {
                return Err("invalid_member_override");
            }
            validate_amounts(
                req.fixed_amount,
                req.min_amount.unwrap_or(0),
                req.max_amount,
            )
            .map_err(|_| "invalid_member_override")?;
        }
        Ok(self)
    }

    /// Assets a contribution may be paid in.
    pub fn effective_assets(&self) -> Vec<String> {
        if self.allowed_assets.is_empty() {
            vec![DEFAULT_ASSET.to_string()]
        } else {
            self.allowed_assets.clone()
        }
    }

    /// Amount window for `member`: their override if any, else the circle-wide one.
    pub fn requirement_for(&self, member: &str) -> Requirement {
        match self.member_overrides.get(member) {
            Some(o) => Requirement {
                fixed_amount: o.fixed_amount,
                min_amount: o.min_amount.unwrap_or(0),
                max_amount: o.max_amount,
            },
            None => Requirement {
                fixed_amount: self.fixed_amount,
                min_amount: self.min_amount,
                max_amount: self.max_amount,
            },
        }
    }

    /// Copy of the rule in which `member` only sees their own override.
    pub fn redacted_for(&self, member: &str) -> Self {
        let mut spec = self.clone();
        spec.member_overrides.retain(|address, _| address == member);
        spec
    }

    /// Deadline of the obligation that applies at `now`, with its cycle id.
    pub fn current_due(&self, now: DateTime<Utc>) -> Option<(Option<String>, DateTime<Utc>)> {
        if self.frequency == "none" {
            return self.deadline.map(|d| (None, d));
        }
        let (start, end, cycle) = calculate_cycle_window(&self.frequency, now)?;
        let due = match self.due_offset_hours {
            Some(hours) => start + Duration::hours(i64::from(hours)),
            None => end,
        };
        Some((Some(cycle), due))
    }
}

// ── Evaluation ────────────────────────────────────────────────────────────────

/// Check a contribution request against `spec`. Pure: the caller supplies the
/// clock. Errors are stable API codes.
pub fn evaluate_contribution(
    spec: &RuleSpec,
    member: &str,
    amount: i64,
    asset: &str,
    now: DateTime<Utc>,
) -> Result<Evaluation, &'static str> {
    if amount <= 0 || amount > MAX_TARGET_AMOUNT {
        return Err("invalid_amount");
    }
    let asset = normalize_asset(asset).ok_or("invalid_asset")?;
    if !spec.effective_assets().contains(&asset) {
        return Err("asset_not_supported");
    }

    let req = spec.requirement_for(member);
    let due = spec.current_due(now);
    let is_late = due.as_ref().is_some_and(|(_, due_at)| {
        now > *due_at + Duration::hours(i64::from(spec.grace_period_hours))
    });

    let mut late_fee = 0;
    if is_late {
        match spec.late_policy {
            LatePolicy::Reject => return Err("contribution_deadline_passed"),
            LatePolicy::Accept => {}
            LatePolicy::Penalty => {
                let base = req.fixed_amount.unwrap_or(req.min_amount);
                late_fee = (i128::from(base) * i128::from(spec.late_penalty_bps) / 10_000) as i64;
            }
        }
    }

    match req.fixed_amount {
        Some(fixed) => {
            if amount != fixed + late_fee {
                return Err("amount_not_fixed");
            }
        }
        None => {
            if req.max_amount.is_some_and(|max| amount > max) {
                return Err("amount_above_maximum");
            }
            if amount < req.min_amount + late_fee {
                return Err("amount_below_minimum");
            }
        }
    }

    Ok(Evaluation {
        is_late,
        late_fee,
        due_at: due.as_ref().map(|(_, d)| *d),
        cycle: due.and_then(|(c, _)| c),
    })
}

// ── Persistence ───────────────────────────────────────────────────────────────

/// The circle's current rule together with its version.
#[derive(Debug, Clone, Serialize)]
pub struct StoredRule {
    pub rule_id: Uuid,
    pub circle_id: Uuid,
    pub version: i32,
    pub rule: RuleSpec,
    pub updated_by: Option<String>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuleVersion {
    pub version: i32,
    pub rule: RuleSpec,
    pub created_by: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// What `GET /contribution-rules` returns to a member.
#[derive(Debug, Serialize)]
pub struct RulesView {
    pub circle_id: Uuid,
    /// 0 when the circle never configured a rule.
    pub version: i32,
    pub rule: Option<RuleSpec>,
    /// What *this* member has to respect under the current rule.
    pub your_requirement: Requirement,
    pub your_assets: Vec<String>,
    pub current_due_at: Option<DateTime<Utc>>,
    pub updated_by: Option<String>,
    pub updated_at: Option<DateTime<Utc>>,
}

const RULE_COLUMNS: &str = "id, circle_id, version, min_amount, max_amount, fixed_amount, \
    frequency, is_mandatory, deadline, due_offset_hours, grace_period_hours, late_policy, \
    late_penalty_bps, allowed_assets, member_overrides, updated_by, updated_at";

fn decode_err<E: std::error::Error + Send + Sync + 'static>(e: E) -> sqlx::Error {
    sqlx::Error::Decode(Box::new(e))
}

fn row_to_rule(row: &PgRow) -> Result<StoredRule, sqlx::Error> {
    let late_policy: String = row.try_get("late_policy")?;
    let late_policy = LatePolicy::parse(&late_policy).ok_or_else(|| {
        decode_err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "unknown late_policy",
        ))
    })?;
    let overrides: serde_json::Value = row.try_get("member_overrides")?;
    Ok(StoredRule {
        rule_id: row.try_get("id")?,
        circle_id: row.try_get("circle_id")?,
        version: row.try_get("version")?,
        rule: RuleSpec {
            min_amount: row.try_get("min_amount")?,
            max_amount: row.try_get("max_amount")?,
            fixed_amount: row.try_get("fixed_amount")?,
            frequency: row.try_get("frequency")?,
            is_mandatory: row.try_get("is_mandatory")?,
            deadline: row.try_get("deadline")?,
            due_offset_hours: row.try_get("due_offset_hours")?,
            grace_period_hours: row.try_get("grace_period_hours")?,
            late_policy,
            late_penalty_bps: row.try_get("late_penalty_bps")?,
            allowed_assets: row.try_get("allowed_assets")?,
            member_overrides: serde_json::from_value(overrides).map_err(decode_err)?,
        },
        updated_by: row.try_get("updated_by")?,
        updated_at: row.try_get("updated_at")?,
    })
}

/// The circle's current rule, if it has one.
pub async fn load_active(
    pool: &PgPool,
    circle_id: Uuid,
) -> Result<Option<StoredRule>, sqlx::Error> {
    let sql = format!(
        "SELECT {RULE_COLUMNS} FROM circle_contribution_rules \
         WHERE circle_id = $1 ORDER BY created_at DESC LIMIT 1"
    );
    sqlx::query(&sql)
        .bind(circle_id)
        .fetch_optional(pool)
        .await?
        .as_ref()
        .map(row_to_rule)
        .transpose()
}

/// A member's view of the current rule (`ViewFinancialInfo`). Only owners and
/// admins see other members' overrides.
pub async fn get_rules(
    pool: &PgPool,
    circle_id: Uuid,
    actor: &str,
    now: DateTime<Utc>,
) -> Result<RulesView, &'static str> {
    let role = circle_membership::require_circle_permission(
        pool,
        circle_id,
        actor,
        CirclePermission::ViewFinancialInfo,
    )
    .await?;
    let stored = load_active(pool, circle_id)
        .await
        .map_err(|_| "internal_error")?;
    let spec = stored.as_ref().map(|s| s.rule.clone()).unwrap_or_default();
    let manager = role.has_permission(CirclePermission::ManageSettings);
    Ok(RulesView {
        circle_id,
        version: stored.as_ref().map_or(0, |s| s.version),
        your_requirement: spec.requirement_for(actor),
        your_assets: spec.effective_assets(),
        current_due_at: spec.current_due(now).map(|(_, due)| due),
        updated_by: stored.as_ref().and_then(|s| s.updated_by.clone()),
        updated_at: stored.as_ref().map(|s| s.updated_at),
        rule: stored.map(|s| {
            if manager {
                s.rule
            } else {
                s.rule.redacted_for(actor)
            }
        }),
    })
}

/// Replace the circle's rule with `spec` as a new version
/// (`ManageSettings`). Re-submitting the current rule is a no-op, so a retry
/// never burns a version number; the flag tells whether a version was written.
pub async fn update_rules(
    pool: &PgPool,
    circle_id: Uuid,
    actor: &str,
    spec: RuleSpec,
) -> Result<(StoredRule, bool), &'static str> {
    circle_membership::require_circle_permission(
        pool,
        circle_id,
        actor,
        CirclePermission::ManageSettings,
    )
    .await?;
    let spec = spec.validated()?;
    let overrides = serde_json::to_value(&spec.member_overrides).map_err(|_| "internal_error")?;
    let snapshot = serde_json::to_value(&spec).map_err(|_| "internal_error")?;

    let mut tx = pool.begin().await.map_err(|_| "internal_error")?;
    // Serialise version bumps per circle.
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))")
        .bind(circle_id.to_string())
        .execute(&mut *tx)
        .await
        .map_err(|_| "internal_error")?;

    let select = format!(
        "SELECT {RULE_COLUMNS} FROM circle_contribution_rules \
         WHERE circle_id = $1 ORDER BY created_at DESC LIMIT 1 FOR UPDATE"
    );
    let current = sqlx::query(&select)
        .bind(circle_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| "internal_error")?
        .as_ref()
        .map(row_to_rule)
        .transpose()
        .map_err(|_| "internal_error")?;

    if let Some(cur) = &current {
        if cur.rule == spec {
            return Ok((cur.clone(), false));
        }
    }

    let (rule_id, version) = match &current {
        Some(cur) => (cur.rule_id, cur.version + 1),
        None => (Uuid::new_v4(), 1),
    };
    let values = if current.is_some() {
        r#"UPDATE circle_contribution_rules
              SET version = $2, min_amount = $3, max_amount = $4, fixed_amount = $5,
                  frequency = $6, is_mandatory = $7, deadline = $8, due_offset_hours = $9,
                  grace_period_hours = $10, late_policy = $11, late_penalty_bps = $12,
                  allowed_assets = $13, member_overrides = $14, updated_by = $15,
                  updated_at = NOW()
            WHERE id = $1"#
    } else {
        r#"INSERT INTO circle_contribution_rules
               (id, version, min_amount, max_amount, fixed_amount, frequency, is_mandatory,
                deadline, due_offset_hours, grace_period_hours, late_policy,
                late_penalty_bps, allowed_assets, member_overrides, updated_by, circle_id)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)"#
    };
    let mut write = sqlx::query(values)
        .bind(rule_id)
        .bind(version)
        .bind(spec.min_amount)
        .bind(spec.max_amount)
        .bind(spec.fixed_amount)
        .bind(&spec.frequency)
        .bind(spec.is_mandatory)
        .bind(spec.deadline)
        .bind(spec.due_offset_hours)
        .bind(spec.grace_period_hours)
        .bind(spec.late_policy.as_str())
        .bind(spec.late_penalty_bps)
        .bind(&spec.allowed_assets)
        .bind(&overrides)
        .bind(actor);
    if current.is_none() {
        write = write.bind(circle_id);
    }
    write
        .execute(&mut *tx)
        .await
        .map_err(|_| "internal_error")?;

    sqlx::query(
        "INSERT INTO circle_contribution_rule_versions \
             (rule_id, circle_id, version, spec, created_by) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(rule_id)
    .bind(circle_id)
    .bind(version)
    .bind(&snapshot)
    .bind(actor)
    .execute(&mut *tx)
    .await
    .map_err(|_| "internal_error")?;
    tx.commit().await.map_err(|_| "internal_error")?;

    let stored = load_active(pool, circle_id)
        .await
        .map_err(|_| "internal_error")?
        .ok_or("internal_error")?;
    Ok((stored, true))
}

/// Every version of the circle's rule, newest first (`ViewFinancialInfo`).
pub async fn list_versions(
    pool: &PgPool,
    circle_id: Uuid,
    actor: &str,
) -> Result<Vec<RuleVersion>, &'static str> {
    let role = circle_membership::require_circle_permission(
        pool,
        circle_id,
        actor,
        CirclePermission::ViewFinancialInfo,
    )
    .await?;
    let manager = role.has_permission(CirclePermission::ManageSettings);
    let rows = sqlx::query(
        "SELECT version, spec, created_by, created_at \
           FROM circle_contribution_rule_versions \
          WHERE circle_id = $1 ORDER BY version DESC",
    )
    .bind(circle_id)
    .fetch_all(pool)
    .await
    .map_err(|_| "internal_error")?;

    rows.iter()
        .map(|row| {
            let spec: serde_json::Value = row.try_get("spec")?;
            let rule: RuleSpec = serde_json::from_value(spec).map_err(decode_err)?;
            Ok(RuleVersion {
                version: row.try_get("version")?,
                rule: if manager {
                    rule
                } else {
                    rule.redacted_for(actor)
                },
                created_by: row.try_get("created_by")?,
                created_at: row.try_get("created_at")?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()
        .map_err(|_| "internal_error")
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use serde_json::json;

    const ISSUER: &str = "GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN";
    const USDC: &str = "USDC:GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN";

    fn spec() -> RuleSpec {
        RuleSpec::default()
    }

    /// Wednesday 2026-12-02 12:00 UTC (ISO week 49: Mon 11-30 .. Sun 12-06).
    fn wednesday() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 12, 2, 12, 0, 0).unwrap()
    }

    #[test]
    fn legacy_payload_deserializes_and_amounts_round_trip_as_strings() {
        let legacy = json!({
            "min_amount": "1000", "max_amount": "5000",
            "frequency": "weekly", "is_mandatory": true
        });
        let rule: RuleSpec = serde_json::from_value(legacy).unwrap();
        assert_eq!(rule.min_amount, 1000);
        assert_eq!(rule.max_amount, Some(5000));
        assert_eq!(rule.late_policy, LatePolicy::Accept);
        assert!(rule.allowed_assets.is_empty());

        let out = serde_json::to_value(&rule).unwrap();
        assert_eq!(out["min_amount"], "1000");
        assert_eq!(out["max_amount"], "5000");
        assert!(out["fixed_amount"].is_null());
        // Integers are accepted on input too; unknown fields are not.
        assert!(serde_json::from_value::<RuleSpec>(
            json!({"min_amount": 5, "frequency": "none", "is_mandatory": false})
        )
        .is_ok());
        assert!(serde_json::from_value::<RuleSpec>(
            json!({"min_amount": "5", "frequency": "none", "is_mandatory": false, "bogus": 1})
        )
        .is_err());
    }

    #[test]
    fn validation_rejects_inconsistent_rules() {
        let check = |f: fn(&mut RuleSpec)| {
            let mut s = spec();
            f(&mut s);
            s.validated().map(|_| ()).unwrap_err()
        };
        assert_eq!(check(|s| s.min_amount = -1), "invalid_amount");
        assert_eq!(
            check(|s| {
                s.min_amount = 10;
                s.max_amount = Some(5)
            }),
            "invalid_amount"
        );
        assert_eq!(
            check(|s| {
                s.max_amount = Some(5);
                s.fixed_amount = Some(6)
            }),
            "invalid_amount"
        );
        assert_eq!(
            check(|s| s.frequency = "hourly".into()),
            "invalid_frequency"
        );
        assert_eq!(
            check(|s| s.due_offset_hours = Some(3)),
            "invalid_due_offset"
        );
        assert_eq!(
            check(|s| {
                s.frequency = "daily".into();
                s.due_offset_hours = Some(24)
            }),
            "invalid_due_offset"
        );
        assert_eq!(
            check(|s| {
                s.frequency = "weekly".into();
                s.deadline = Some(wednesday())
            }),
            "invalid_deadline"
        );
        assert_eq!(
            check(|s| s.late_policy = LatePolicy::Penalty),
            "invalid_late_penalty"
        );
        assert_eq!(check(|s| s.late_penalty_bps = 500), "invalid_late_penalty");
        assert_eq!(
            check(|s| s.allowed_assets = vec!["USDC".into()]),
            "invalid_asset"
        );
        assert_eq!(
            check(|s| {
                s.member_overrides.insert(
                    "GMEMBER".into(),
                    MemberRequirement {
                        min_amount: Some(9),
                        max_amount: Some(3),
                        ..Default::default()
                    },
                );
            }),
            "invalid_member_override"
        );
    }

    #[test]
    fn validation_canonicalises_frequency_and_assets() {
        let mut s = spec();
        s.frequency = " Weekly ".into();
        s.allowed_assets = vec!["XLM".into(), "native".into(), USDC.into()];
        let s = s.validated().unwrap();
        assert_eq!(s.frequency, "weekly");
        assert_eq!(
            s.allowed_assets,
            vec!["native".to_string(), USDC.to_string()]
        );
        // Asset codes are case-sensitive on Stellar; malformed ids are rejected.
        assert_eq!(
            normalize_asset(&format!("usdc:{ISSUER}")),
            Some(format!("usdc:{ISSUER}"))
        );
        assert_eq!(normalize_asset("USDC:GSHORT"), None);
        assert_eq!(
            normalize_asset("CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC"),
            None
        );
        assert_eq!(normalize_asset("native").as_deref(), Some("native"));
    }

    #[test]
    fn amount_rules_fixed_min_max() {
        let now = wednesday();
        let mut s = spec();
        s.min_amount = 100;
        s.max_amount = Some(500);
        assert!(evaluate_contribution(&s, "GM", 100, "native", now).is_ok());
        assert!(evaluate_contribution(&s, "GM", 500, "native", now).is_ok());
        assert_eq!(
            evaluate_contribution(&s, "GM", 99, "native", now).unwrap_err(),
            "amount_below_minimum"
        );
        assert_eq!(
            evaluate_contribution(&s, "GM", 501, "native", now).unwrap_err(),
            "amount_above_maximum"
        );
        assert_eq!(
            evaluate_contribution(&s, "GM", 0, "native", now).unwrap_err(),
            "invalid_amount"
        );

        s.fixed_amount = Some(250);
        assert!(evaluate_contribution(&s, "GM", 250, "native", now).is_ok());
        assert_eq!(
            evaluate_contribution(&s, "GM", 251, "native", now).unwrap_err(),
            "amount_not_fixed"
        );
    }

    #[test]
    fn member_override_replaces_the_circle_wide_amounts() {
        let now = wednesday();
        let mut s = spec();
        s.fixed_amount = Some(100);
        s.member_overrides.insert(
            "GVIP".into(),
            MemberRequirement {
                fixed_amount: Some(400),
                ..Default::default()
            },
        );
        assert!(evaluate_contribution(&s, "GVIP", 400, "native", now).is_ok());
        assert!(evaluate_contribution(&s, "GVIP", 100, "native", now).is_err());
        assert!(evaluate_contribution(&s, "GOTHER", 100, "native", now).is_ok());
        assert_eq!(s.requirement_for("GVIP").fixed_amount, Some(400));

        // Regular members never see someone else's requirement.
        let seen = s.redacted_for("GOTHER");
        assert!(seen.member_overrides.is_empty());
        assert_eq!(s.redacted_for("GVIP").member_overrides.len(), 1);
    }

    #[test]
    fn asset_must_be_allowed_and_defaults_to_native() {
        let now = wednesday();
        let mut s = spec();
        assert!(evaluate_contribution(&s, "GM", 10, "native", now).is_ok());
        assert_eq!(
            evaluate_contribution(&s, "GM", 10, USDC, now).unwrap_err(),
            "asset_not_supported"
        );
        assert_eq!(
            evaluate_contribution(&s, "GM", 10, "garbage", now).unwrap_err(),
            "invalid_asset"
        );
        s.allowed_assets = vec![USDC.to_string()];
        assert!(evaluate_contribution(&s, "GM", 10, USDC, now).is_ok());
        assert_eq!(
            evaluate_contribution(&s, "GM", 10, "native", now).unwrap_err(),
            "asset_not_supported"
        );
    }

    #[test]
    fn late_policy_reject_accept_and_penalty() {
        // One-off rule, deadline a day before "now", 12 hour grace.
        let now = wednesday();
        let mut s = spec();
        s.deadline = Some(now - Duration::hours(24));
        s.grace_period_hours = 12;
        s.min_amount = 1_000;

        s.late_policy = LatePolicy::Reject;
        assert_eq!(
            evaluate_contribution(&s, "GM", 1_000, "native", now).unwrap_err(),
            "contribution_deadline_passed"
        );

        s.late_policy = LatePolicy::Accept;
        let ok = evaluate_contribution(&s, "GM", 1_000, "native", now).unwrap();
        assert!(ok.is_late);
        assert_eq!(ok.late_fee, 0);

        s.late_policy = LatePolicy::Penalty;
        s.late_penalty_bps = 500; // 5%
        assert_eq!(
            evaluate_contribution(&s, "GM", 1_000, "native", now).unwrap_err(),
            "amount_below_minimum"
        );
        let ok = evaluate_contribution(&s, "GM", 1_050, "native", now).unwrap();
        assert_eq!((ok.is_late, ok.late_fee), (true, 50));

        // Still inside the grace window: on time, no penalty.
        s.deadline = Some(now - Duration::hours(6));
        let ok = evaluate_contribution(&s, "GM", 1_000, "native", now).unwrap();
        assert_eq!((ok.is_late, ok.late_fee), (false, 0));
    }

    #[test]
    fn recurring_rules_use_their_cycle_for_deadlines() {
        let now = wednesday();
        let mut s = spec();
        s.frequency = "weekly".into();
        s.late_policy = LatePolicy::Reject;

        // Default: due at the end of the cycle, so never late inside it.
        let ok = evaluate_contribution(&s, "GM", 10, "native", now).unwrap();
        assert!(!ok.is_late);
        assert_eq!(ok.cycle.as_deref(), Some("weekly_2026_w49"));

        // Due 24h after the cycle starts (Monday 00:00 -> Tuesday 00:00).
        s.due_offset_hours = Some(24);
        assert_eq!(
            evaluate_contribution(&s, "GM", 10, "native", now).unwrap_err(),
            "contribution_deadline_passed"
        );
        let monday = Utc.with_ymd_and_hms(2026, 11, 30, 8, 0, 0).unwrap();
        let ok = evaluate_contribution(&s, "GM", 10, "native", monday).unwrap();
        assert_eq!(
            ok.due_at,
            Some(Utc.with_ymd_and_hms(2026, 12, 1, 0, 0, 0).unwrap())
        );
    }
}
