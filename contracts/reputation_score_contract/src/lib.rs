#![no_std]

//! # Reputation Score Contract
//!
//! ## Scoring Algorithm
//!
//! Each address starts with a neutral score of 500 (range 0–1000).
//!
//! ### Transaction Volume Scoring
//! Successful transactions contribute a volume-weighted delta:
//!
//!   delta = BASE_SUCCESS_DELTA + floor(volume / VOLUME_TIER)
//!
//! where `VOLUME_TIER` = 1 000 000 (stroops).  The per-tx bonus is capped at
//! `MAX_VOLUME_BONUS` (20 pts) so a single large tx cannot spike the score.
//!
//! ### Dispute Impact
//! Disputes are differentiated by outcome:
//!
//! | Outcome                  | Delta  |
//! |--------------------------|--------|
//! | Opened (unresolved)      | -20    |
//! | Resolved against user    | -50    |
//! | Resolved in user's favour| +15    |
//!
//! ### Time Decay (mean-reversion)
//! Applied lazily whenever a record is read or written.
//! For every `DECAY_PERIOD` ledgers elapsed since the last update,
//! the score moves 1 % closer to the neutral value of 500.
//!
//!   score = score + (500 - score) / 100   (per period, integer arithmetic)
//!
//! High scores drift down; low scores drift up — reputation must be
//! continuously earned.
//!
//! ### Score Breakdown
//! `calculate_score` returns a `ScoreBreakdown` with the current decayed
//! score plus the individual components that produced it, useful for
//! off-chain analytics and dispute UIs.
//!
//! ### Access Control
//! Only addresses in the reporter whitelist (set by admin) may record
//! transactions.  The admin manages the whitelist.

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, symbol_short, Address,
    Env,
};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Neutral / starting score.
pub const NEUTRAL: u32 = 500;
/// Maximum score.
pub const MAX_SCORE: u32 = 1_000;

// --- Volume scoring ---------------------------------------------------------
/// Base points added per successful transaction (before volume bonus).
pub const BASE_SUCCESS_DELTA: u32 = 5;
/// Volume tier in token base units (stroops).  Each full tier adds 1 bonus pt.
pub const VOLUME_TIER: i128 = 1_000_000;
/// Maximum volume bonus per transaction (caps runaway single-tx spikes).
pub const MAX_VOLUME_BONUS: u32 = 20;

// --- Dispute scoring --------------------------------------------------------
/// Points deducted when a dispute is opened (unresolved).
pub const DISPUTE_OPEN_DELTA: u32 = 20;
/// Additional points deducted when a dispute is resolved *against* the user
/// (total impact = DISPUTE_OPEN_DELTA + DISPUTE_LOSS_DELTA = 70).
pub const DISPUTE_LOSS_DELTA: u32 = 50;
/// Points *added* when a dispute is resolved *in the user's favour*
/// (partially offsets the initial -20 opening penalty).
pub const DISPUTE_WIN_DELTA: u32 = 15;

// --- Decay ------------------------------------------------------------------
/// Ledgers per decay period (~1 day at 5 s/ledger = 17 280 ledgers).
pub const DECAY_PERIOD: u32 = 17_280;

// --- Storage TTL ------------------------------------------------------------
const TTL_THRESHOLD: u32 = 100_000;
const TTL_EXTEND: u32 = 6_307_200;
const PERSISTENT_TTL_THRESHOLD: u32 = 50_000;
const PERSISTENT_TTL_EXTEND: u32 = 3_153_600;

// ---------------------------------------------------------------------------
// Storage keys
// ---------------------------------------------------------------------------

