#![no_std]

//! # Payment Analytics Contract
//!
//! Aggregates on-chain payment metrics for reporting and monitoring.
//!
//! ## What is tracked
//!
//! * **Global counters** – total payment count, total volume, total fees
//!   collected, total failed payments.
//! * **Per-token counters** – volume and count broken down by token address.
//! * **Per-merchant counters** – volume, count, and fee totals per merchant.
//! * **Time-bucketed volume** – volume bucketed by configurable ledger period
//!   (default ~1 day), enabling time-series queries.
//! * **Largest payment** – the single largest payment amount ever recorded.
//!
//! ## Access control
//!
//! Only authorised **recorders** (e.g. the payment router) may call
//! `record_payment` and `record_failure`.  The admin manages the recorder
//! whitelist.
//!
//! ## Design notes
//!
//! All counters use `i128` for amounts (matching the token standard) and
//! `u64` for counts.  Overflow is handled with saturating arithmetic so the
//! contract never panics on counter overflow.
//!
//! Bucket keys are stored in persistent storage; all other counters live in
//! instance storage for cheap access.

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, symbol_short,
    Address, Env, Symbol,
};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Default ledgers per analytics bucket (~1 day at 5 s/ledger).
pub const DEFAULT_BUCKET_SIZE: u32 = 17_280;

/// Instance storage TTL (~1 year).
const TTL_THRESHOLD: u32 = 100_000;
const TTL_EXTEND: u32 = 6_307_200;

/// Persistent storage TTL (~6 months).
const PERSISTENT_TTL_THRESHOLD: u32 = 50_000;
const PERSISTENT_TTL_EXTEND: u32 = 3_153_600;

// ---------------------------------------------------------------------------
// Storage keys
// ---------------------------------------------------------------------------

const KEY_ADMIN: Symbol = symbol_short!("admin");
const KEY_PAUSED: Symbol = symbol_short!("paused");
const KEY_VERSION: Symbol = symbol_short!("version");
const KEY_BUCKET_SZ: Symbol = symbol_short!("bkt_sz");

/// Global aggregate keys (all in instance storage).
const KEY_TOTAL_COUNT: Symbol = symbol_short!("tot_cnt");
const KEY_TOTAL_VOL: Symbol = symbol_short!("tot_vol");
const KEY_TOTAL_FEES: Symbol = symbol_short!("tot_fees");
const KEY_TOTAL_FAIL: Symbol = symbol_short!("tot_fail");
const KEY_MAX_PAYMENT: Symbol = symbol_short!("max_pay");

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    /// Authorised recorder whitelist.
    Recorder(Address),
    /// Per-token volume: token → i128.
    TokenVolume(Address),
    /// Per-token count: token → u64.
    TokenCount(Address),
    /// Per-merchant volume: merchant → i128.
    MerchantVolume(Address),
    /// Per-merchant count: merchant → u64.
    MerchantCount(Address),
    /// Per-merchant fees: merchant → i128.
    MerchantFees(Address),
    /// Time-bucketed volume: bucket_index → i128.
    BucketVolume(u64),
    /// Time-bucketed count: bucket_index → u64.
    BucketCount(u64),
}

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Snapshot of all global analytics counters.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct GlobalStats {
    pub total_payment_count: u64,
    pub total_volume: i128,
    pub total_fees: i128,
    pub total_failed_count: u64,
    pub largest_payment: i128,
}

/// Per-token analytics snapshot.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct TokenStats {
    pub token: Address,
    pub volume: i128,
    pub count: u64,
}

/// Per-merchant analytics snapshot.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct MerchantStats {
    pub merchant: Address,
    pub volume: i128,
    pub count: u64,
    pub fees: i128,
}

/// A single time-bucket snapshot.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct BucketStats {
    pub bucket_index: u64,
    pub volume: i128,
    pub count: u64,
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[contracterror]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum AnalyticsError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    ContractPaused = 4,
    InvalidAmount = 5,
    RecorderAlreadyAdded = 6,
    RecorderNotFound = 7,
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn bump_instance(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(TTL_THRESHOLD, TTL_EXTEND);
}

fn bump_persistent<K>(env: &Env, key: &K)
where
    K: soroban_sdk::IntoVal<Env, soroban_sdk::Val>,
{
    env.storage()
        .persistent()
        .extend_ttl(key, PERSISTENT_TTL_THRESHOLD, PERSISTENT_TTL_EXTEND);
}

fn require_admin(env: &Env) -> Address {
    let admin: Address = env
        .storage()
        .instance()
        .get(&KEY_ADMIN)
        .unwrap_or_else(|| panic_with_error!(env, AnalyticsError::NotInitialized));
    admin.require_auth();
    admin
}

