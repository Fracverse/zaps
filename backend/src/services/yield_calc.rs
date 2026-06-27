use chrono::{DateTime, NaiveDateTime, Utc};
use crate::db::models::UserYieldBalance;

/// Stellar mainnet/testnet approximate ledger cadence (~5 seconds).
pub const SECONDS_PER_YEAR: i64 = 31_536_000;
const DEFAULT_APY_BPS: i32 = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct YieldEstimate {
    /// Interest accrued since the last sync point (micro-units).
    pub accrued_interest: i64,
    /// Earning balance including accrued but not yet checkpointed interest.
    pub total_earning_balance: i64,
}

/// Linear off-chain yield estimate from the user's earning balance, APY, and
/// elapsed time since the last blockchain/indexer sync.
pub fn estimate_accrued_yield(
    earning_balance: i64,
    apy_bps: i32,
    last_sync_at: NaiveDateTime,
    now: DateTime<Utc>,
) -> YieldEstimate {
    if earning_balance <= 0 || apy_bps <= 0 {
        return YieldEstimate {
            accrued_interest: 0,
            total_earning_balance: earning_balance.max(0),
        };
    }

    let sync_utc = last_sync_at.and_utc();
    let elapsed_secs = (now - sync_utc).num_seconds().max(0);

    let accrued = earning_balance
        .saturating_mul(apy_bps as i64)
        .saturating_mul(elapsed_secs)
        / (10_000 * SECONDS_PER_YEAR);

    YieldEstimate {
        accrued_interest: accrued,
        total_earning_balance: earning_balance.saturating_add(accrued),
    }
}

/// Convenience wrapper for API handlers and background jobs.
pub fn estimate_for_balance(
    balance: &UserYieldBalance,
    apy_bps: Option<i32>,
    now: DateTime<Utc>,
) -> YieldEstimate {
    estimate_accrued_yield(
        balance.earning_balance,
        apy_bps.unwrap_or(DEFAULT_APY_BPS),
        balance.last_yield_sync_at,
        now,
    )
}