#[contracttype]
#[derive(Clone)]
enum Key {
    Admin,
    Reporter(Address),
    Record(Address),
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Full per-address reputation record stored on-chain.
#[contracttype]
#[derive(Clone)]
pub struct ReputationRecord {
    /// Current score (0–1000), updated on every write.
    pub score: u32,
    /// Ledger sequence of last write (used for decay).
    pub last_updated: u32,
    /// Cumulative successful transactions recorded.
    pub tx_success: u32,
    /// Cumulative disputed transactions opened.
    pub tx_disputed: u32,
    /// Cumulative volume (sum of all successful tx amounts, in base units).
    pub total_volume: i128,
    /// Disputes resolved in the user's favour.
    pub disputes_won: u32,
    /// Disputes resolved against the user.
    pub disputes_lost: u32,
}

/// Returned by `calculate_score` — the current score plus a breakdown of
/// how each component contributed.
#[contracttype]
#[derive(Clone)]
pub struct ScoreBreakdown {
    /// Final score after decay (0–1000).
    pub score: u32,
    /// Ledger periods elapsed since last write (decay periods applied).
    pub decay_periods: u32,
    /// Raw score before decay was applied.
    pub pre_decay_score: u32,
    /// Points contributed by successful transactions (volume-weighted).
    pub volume_contribution: i64,
    /// Points lost to dispute openings.
    pub dispute_open_impact: i64,
    /// Points lost to disputes resolved against the user.
    pub dispute_loss_impact: i64,
    /// Points recovered from disputes resolved in the user's favour.
    pub dispute_win_impact: i64,
    /// Cumulative successful transactions.
    pub tx_success: u32,
    /// Cumulative disputed transactions.
    pub tx_disputed: u32,
    /// Total volume processed.
    pub total_volume: i128,
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[contracterror]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum RepError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    ReporterAlreadyAdded = 4,
    ReporterNotFound = 5,
    InvalidVolume = 6,
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn require_admin(env: &Env) -> Address {
    let admin: Address = env
        .storage()
        .instance()
        .get(&Key::Admin)
        .unwrap_or_else(|| panic_with_error!(env, RepError::NotInitialized));
    admin.require_auth();
    admin
}

fn require_reporter(env: &Env, reporter: &Address) {
    reporter.require_auth();
    if !env
        .storage()
        .instance()
        .get::<Key, bool>(&Key::Reporter(reporter.clone()))
        .unwrap_or(false)
    {
        panic_with_error!(env, RepError::Unauthorized);
    }
    env.storage()
        .instance()
        .extend_ttl(TTL_THRESHOLD, TTL_EXTEND);
}

/// Compute the volume-weighted success delta for a single transaction.
///
///   delta = BASE_SUCCESS_DELTA + min(floor(volume / VOLUME_TIER), MAX_VOLUME_BONUS)
pub fn volume_delta(volume: i128) -> u32 {
    let bonus = (volume / VOLUME_TIER) as u32;
    BASE_SUCCESS_DELTA + bonus.min(MAX_VOLUME_BONUS)
}

/// Apply time-decay to a score: for each elapsed DECAY_PERIOD, move 1 % toward NEUTRAL.
///
/// Returns `(decayed_score, periods_elapsed)`.
pub fn apply_decay(score: u32, last_updated: u32, current_ledger: u32) -> (u32, u32) {
    if current_ledger <= last_updated {
        return (score, 0);
    }
    let periods = (current_ledger - last_updated) / DECAY_PERIOD;
    if periods == 0 {
        return (score, 0);
    }
    let mut s = score as i64;
    let neutral = NEUTRAL as i64;
    for _ in 0..periods {
        let delta = (neutral - s) / 100;
        s += delta;
        // If delta rounds to 0 but score ≠ neutral, nudge by 1 toward neutral
        // so the score always converges rather than stalling.
        if delta == 0 && s != neutral {
            s += if s < neutral { 1 } else { -1 };
        }
    }
    (s.clamp(0, MAX_SCORE as i64) as u32, periods)
}

fn load_record(env: &Env, user: &Address) -> ReputationRecord {
    env.storage()
        .persistent()
        .get(&Key::Record(user.clone()))
        .unwrap_or(ReputationRecord {
            score: NEUTRAL,
            last_updated: env.ledger().sequence(),
            tx_success: 0,
            tx_disputed: 0,
            total_volume: 0,
            disputes_won: 0,
            disputes_lost: 0,
        })
}

fn save_record(env: &Env, user: &Address, record: &ReputationRecord) {
    let key = Key::Record(user.clone());
    env.storage().persistent().set(&key, record);
    env.storage()
        .persistent()
        .extend_ttl(&key, PERSISTENT_TTL_THRESHOLD, PERSISTENT_TTL_EXTEND);
}

/// Load record, apply decay, update `last_updated` to now.  Does NOT save.
fn decayed_record(env: &Env, user: &Address) -> ReputationRecord {
    let mut r = load_record(env, user);
    let now = env.ledger().sequence();
    let (decayed, _) = apply_decay(r.score, r.last_updated, now);
    r.score = decayed;
    r.last_updated = now;
    r
}

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

#[contract]
pub struct ReputationScoreContract;

#[contractimpl]
impl ReputationScoreContract {