fn require_not_paused(env: &Env) {
    if env
        .storage()
        .instance()
        .get::<Symbol, bool>(&KEY_PAUSED)
        .unwrap_or(false)
    {
        panic_with_error!(env, AnalyticsError::ContractPaused);
    }
}

fn require_recorder(env: &Env, recorder: &Address) {
    recorder.require_auth();
    if !env
        .storage()
        .persistent()
        .get::<DataKey, bool>(&DataKey::Recorder(recorder.clone()))
        .unwrap_or(false)
    {
        panic_with_error!(env, AnalyticsError::Unauthorized);
    }
}

/// Compute the current bucket index from the ledger sequence.
fn current_bucket(env: &Env) -> u64 {
    let bucket_size: u32 = env
        .storage()
        .instance()
        .get(&KEY_BUCKET_SZ)
        .unwrap_or(DEFAULT_BUCKET_SIZE);
    (env.ledger().sequence() / bucket_size) as u64
}

fn add_to_persistent_i128(env: &Env, key: &DataKey, delta: i128) -> i128 {
    let current: i128 = env.storage().persistent().get(key).unwrap_or(0);
    let new_val = current.saturating_add(delta);
    env.storage().persistent().set(key, &new_val);
    bump_persistent(env, key);
    new_val
}

fn add_to_persistent_u64(env: &Env, key: &DataKey, delta: u64) -> u64 {
    let current: u64 = env.storage().persistent().get(key).unwrap_or(0);
    let new_val = current.saturating_add(delta);
    env.storage().persistent().set(key, &new_val);
    bump_persistent(env, key);
    new_val
}

fn add_to_instance_i128(env: &Env, key: &Symbol, delta: i128) -> i128 {
    let current: i128 = env.storage().instance().get(key).unwrap_or(0);
    let new_val = current.saturating_add(delta);
    env.storage().instance().set(key, &new_val);
    new_val
}

fn add_to_instance_u64(env: &Env, key: &Symbol, delta: u64) -> u64 {
    let current: u64 = env.storage().instance().get(key).unwrap_or(0);
    let new_val = current.saturating_add(delta);
    env.storage().instance().set(key, &new_val);
    new_val
}

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

#[contract]
pub struct PaymentAnalytics;

#[contractimpl]
impl PaymentAnalytics {
    // -----------------------------------------------------------------------
    // Initialisation
    // -----------------------------------------------------------------------

    /// Initialise the analytics contract.
    ///
    /// * `admin`       – address that manages recorders and configuration
    /// * `bucket_size` – ledgers per time bucket (0 → use default ~1 day)
    pub fn initialize(env: Env, admin: Address, bucket_size: u32) {
        if env.storage().instance().has(&KEY_ADMIN) {
            panic_with_error!(env, AnalyticsError::AlreadyInitialized);
        }

        admin.require_auth();

        let bkt = if bucket_size == 0 {
            DEFAULT_BUCKET_SIZE
        } else {
            bucket_size
        };

        env.storage().instance().set(&KEY_ADMIN, &admin);
        env.storage().instance().set(&KEY_BUCKET_SZ, &bkt);
        env.storage().instance().set(&KEY_PAUSED, &false);
        env.storage().instance().set(&KEY_VERSION, &1u32);
        env.storage().instance().set(&KEY_TOTAL_COUNT, &0u64);
        env.storage().instance().set(&KEY_TOTAL_VOL, &0i128);
        env.storage().instance().set(&KEY_TOTAL_FEES, &0i128);
        env.storage().instance().set(&KEY_TOTAL_FAIL, &0u64);
        env.storage().instance().set(&KEY_MAX_PAYMENT, &0i128);
        bump_instance(&env);

        env.events().publish(
            (symbol_short!("analytics"), symbol_short!("init")),
            admin,
        );
    }

    // -----------------------------------------------------------------------
    // Recorder management (admin only)
    // -----------------------------------------------------------------------

    /// Authorise an address to record payment events.
    pub fn add_recorder(env: Env, recorder: Address) {
        require_admin(&env);
        bump_instance(&env);

        let key = DataKey::Recorder(recorder.clone());
        if env
            .storage()
            .persistent()
            .get::<DataKey, bool>(&key)
            .unwrap_or(false)
        {
            panic_with_error!(env, AnalyticsError::RecorderAlreadyAdded);
        }

        env.storage().persistent().set(&key, &true);
        bump_persistent(&env, &key);

        env.events().publish(
            (symbol_short!("analytics"), symbol_short!("rec_add")),
            recorder,
        );
    }

