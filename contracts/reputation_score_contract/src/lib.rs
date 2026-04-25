#![no_std]

//! # Reputation Score Contract
//!
//! ## Scoring Algorithm
//!
//! Each address starts with a neutral score of 500 (range 0–1000).
//!
//! ### Events
//! - Successful transaction: +10 points (capped at 1000)
//! - Disputed transaction:   -50 points (floor 0)
//!
//! ### Time Decay (mean-reversion)
//! Applied lazily whenever a record is read or written.
//! For every `DECAY_PERIOD` ledgers elapsed since the last update,
//! the score moves 1% closer to the neutral value of 500.
//!
//! Formula per period:  score = score + (500 - score) / 100
//!
//! This means high scores drift down and low scores drift up over time,
//! reflecting that reputation must be continuously earned.
//!
//! ### Access Control
//! Only addresses in the reporter whitelist (set by admin) may record
//! transactions. The admin manages the whitelist and can upgrade the contract.

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, symbol_short, Address,
    Env,
};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Neutral / starting score.
const NEUTRAL: u32 = 500;
/// Maximum score.
const MAX_SCORE: u32 = 1_000;
/// Points added per successful transaction.
const SUCCESS_DELTA: u32 = 10;
/// Points removed per disputed transaction.
const DISPUTE_DELTA: u32 = 50;
/// Ledgers per decay period (~1 day at 5s/ledger = 17_280 ledgers).
const DECAY_PERIOD: u32 = 17_280;
/// Instance storage TTL threshold / extension (~1 year).
const TTL_THRESHOLD: u32 = 100_000;
const TTL_EXTEND: u32 = 6_307_200;
/// Persistent storage TTL (~6 months).
const PERSISTENT_TTL_THRESHOLD: u32 = 50_000;
const PERSISTENT_TTL_EXTEND: u32 = 3_153_600;

// ---------------------------------------------------------------------------
// Storage keys
// ---------------------------------------------------------------------------

#[contracttype]
#[derive(Clone)]
enum Key {
    Admin,
    Reporter(Address), // whitelist entry
    Record(Address),   // per-address reputation record
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[contracttype]
#[derive(Clone)]
pub struct ReputationRecord {
    /// Current score (0–1000).
    pub score: u32,
    /// Ledger number of last update (used for decay calculation).
    pub last_updated: u32,
    /// Cumulative successful transactions recorded.
    pub tx_success: u32,
    /// Cumulative disputed transactions recorded.
    pub tx_disputed: u32,
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

/// Apply time-decay to a score: for each elapsed DECAY_PERIOD, move 1% toward NEUTRAL.
fn apply_decay(score: u32, last_updated: u32, current_ledger: u32) -> u32 {
    if current_ledger <= last_updated {
        return score;
    }
    let periods = (current_ledger - last_updated) / DECAY_PERIOD;
    if periods == 0 {
        return score;
    }
    // Apply decay iteratively (max ~365 iterations/year, acceptable).
    // Each period: score += (NEUTRAL - score) / 100  (integer, rounds toward neutral)
    let mut s = score as i64;
    let neutral = NEUTRAL as i64;
    for _ in 0..periods {
        let delta = (neutral - s) / 100;
        s += delta;
        // Clamp: if delta was 0 and score != neutral, nudge by 1 toward neutral
        if delta == 0 && s != neutral {
            s += if s < neutral { 1 } else { -1 };
        }
    }
    s.clamp(0, MAX_SCORE as i64) as u32
}

fn load_record(env: &Env, user: &Address) -> ReputationRecord {
    let key = Key::Record(user.clone());
    env.storage()
        .persistent()
        .get(&key)
        .unwrap_or(ReputationRecord {
            score: NEUTRAL,
            last_updated: env.ledger().sequence(),
            tx_success: 0,
            tx_disputed: 0,
        })
}

fn save_record(env: &Env, user: &Address, record: &ReputationRecord) {
    let key = Key::Record(user.clone());
    env.storage().persistent().set(&key, record);
    env.storage()
        .persistent()
        .extend_ttl(&key, PERSISTENT_TTL_THRESHOLD, PERSISTENT_TTL_EXTEND);
}

fn decayed_record(env: &Env, user: &Address) -> ReputationRecord {
    let mut r = load_record(env, user);
    let now = env.ledger().sequence();
    r.score = apply_decay(r.score, r.last_updated, now);
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
    // Init
    // -----------------------------------------------------------------------

    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&Key::Admin) {
            panic_with_error!(env, RepError::AlreadyInitialized);
        }
        admin.require_auth();
        env.storage().instance().set(&Key::Admin, &admin);
        env.storage()
            .instance()
            .extend_ttl(TTL_THRESHOLD, TTL_EXTEND);
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
        env.storage()
            .instance()
            .extend_ttl(TTL_THRESHOLD, TTL_EXTEND);
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

    /// Record a successful transaction for `user`. Caller must be a whitelisted reporter.
    pub fn record_success(env: Env, reporter: Address, user: Address) {
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

        let mut r = decayed_record(&env, &user);
        r.score = (r.score + SUCCESS_DELTA).min(MAX_SCORE);
        r.tx_success += 1;
        save_record(&env, &user, &r);

        env.events().publish(
            (symbol_short!("rep"), symbol_short!("success")),
            (user, r.score),
        );
    }

    /// Record a disputed transaction for `user`. Caller must be a whitelisted reporter.
    pub fn record_dispute(env: Env, reporter: Address, user: Address) {
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

        let mut r = decayed_record(&env, &user);
        r.score = r.score.saturating_sub(DISPUTE_DELTA);
        r.tx_disputed += 1;
        save_record(&env, &user, &r);

        env.events().publish(
            (symbol_short!("rep"), symbol_short!("dispute")),
            (user, r.score),
        );
    }

    // -----------------------------------------------------------------------
    // Admin: transfer admin
    // -----------------------------------------------------------------------

    pub fn transfer_admin(env: Env, new_admin: Address) {
        require_admin(&env);
        env.storage().instance().set(&Key::Admin, &new_admin);
        env.events()
            .publish((symbol_short!("rep"), symbol_short!("adm_xfer")), new_admin);
    }

    // -----------------------------------------------------------------------
    // Admin: upgrade
    // -----------------------------------------------------------------------

    pub fn upgrade(env: Env, new_wasm_hash: soroban_sdk::BytesN<32>) {
        require_admin(&env);
        env.deployer().update_current_contract_wasm(new_wasm_hash);
    }

    // -----------------------------------------------------------------------
    // Queries
    // -----------------------------------------------------------------------

    /// Returns the current score after applying time decay (does not write).
    pub fn get_score(env: Env, user: Address) -> u32 {
        let r = load_record(&env, &user);
        apply_decay(r.score, r.last_updated, env.ledger().sequence())
    }

    /// Returns the full reputation record after applying time decay (does not write).
    pub fn get_record(env: Env, user: Address) -> ReputationRecord {
        let mut r = load_record(&env, &user);
        r.score = apply_decay(r.score, r.last_updated, env.ledger().sequence());
        r
    }

    pub fn is_reporter(env: Env, reporter: Address) -> bool {
        env.storage()
            .instance()
            .get::<Key, bool>(&Key::Reporter(reporter))
            .unwrap_or(false)
    }
}

mod test;
