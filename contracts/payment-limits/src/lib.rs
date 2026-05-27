#![no_std]

//! # Payment Limits Contract
//!
//! Enforces per-user and global payment limits on a configurable token.
//!
//! ## Features
//!
//! * **Per-user daily limit** – each address may not send more than
//!   `user_daily_limit` in a rolling 24-hour window (measured in ledgers).
//! * **Per-user transaction limit** – a single payment may not exceed
//!   `user_tx_limit`.
//! * **Global daily limit** – total volume across all users may not exceed
//!   `global_daily_limit` in the same rolling window.
//! * **Whitelist** – admin-approved addresses bypass all limits (e.g. the
//!   payment router itself).
//! * **Circuit breaker** – admin can pause all limit checks (emergency stop).
//! * **Upgrade** – admin can upgrade the contract WASM.
//!
//! ## Integration
//!
//! The payment router (or any authorised caller) calls `check_and_record`
//! before executing a payment.  The call reverts if any limit would be
//! breached; otherwise it records the spend and returns `true`.
//!
//! ## Rolling window
//!
//! The window is approximated per-user: we store the ledger sequence at which
//! the current window started and the cumulative spend within that window.
//! When `current_ledger - window_start >= LEDGERS_PER_DAY` the window resets.
//! This is a simple "fixed window" approach — sufficient for on-chain use
//! where exact sliding windows are prohibitively expensive.

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, symbol_short,
    Address, Env, Symbol,
};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Approximate ledgers per 24 hours at ~5 s/ledger.
pub const LEDGERS_PER_DAY: u32 = 17_280;

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

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    /// Global limit configuration.
    Config,
    /// Per-user spend window: (window_start_ledger, cumulative_spend).
    UserWindow(Address),
    /// Global spend window: (window_start_ledger, cumulative_spend).
    GlobalWindow,
    /// Whitelisted address — bypasses all limits.
    Whitelist(Address),
}

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Configurable limit parameters.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct LimitConfig {
    /// Maximum amount a single user may send in one transaction.
    pub user_tx_limit: i128,
    /// Maximum amount a single user may send within one rolling day window.
    pub user_daily_limit: i128,
    /// Maximum total amount all users may send within one rolling day window.
    pub global_daily_limit: i128,
}

/// Rolling-window spend tracker stored per user and globally.
#[contracttype]
#[derive(Clone)]
pub struct SpendWindow {
    /// Ledger sequence at which this window started.
    pub window_start: u32,
    /// Cumulative spend within the current window.
    pub spent: i128,
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[contracterror]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum LimitError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    ContractPaused = 4,
    /// Single transaction exceeds `user_tx_limit`.
    TxLimitExceeded = 5,
    /// User's daily cumulative spend would exceed `user_daily_limit`.
    UserDailyLimitExceeded = 6,
    /// Global daily cumulative spend would exceed `global_daily_limit`.
    GlobalDailyLimitExceeded = 7,
    InvalidAmount = 8,
    InvalidConfig = 9,
    AddressAlreadyWhitelisted = 10,
    AddressNotWhitelisted = 11,
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
        .unwrap_or_else(|| panic_with_error!(env, LimitError::NotInitialized));
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
        panic_with_error!(env, LimitError::ContractPaused);
    }
}

fn load_config(env: &Env) -> LimitConfig {
    env.storage()
        .instance()
        .get(&DataKey::Config)
        .unwrap_or_else(|| panic_with_error!(env, LimitError::NotInitialized))
}

/// Load a spend window, resetting it if the rolling day has elapsed.
fn load_window(env: &Env, key: &DataKey) -> SpendWindow {
    let current = env.ledger().sequence();
    let window: SpendWindow = env
        .storage()
        .persistent()
        .get(key)
        .unwrap_or(SpendWindow {
            window_start: current,
            spent: 0,
        });

    if current.saturating_sub(window.window_start) >= LEDGERS_PER_DAY {
        // Window has expired — return a fresh one.
        SpendWindow {
            window_start: current,
            spent: 0,
        }
    } else {
        window
    }
}