    /// Remove a recorder.
    pub fn remove_recorder(env: Env, recorder: Address) {
        require_admin(&env);
        bump_instance(&env);

        let key = DataKey::Recorder(recorder.clone());
        if !env
            .storage()
            .persistent()
            .get::<DataKey, bool>(&key)
            .unwrap_or(false)
        {
            panic_with_error!(env, AnalyticsError::RecorderNotFound);
        }

        env.storage().persistent().remove(&key);

        env.events().publish(
            (symbol_short!("analytics"), symbol_short!("rec_rm")),
            recorder,
        );
    }

    // -----------------------------------------------------------------------
    // Core: record a successful payment
    // -----------------------------------------------------------------------

    /// Record a successful payment.
    ///
    /// * `recorder`  – authorised recorder (must sign)
    /// * `token`     – token used for the payment
    /// * `merchant`  – merchant that received the payment
    /// * `amount`    – gross payment amount (> 0)
    /// * `fee`       – fee deducted from the payment (≥ 0)
    pub fn record_payment(
        env: Env,
        recorder: Address,
        token: Address,
        merchant: Address,
        amount: i128,
        fee: i128,
    ) {
        require_not_paused(&env);
        bump_instance(&env);

        require_recorder(&env, &recorder);

        if amount <= 0 {
            panic_with_error!(env, AnalyticsError::InvalidAmount);
        }
        if fee < 0 {
            panic_with_error!(env, AnalyticsError::InvalidAmount);
        }

        // --- Global counters (instance storage) ---
        add_to_instance_u64(&env, &KEY_TOTAL_COUNT, 1);
        add_to_instance_i128(&env, &KEY_TOTAL_VOL, amount);
        add_to_instance_i128(&env, &KEY_TOTAL_FEES, fee);

        // Update largest payment.
        let current_max: i128 = env
            .storage()
            .instance()
            .get(&KEY_MAX_PAYMENT)
            .unwrap_or(0);
        if amount > current_max {
            env.storage().instance().set(&KEY_MAX_PAYMENT, &amount);
        }

        // --- Per-token counters (persistent storage) ---
        add_to_persistent_i128(&env, &DataKey::TokenVolume(token.clone()), amount);
        add_to_persistent_u64(&env, &DataKey::TokenCount(token.clone()), 1);

        // --- Per-merchant counters (persistent storage) ---
        add_to_persistent_i128(&env, &DataKey::MerchantVolume(merchant.clone()), amount);
        add_to_persistent_u64(&env, &DataKey::MerchantCount(merchant.clone()), 1);
        add_to_persistent_i128(&env, &DataKey::MerchantFees(merchant.clone()), fee);

        // --- Time-bucketed counters (persistent storage) ---
        let bucket = current_bucket(&env);
        add_to_persistent_i128(&env, &DataKey::BucketVolume(bucket), amount);
        add_to_persistent_u64(&env, &DataKey::BucketCount(bucket), 1);

        env.events().publish(
            (symbol_short!("analytics"), symbol_short!("payment")),
            (token, merchant, amount, fee, bucket),
        );
    }

    /// Record a failed payment attempt.
    ///
    /// * `recorder` – authorised recorder (must sign)
    /// * `token`    – token that was attempted
    /// * `amount`   – attempted amount (> 0)
    pub fn record_failure(env: Env, recorder: Address, token: Address, amount: i128) {
        require_not_paused(&env);
        bump_instance(&env);

        require_recorder(&env, &recorder);

        if amount <= 0 {
            panic_with_error!(env, AnalyticsError::InvalidAmount);
        }

        add_to_instance_u64(&env, &KEY_TOTAL_FAIL, 1);

        env.events().publish(
            (symbol_short!("analytics"), symbol_short!("failure")),
            (token, amount),
        );
    }

    // -----------------------------------------------------------------------
    // Admin: configuration / circuit breaker / upgrade
    // -----------------------------------------------------------------------

    /// Update the time-bucket size (ledgers per bucket).
    /// Note: changing this does not retroactively re-bucket historical data.
    pub fn set_bucket_size(env: Env, bucket_size: u32) {
        require_admin(&env);
        bump_instance(&env);

        let bkt = if bucket_size == 0 {
            DEFAULT_BUCKET_SIZE
        } else {
            bucket_size
        };

        env.storage().instance().set(&KEY_BUCKET_SZ, &bkt);

        env.events().publish(
            (symbol_short!("analytics"), symbol_short!("bkt_upd")),
            bkt,
        );
    }

    pub fn pause(env: Env) {
        require_admin(&env);
        env.storage().instance().set(&KEY_PAUSED, &true);
        env.events()
            .publish((symbol_short!("analytics"), symbol_short!("paused")), ());
    }

    pub fn unpause(env: Env) {
        require_admin(&env);
        env.storage().instance().set(&KEY_PAUSED, &false);
        env.events()
            .publish((symbol_short!("analytics"), symbol_short!("unpaused")), ());
    }