    // -----------------------------------------------------------------------
    // Initialisation
    // -----------------------------------------------------------------------

    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&Key::Admin) {
            panic_with_error!(env, RepError::AlreadyInitialized);
        }
        admin.require_auth();
        env.storage().instance().set(&Key::Admin, &admin);
        env.storage().instance().extend_ttl(TTL_THRESHOLD, TTL_EXTEND);
    }

    // -----------------------------------------------------------------------
    // Reporter management (admin only)
    // -----------------------------------------------------------------------

    pub fn add_reporter(env: Env, reporter: Address) {
        require_admin(&env);
        let key = Key::Reporter(reporter.clone());
        if env.storage().instance().has(&key) {
            panic_with_error!(env, RepError::ReporterAlreadyAdded);
        }
        env.storage().instance().set(&key, &true);
        env.storage().instance().extend_ttl(TTL_THRESHOLD, TTL_EXTEND);
        env.events()
            .publish((symbol_short!("rep"), symbol_short!("rptr_add")), reporter);
    }

    pub fn remove_reporter(env: Env, reporter: Address) {
        require_admin(&env);
        let key = Key::Reporter(reporter.clone());
        if !env.storage().instance().has(&key) {
            panic_with_error!(env, RepError::ReporterNotFound);
        }
        env.storage().instance().remove(&key);
        env.events()
            .publish((symbol_short!("rep"), symbol_short!("rptr_rm")), reporter);
    }

    // -----------------------------------------------------------------------
    // Transaction recording (reporter only)
    // -----------------------------------------------------------------------

    /// Record a successful transaction for `user`.
    ///
    /// `volume` is the transaction amount in token base units (stroops).
    /// Larger volumes earn a higher score delta (capped at `MAX_VOLUME_BONUS`
    /// extra points per transaction).
    pub fn record_success(env: Env, reporter: Address, user: Address, volume: i128) {
        require_reporter(&env, &reporter);

        if volume < 0 {
            panic_with_error!(env, RepError::InvalidVolume);
        }

        let mut r = decayed_record(&env, &user);
        let delta = volume_delta(volume);
        r.score = (r.score + delta).min(MAX_SCORE);
        r.tx_success += 1;
        r.total_volume += volume;
        save_record(&env, &user, &r);

        env.events().publish(
            (symbol_short!("rep"), symbol_short!("success")),
            (user, r.score, volume, delta),
        );
    }

    /// Record a dispute being *opened* against `user`.
    ///
    /// Applies the initial `DISPUTE_OPEN_DELTA` penalty.  Call
    /// `record_dispute_resolved` once the outcome is known.
    pub fn record_dispute(env: Env, reporter: Address, user: Address) {
        require_reporter(&env, &reporter);

        let mut r = decayed_record(&env, &user);
        r.score = r.score.saturating_sub(DISPUTE_OPEN_DELTA);
        r.tx_disputed += 1;
        save_record(&env, &user, &r);

        env.events().publish(
            (symbol_short!("rep"), symbol_short!("dispute")),
            (user, r.score),
        );
    }