fn save_window(env: &Env, key: &DataKey, window: &SpendWindow) {
    env.storage().persistent().set(key, window);
    bump_persistent(env, key);
}

fn is_whitelisted(env: &Env, address: &Address) -> bool {
    env.storage()
        .persistent()
        .get::<DataKey, bool>(&DataKey::Whitelist(address.clone()))
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

#[contract]
pub struct PaymentLimits;

#[contractimpl]
impl PaymentLimits {
    // -----------------------------------------------------------------------
    // Initialisation
    // -----------------------------------------------------------------------

    /// Initialise the contract.
    ///
    /// * `admin`  – address that manages limits and the whitelist
    /// * `config` – initial limit configuration
    pub fn initialize(env: Env, admin: Address, config: LimitConfig) {
        if env.storage().instance().has(&KEY_ADMIN) {
            panic_with_error!(env, LimitError::AlreadyInitialized);
        }

        admin.require_auth();

        if config.user_tx_limit <= 0
            || config.user_daily_limit <= 0
            || config.global_daily_limit <= 0
        {
            panic_with_error!(env, LimitError::InvalidConfig);
        }

        env.storage().instance().set(&KEY_ADMIN, &admin);
        env.storage().instance().set(&DataKey::Config, &config);
        env.storage().instance().set(&KEY_PAUSED, &false);
        env.storage().instance().set(&KEY_VERSION, &1u32);
        bump_instance(&env);

        env.events().publish(
            (symbol_short!("limits"), symbol_short!("init")),
            admin,
        );
    }

    // -----------------------------------------------------------------------
    // Core: check and record a payment
    // -----------------------------------------------------------------------

    /// Check whether `payer` may send `amount` and, if so, record the spend.
    ///
    /// Reverts with the appropriate error if any limit would be breached.
    /// Whitelisted addresses bypass all checks.
    ///
    /// * `payer`  – the address initiating the payment (must sign)
    /// * `amount` – the payment amount in token base units (> 0)
    ///
    /// Returns `true` on success.
    pub fn check_and_record(env: Env, payer: Address, amount: i128) -> bool {
        require_not_paused(&env);
        bump_instance(&env);

        payer.require_auth();

        if amount <= 0 {
            panic_with_error!(env, LimitError::InvalidAmount);
        }

        // Whitelisted addresses skip all limit checks.
        if is_whitelisted(&env, &payer) {
            return true;
        }

        let config = load_config(&env);

        // 1. Per-transaction limit.
        if amount > config.user_tx_limit {
            panic_with_error!(env, LimitError::TxLimitExceeded);
        }

        // 2. Per-user daily limit.
        let user_key = DataKey::UserWindow(payer.clone());
        let mut user_window = load_window(&env, &user_key);
        let new_user_spent = user_window.spent + amount;
        if new_user_spent > config.user_daily_limit {
            panic_with_error!(env, LimitError::UserDailyLimitExceeded);
        }

        // 3. Global daily limit.
        let global_key = DataKey::GlobalWindow;
        let mut global_window = load_window(&env, &global_key);
        let new_global_spent = global_window.spent + amount;
        if new_global_spent > config.global_daily_limit {
            panic_with_error!(env, LimitError::GlobalDailyLimitExceeded);
        }

        // All checks passed — commit the spend.
        user_window.spent = new_user_spent;
        global_window.spent = new_global_spent;

        save_window(&env, &user_key, &user_window);
        save_window(&env, &global_key, &global_window);

        env.events().publish(
            (symbol_short!("limits"), symbol_short!("recorded")),
            (payer, amount, new_user_spent, new_global_spent),
        );

        true
    }

    // -----------------------------------------------------------------------
    // Admin: limit configuration
    // -----------------------------------------------------------------------

    /// Update the limit configuration.
    pub fn update_config(env: Env, config: LimitConfig) {
        require_admin(&env);
        bump_instance(&env);

        if config.user_tx_limit <= 0
            || config.user_daily_limit <= 0
            || config.global_daily_limit <= 0
        {
            panic_with_error!(env, LimitError::InvalidConfig);
        }

        env.storage().instance().set(&DataKey::Config, &config);

        env.events().publish(
            (symbol_short!("limits"), symbol_short!("cfg_upd")),
            (),
        );
    }

    // -----------------------------------------------------------------------
    // Admin: whitelist management
    // -----------------------------------------------------------------------

    /// Add an address to the whitelist (bypasses all limits).
    pub fn add_to_whitelist(env: Env, address: Address) {
        require_admin(&env);
        bump_instance(&env);

        let key = DataKey::Whitelist(address.clone());
        if env
            .storage()
            .persistent()
            .get::<DataKey, bool>(&key)
            .unwrap_or(false)
        {
            panic_with_error!(env, LimitError::AddressAlreadyWhitelisted);
        }

        env.storage().persistent().set(&key, &true);
        bump_persistent(&env, &key);

        env.events().publish(
            (symbol_short!("limits"), symbol_short!("wl_add")),
            address,
        );
    }

    /// Remove an address from the whitelist.
    pub fn remove_from_whitelist(env: Env, address: Address) {
        require_admin(&env);
        bump_instance(&env);

        let key = DataKey::Whitelist(address.clone());
        if !env
            .storage()
            .persistent()
            .get::<DataKey, bool>(&key)
            .unwrap_or(false)
        {
            panic_with_error!(env, LimitError::AddressNotWhitelisted);
        }

        env.storage().persistent().remove(&key);

        env.events().publish(
            (symbol_short!("limits"), symbol_short!("wl_rm")),
            address,
        );
    }

    // -----------------------------------------------------------------------
    // Admin: circuit breaker
    // -----------------------------------------------------------------------

    pub fn pause(env: Env) {
        require_admin(&env);
        env.storage().instance().set(&KEY_PAUSED, &true);
        env.events()
            .publish((symbol_short!("limits"), symbol_short!("paused")), ());
    }

    pub fn unpause(env: Env) {
        require_admin(&env);
        env.storage().instance().set(&KEY_PAUSED, &false);
        env.events()
            .publish((symbol_short!("limits"), symbol_short!("unpaused")), ());
    }

    // -----------------------------------------------------------------------
    // Admin: upgrade / transfer admin
    // -----------------------------------------------------------------------

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
            (symbol_short!("limits"), symbol_short!("adm_xfer")),
            new_admin,
        );
    }

    // -----------------------------------------------------------------------
    // Views
    // -----------------------------------------------------------------------

    /// Returns the current limit configuration.
    pub fn get_config(env: Env) -> LimitConfig {
        load_config(&env)
    }

    /// Returns the current spend window for a user.
    /// The window is reset lazily if the rolling day has elapsed.
    pub fn get_user_window(env: Env, user: Address) -> SpendWindow {
        load_window(&env, &DataKey::UserWindow(user))
    }

    /// Returns the current global spend window.
    pub fn get_global_window(env: Env) -> SpendWindow {
        load_window(&env, &DataKey::GlobalWindow)
    }

    /// Returns how much `user` may still send today (0 if limit exhausted).
    pub fn remaining_user_limit(env: Env, user: Address) -> i128 {
        if is_whitelisted(&env, &user) {
            return i128::MAX;
        }
        let config = load_config(&env);
        let window = load_window(&env, &DataKey::UserWindow(user));
        (config.user_daily_limit - window.spent).max(0)
    }

    /// Returns how much may still be sent globally today.
    pub fn remaining_global_limit(env: Env) -> i128 {
        let config = load_config(&env);
        let window = load_window(&env, &DataKey::GlobalWindow);
        (config.global_daily_limit - window.spent).max(0)
    }

    pub fn is_whitelisted(env: Env, address: Address) -> bool {
        is_whitelisted(&env, &address)
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
            .unwrap_or_else(|| panic_with_error!(env, LimitError::NotInitialized))
    }

    pub fn get_version(env: Env) -> u32 {
        env.storage().instance().get(&KEY_VERSION).unwrap_or(1)
    }
}