    pub fn upgrade(env: Env, new_wasm_hash: soroban_sdk::BytesN<32>) {
        require_admin(&env);
        let version: u32 = env.storage().instance().get(&KEY_VERSION).unwrap_or(1);
        env.deployer().update_current_contract_wasm(new_wasm_hash);
        env.storage().instance().set(&KEY_VERSION, &(version + 1));
    }

    pub fn transfer_admin(env: Env, new_admin: Address) {
        require_admin(&env);
        env.storage().instance().set(&KEY_ADMIN, &new_admin);
        env.events().publish(
            (symbol_short!("analytics"), symbol_short!("adm_xfer")),
            new_admin,
        );
    }

    // -----------------------------------------------------------------------
    // Views
    // -----------------------------------------------------------------------

    /// Returns a snapshot of all global counters.
    pub fn get_global_stats(env: Env) -> GlobalStats {
        GlobalStats {
            total_payment_count: env
                .storage()
                .instance()
                .get(&KEY_TOTAL_COUNT)
                .unwrap_or(0),
            total_volume: env
                .storage()
                .instance()
                .get(&KEY_TOTAL_VOL)
                .unwrap_or(0),
            total_fees: env
                .storage()
                .instance()
                .get(&KEY_TOTAL_FEES)
                .unwrap_or(0),
            total_failed_count: env
                .storage()
                .instance()
                .get(&KEY_TOTAL_FAIL)
                .unwrap_or(0),
            largest_payment: env
                .storage()
                .instance()
                .get(&KEY_MAX_PAYMENT)
                .unwrap_or(0),
        }
    }

    /// Returns analytics for a specific token.
    pub fn get_token_stats(env: Env, token: Address) -> TokenStats {
        TokenStats {
            token: token.clone(),
            volume: env
                .storage()
                .persistent()
                .get(&DataKey::TokenVolume(token.clone()))
                .unwrap_or(0),
            count: env
                .storage()
                .persistent()
                .get(&DataKey::TokenCount(token))
                .unwrap_or(0),
        }
    }

    /// Returns analytics for a specific merchant.
    pub fn get_merchant_stats(env: Env, merchant: Address) -> MerchantStats {
        MerchantStats {
            merchant: merchant.clone(),
            volume: env
                .storage()
                .persistent()
                .get(&DataKey::MerchantVolume(merchant.clone()))
                .unwrap_or(0),
            count: env
                .storage()
                .persistent()
                .get(&DataKey::MerchantCount(merchant.clone()))
                .unwrap_or(0),
            fees: env
                .storage()
                .persistent()
                .get(&DataKey::MerchantFees(merchant))
                .unwrap_or(0),
        }
    }

    /// Returns analytics for a specific time bucket.
    pub fn get_bucket_stats(env: Env, bucket_index: u64) -> BucketStats {
        BucketStats {
            bucket_index,
            volume: env
                .storage()
                .persistent()
                .get(&DataKey::BucketVolume(bucket_index))
                .unwrap_or(0),
            count: env
                .storage()
                .persistent()
                .get(&DataKey::BucketCount(bucket_index))
                .unwrap_or(0),
        }
    }

    /// Returns analytics for the current time bucket.
    pub fn get_current_bucket_stats(env: Env) -> BucketStats {
        let bucket = current_bucket(&env);
        BucketStats {
            bucket_index: bucket,
            volume: env
                .storage()
                .persistent()
                .get(&DataKey::BucketVolume(bucket))
                .unwrap_or(0),
            count: env
                .storage()
                .persistent()
                .get(&DataKey::BucketCount(bucket))
                .unwrap_or(0),
        }
    }

    /// Returns the current bucket index.
    pub fn current_bucket_index(env: Env) -> u64 {
        current_bucket(&env)
    }

    pub fn is_recorder(env: Env, recorder: Address) -> bool {
        env.storage()
            .persistent()
            .get::<DataKey, bool>(&DataKey::Recorder(recorder))
            .unwrap_or(false)
    }

    pub fn is_paused(env: Env) -> bool {
        env.storage()
            .instance()
            .get::<Symbol, bool>(&KEY_PAUSED)
            .unwrap_or(false)
    }

    pub fn get_admin(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&KEY_ADMIN)
            .unwrap_or_else(|| panic_with_error!(env, AnalyticsError::NotInitialized))
    }

    pub fn get_bucket_size(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&KEY_BUCKET_SZ)
            .unwrap_or(DEFAULT_BUCKET_SIZE)
    }

    pub fn get_version(env: Env) -> u32 {
        env.storage().instance().get(&KEY_VERSION).unwrap_or(1)
    }
}