    /// Record the resolution of a previously opened dispute.
    ///
    /// * `resolved_against_user = true`  → additional -`DISPUTE_LOSS_DELTA`
    /// * `resolved_against_user = false` → +`DISPUTE_WIN_DELTA` (partial recovery)
    pub fn record_dispute_resolved(
        env: Env,
        reporter: Address,
        user: Address,
        resolved_against_user: bool,
    ) {
        require_reporter(&env, &reporter);

        let mut r = decayed_record(&env, &user);

        if resolved_against_user {
            r.score = r.score.saturating_sub(DISPUTE_LOSS_DELTA);
            r.disputes_lost += 1;
        } else {
            r.score = (r.score + DISPUTE_WIN_DELTA).min(MAX_SCORE);
            r.disputes_won += 1;
        }

        save_record(&env, &user, &r);

        env.events().publish(
            (symbol_short!("rep"), symbol_short!("disp_res")),
            (user, r.score, resolved_against_user),
        );
    }

    // -----------------------------------------------------------------------
    // Admin
    // -----------------------------------------------------------------------

    pub fn transfer_admin(env: Env, new_admin: Address) {
        require_admin(&env);
        env.storage().instance().set(&Key::Admin, &new_admin);
        env.events()
            .publish((symbol_short!("rep"), symbol_short!("adm_xfer")), new_admin);
    }

    pub fn upgrade(env: Env, new_wasm_hash: soroban_sdk::BytesN<32>) {
        require_admin(&env);
        env.deployer().update_current_contract_wasm(new_wasm_hash);
    }

    // -----------------------------------------------------------------------
    // Queries
    // -----------------------------------------------------------------------

    /// Returns the current score after applying time decay (read-only, no write).
    pub fn get_score(env: Env, user: Address) -> u32 {
        let r = load_record(&env, &user);
        let (decayed, _) = apply_decay(r.score, r.last_updated, env.ledger().sequence());
        decayed
    }

    /// Returns the full reputation record (score is decayed, but NOT written back).
    pub fn get_record(env: Env, user: Address) -> ReputationRecord {
        let mut r = load_record(&env, &user);
        let (decayed, _) = apply_decay(r.score, r.last_updated, env.ledger().sequence());
        r.score = decayed;
        r
    }

    /// Returns a detailed score breakdown showing how each component
    /// (volume, disputes, decay) contributed to the current score.
    ///
    /// This is a pure read — nothing is written to storage.
    pub fn calculate_score(env: Env, user: Address) -> ScoreBreakdown {
        let r = load_record(&env, &user);
        let now = env.ledger().sequence();
        let (decayed, periods) = apply_decay(r.score, r.last_updated, now);

        // Reconstruct the contribution of each component from the stored
        // counters.  These are approximations because the score is updated
        // incrementally (each tx applies decay first), but they give a
        // meaningful picture of what drove the score.
        //
        // volume_contribution: total points earned from successful txs
        //   = tx_success * BASE_SUCCESS_DELTA  +  volume_bonus_estimate
        // We use the average volume per tx to estimate the bonus.
        let avg_volume = if r.tx_success > 0 {
            r.total_volume / r.tx_success as i128
        } else {
            0
        };
        let avg_bonus = (avg_volume / VOLUME_TIER) as i64;
        let avg_bonus_capped = avg_bonus.min(MAX_VOLUME_BONUS as i64);
        let volume_contribution: i64 =
            r.tx_success as i64 * (BASE_SUCCESS_DELTA as i64 + avg_bonus_capped);

        let dispute_open_impact: i64 = -(r.tx_disputed as i64 * DISPUTE_OPEN_DELTA as i64);
        let dispute_loss_impact: i64 = -(r.disputes_lost as i64 * DISPUTE_LOSS_DELTA as i64);
        let dispute_win_impact: i64 = r.disputes_won as i64 * DISPUTE_WIN_DELTA as i64;

        ScoreBreakdown {
            score: decayed,
            decay_periods: periods,
            pre_decay_score: r.score,
            volume_contribution,
            dispute_open_impact,
            dispute_loss_impact,
            dispute_win_impact,
            tx_success: r.tx_success,
            tx_disputed: r.tx_disputed,
            total_volume: r.total_volume,
        }
    }

    pub fn is_reporter(env: Env, reporter: Address) -> bool {
        env.storage()
            .instance()
            .get::<Key, bool>(&Key::Reporter(reporter))
            .unwrap_or(false)
    }
}

mod test;