/// Helper for feed-style responses that surface live yield totals.
pub fn format_yield_feed_fields(
    balance: &UserYieldBalance,
    apy_bps: Option<i32>,
) -> (i64, i64, f64) {
    let estimate = estimate_for_balance(balance, apy_bps, Utc::now());
    let apy_pct = apy_bps.unwrap_or(DEFAULT_APY_BPS) as f64 / 100.0;
    (
        estimate.accrued_interest,
        estimate.total_earning_balance,
        apy_pct,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use uuid::Uuid;

    fn sync_at(year: i32, month: u32, day: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(year, month, day)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap()
    }

    fn now_at(rfc3339: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(rfc3339)
            .unwrap()
            .with_timezone(&Utc)
    }

    // ── Issue #382: balance calculation unit tests ─────────────────────────

    #[test]
    fn zero_balance_yields_no_interest() {
        let est = estimate_accrued_yield(0, 500, sync_at(2026, 1, 1), now_at("2026-06-01T00:00:00Z"));
        assert_eq!(est.accrued_interest, 0);
        assert_eq!(est.total_earning_balance, 0);
    }

    #[test]
    fn linear_interest_scales_with_time_and_balance() {
        let sync = sync_at(2026, 6, 1);
        let now = now_at("2026-06-02T00:00:00Z");

        let est = estimate_accrued_yield(1_000_000, 500, sync, now);
        let expected = 1_000_000 * 500 * 86_400 / (10_000 * SECONDS_PER_YEAR);
        assert_eq!(est.accrued_interest, expected);
        assert_eq!(est.total_earning_balance, 1_000_000 + expected);
    }

    #[test]
    fn negative_elapsed_is_clamped_to_zero() {
        let est = estimate_accrued_yield(500_000, 500, sync_at(2026, 6, 2), now_at("2026-06-01T00:00:00Z"));
        assert_eq!(est.accrued_interest, 0);
        // total_earning_balance should equal earning_balance when no time has elapsed
        assert_eq!(est.total_earning_balance, 500_000);
    }

    #[test]
    fn zero_apy_yields_no_interest() {
        let est = estimate_accrued_yield(1_000_000, 0, sync_at(2026, 1, 1), now_at("2026-12-31T00:00:00Z"));
        assert_eq!(est.accrued_interest, 0);
        assert_eq!(est.total_earning_balance, 1_000_000);
    }

    #[test]
    fn negative_earning_balance_yields_zero() {
        let est = estimate_accrued_yield(-100, 500, sync_at(2026, 1, 1), now_at("2026-06-01T00:00:00Z"));
        assert_eq!(est.accrued_interest, 0);
        assert_eq!(est.total_earning_balance, 0);
    }

    #[test]
    fn full_year_at_5pct_apy_yields_correct_interest() {
        let sync = sync_at(2026, 1, 1);
        // Exactly one year later (non-leap year: 365 days = 31_536_000 s)
        let now = now_at("2027-01-01T00:00:00Z");
        let balance = 10_000_000_i64; // 10 XLM in micro-units

        let est = estimate_accrued_yield(balance, 500, sync, now);
        // 5% of 10_000_000 = 500_000
        let expected = balance * 500 * SECONDS_PER_YEAR / (10_000 * SECONDS_PER_YEAR);
        assert_eq!(est.accrued_interest, expected);
        assert_eq!(est.accrued_interest, 500_000);
        assert_eq!(est.total_earning_balance, 10_500_000);
    }

    #[test]
    fn high_apy_does_not_overflow_with_large_balance() {
        // 1_000_000_000 micro-units at 10000 bps (100% APY) for one full year.
        // The intermediate multiplication overflows i64, so saturating_mul kicks in;
        // the result will be less than the theoretical value but must not be negative
        // and must not panic.
        let balance = 1_000_000_000_i64;
        let now = now_at("2027-01-01T00:00:00Z");
        let est = estimate_accrued_yield(balance, 10_000, sync_at(2026, 1, 1), now);

        assert!(est.accrued_interest >= 0, "saturating_mul must not produce a negative result");
        assert!(
            est.total_earning_balance >= balance,
            "total must be at least the principal"
        );
    }

    #[test]
    fn very_large_balance_saturates_instead_of_wrapping() {
        // i64::MAX earning balance — saturating_mul must not panic or wrap
        let est = estimate_accrued_yield(i64::MAX, 10_000, sync_at(2026, 1, 1), now_at("2027-01-01T00:00:00Z"));
        // Result is implementation-defined on saturation, but must not panic and
        // total_earning_balance must be >= earning_balance (no wrap-around underflow).
        assert!(est.total_earning_balance >= 0);
    }

    #[test]
    fn interest_at_one_second_elapsed_is_minimal() {
        let balance = 1_000_000_i64;
        // Only 1 second has elapsed — interest should be at most 1 micro-unit
        let est = estimate_accrued_yield(balance, 500, sync_at(2026, 1, 1), now_at("2026-01-01T00:00:01Z"));
        assert!(est.accrued_interest <= 1);
        assert!(est.accrued_interest >= 0);
    }

    #[test]
    fn estimate_for_balance_uses_default_apy_when_none() {
        let balance = UserYieldBalance {
            user_id: Uuid::new_v4(),
            available_balance: 0,
            earning_balance: 1_000_000,
            last_yield_sync_at: sync_at(2026, 1, 1),
            updated_at: sync_at(2026, 1, 1),
        };
        let now = now_at("2026-06-01T00:00:00Z");

        let with_none = estimate_for_balance(&balance, None, now);
        let with_default = estimate_for_balance(&balance, Some(DEFAULT_APY_BPS), now);
        assert_eq!(with_none, with_default);
    }

    #[test]
    fn estimate_for_balance_explicit_apy_overrides_default() {
        let balance = UserYieldBalance {
            user_id: Uuid::new_v4(),
            available_balance: 0,
            earning_balance: 1_000_000,
            last_yield_sync_at: sync_at(2026, 1, 1),
            updated_at: sync_at(2026, 1, 1),
        };
        let now = now_at("2027-01-01T00:00:00Z");

        let at_5pct = estimate_for_balance(&balance, Some(500), now);
        let at_10pct = estimate_for_balance(&balance, Some(1000), now);
        // Higher APY must produce strictly more interest
        assert!(at_10pct.accrued_interest > at_5pct.accrued_interest);
    }

    #[test]
    fn interest_proportional_to_elapsed_time() {
        let balance = 1_000_000_i64;
        let sync = sync_at(2026, 1, 1);

        let half_year = estimate_accrued_yield(balance, 500, sync, now_at("2026-07-02T12:00:00Z"));
        let full_year = estimate_accrued_yield(balance, 500, sync, now_at("2027-01-01T00:00:00Z"));

        // Full year (31_536_000 s) is exactly 2× half year (15_768_000 s) — integer division
        // is exact here because both elapsed_secs divide SECONDS_PER_YEAR evenly.
        assert_eq!(full_year.accrued_interest, half_year.accrued_interest * 2);
    }

    #[test]
    fn interest_proportional_to_balance() {
        let sync = sync_at(2026, 1, 1);
        let now = now_at("2026-06-01T00:00:00Z");

        let small = estimate_accrued_yield(500_000, 500, sync, now);
        let large = estimate_accrued_yield(1_000_000, 500, sync, now);

        // Doubling balance must double interest (linear model)
        assert_eq!(large.accrued_interest, small.accrued_interest * 2);
    }

    #[test]
    fn sync_at_now_yields_zero_interest() {
        let now = now_at("2026-06-01T00:00:00Z");
        let sync = now.naive_utc();

        let est = estimate_accrued_yield(1_000_000, 500, sync, now);
        assert_eq!(est.accrued_interest, 0);
        assert_eq!(est.total_earning_balance, 1_000_000);
    }
}
