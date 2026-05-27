#![no_std]

//! # Payment Recovery Contract
//!
//! Manages recovery of failed or stuck payments.
//!
//! ## Lifecycle
//!
//! 1. An authorised reporter (e.g. the payment router) calls `report_failure`
//!    when a payment cannot be settled.  A `RecoveryCase` is created with
//!    status `Pending`.
//! 2. The admin (or an authorised resolver) calls `resolve` to either:
//!    - **Refund** the payer — tokens are transferred back from this contract's
//!      custody to the payer.
//!    - **Retry** the payment — tokens are forwarded to the intended recipient.
//!    - **Escalate** — the case is flagged for manual review (no token movement).
//! 3. Cases that are not resolved within `EXPIRY_LEDGERS` may be force-closed
//!    by anyone via `expire_case`, which automatically refunds the payer.
//!
//! ## Token custody
//!
//! When `report_failure` is called the reporter must have already transferred
//! the stuck funds to this contract (or the contract must hold them from a
//! prior lock).  The contract tracks the amount per case and moves tokens only
//! during `resolve` or `expire_case`.
//!
//! ## Access control
//!
//! - **Admin**: initialise, add/remove reporters, pause/unpause, upgrade,
//!   transfer admin, resolve any case.
//! - **Reporter**: report a failure (creates a case).
//! - **Anyone**: expire a case that has passed its deadline.

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, symbol_short,
    token::Client as TokenClient, Address, Bytes, Env, Symbol,
};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Default ledgers before an unresolved case can be force-expired (~7 days).
pub const DEFAULT_EXPIRY_LEDGERS: u32 = 120_960;

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
const KEY_LOCKED: Symbol = symbol_short!("locked");
const KEY_COUNTER: Symbol = symbol_short!("counter");
const KEY_EXPIRY: Symbol = symbol_short!("expiry");
const KEY_VERSION: Symbol = symbol_short!("version");

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    /// Authorised reporter whitelist.
    Reporter(Address),
    /// Recovery case by ID.
    Case(u64),
}

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Resolution action chosen by the admin.
#[contracttype]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Resolution {
    /// Return funds to the payer.
    Refund = 1,
    /// Forward funds to the intended recipient.
    Retry = 2,
    /// Flag for manual review — no token movement.
    Escalate = 3,
}

/// Status of a recovery case.
#[contracttype]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum CaseStatus {
    Pending = 1,
    Refunded = 2,
    Retried = 3,
    Escalated = 4,
    Expired = 5,
}

/// A failed-payment recovery case.
#[contracttype]
#[derive(Clone)]
pub struct RecoveryCase {
    pub id: u64,
    /// Address that reported the failure (the payment router).
    pub reporter: Address,
    /// Original payer — receives refund if resolved as `Refund` or expired.
    pub payer: Address,
    /// Intended recipient — receives funds if resolved as `Retry`.
    pub recipient: Address,
    /// Token contract address.
    pub token: Address,
    /// Amount held in custody by this contract.
    pub amount: i128,
    /// Opaque reference (e.g. original payment ID or tx hash).
    pub reference: Bytes,
    /// Ledger at which the case was created.
    pub created_ledger: u32,
    /// Ledger after which the case may be force-expired.
    pub expiry_ledger: u32,
    pub status: CaseStatus,
    /// Optional note added by the resolver.
    pub note: Bytes,
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[contracterror]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum RecoveryError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    ContractPaused = 4,
    Reentrant = 5,
    CaseNotFound = 6,
    CaseNotPending = 7,
    CaseNotExpired = 8,
    InvalidAmount = 9,
    ReporterAlreadyAdded = 10,
    ReporterNotFound = 11,
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
        .unwrap_or_else(|| panic_with_error!(env, RecoveryError::NotInitialized));
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
        panic_with_error!(env, RecoveryError::ContractPaused);
    }
}

fn require_reporter(env: &Env, reporter: &Address) {
    reporter.require_auth();
    if !env
        .storage()
        .persistent()
        .get::<DataKey, bool>(&DataKey::Reporter(reporter.clone()))
        .unwrap_or(false)
    {
        panic_with_error!(env, RecoveryError::Unauthorized);
    }
}

fn reentrancy_enter(env: &Env) {
    if env
        .storage()
        .instance()
        .get::<Symbol, bool>(&KEY_LOCKED)
        .unwrap_or(false)
    {
        panic_with_error!(env, RecoveryError::Reentrant);
    }
    env.storage().instance().set(&KEY_LOCKED, &true);
}

fn reentrancy_exit(env: &Env) {
    env.storage().instance().set(&KEY_LOCKED, &false);
}

fn next_id(env: &Env) -> u64 {
    let id: u64 = env.storage().instance().get(&KEY_COUNTER).unwrap_or(0);
    env.storage().instance().set(&KEY_COUNTER, &(id + 1));
    id
}

fn load_case(env: &Env, case_id: u64) -> RecoveryCase {
    env.storage()
        .persistent()
        .get(&DataKey::Case(case_id))
        .unwrap_or_else(|| panic_with_error!(env, RecoveryError::CaseNotFound))
}

fn save_case(env: &Env, case: &RecoveryCase) {
    env.storage()
        .persistent()
        .set(&DataKey::Case(case.id), case);
    bump_persistent(env, &DataKey::Case(case.id));
}

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

#[contract]
pub struct PaymentRecovery;

#[contractimpl]
impl PaymentRecovery {
    // -----------------------------------------------------------------------
    // Initialisation
    // -----------------------------------------------------------------------

    /// Initialise the contract.
    ///
    /// * `admin`          – address that manages reporters and resolves cases
    /// * `expiry_ledgers` – ledgers before an unresolved case can be expired
    ///                      (pass 0 to use the default of ~7 days)
    pub fn initialize(env: Env, admin: Address, expiry_ledgers: u32) {
        if env.storage().instance().has(&KEY_ADMIN) {
            panic_with_error!(env, RecoveryError::AlreadyInitialized);
        }

        admin.require_auth();

        let expiry = if expiry_ledgers == 0 {
            DEFAULT_EXPIRY_LEDGERS
        } else {
            expiry_ledgers
        };

        env.storage().instance().set(&KEY_ADMIN, &admin);
        env.storage().instance().set(&KEY_EXPIRY, &expiry);
        env.storage().instance().set(&KEY_PAUSED, &false);
        env.storage().instance().set(&KEY_LOCKED, &false);
        env.storage().instance().set(&KEY_COUNTER, &0u64);
        env.storage().instance().set(&KEY_VERSION, &1u32);
        bump_instance(&env);

        env.events().publish(
            (symbol_short!("recovery"), symbol_short!("init")),
            admin,
        );
    }

    // -----------------------------------------------------------------------
    // Reporter management (admin only)
    // -----------------------------------------------------------------------

    /// Authorise an address to report payment failures.
    pub fn add_reporter(env: Env, reporter: Address) {
        require_admin(&env);
        bump_instance(&env);

        let key = DataKey::Reporter(reporter.clone());
        if env
            .storage()
            .persistent()
            .get::<DataKey, bool>(&key)
            .unwrap_or(false)
        {
            panic_with_error!(env, RecoveryError::ReporterAlreadyAdded);
        }

        env.storage().persistent().set(&key, &true);
        bump_persistent(&env, &key);

        env.events().publish(
            (symbol_short!("recovery"), symbol_short!("rptr_add")),
            reporter,
        );
    }

    /// Remove a reporter.
    pub fn remove_reporter(env: Env, reporter: Address) {
        require_admin(&env);
        bump_instance(&env);

        let key = DataKey::Reporter(reporter.clone());
        if !env
            .storage()
            .persistent()
            .get::<DataKey, bool>(&key)
            .unwrap_or(false)
        {
            panic_with_error!(env, RecoveryError::ReporterNotFound);
        }

        env.storage().persistent().remove(&key);

        env.events().publish(
            (symbol_short!("recovery"), symbol_short!("rptr_rm")),
            reporter,
        );
    }

    // -----------------------------------------------------------------------
    // Core: report a failure
    // -----------------------------------------------------------------------

    /// Report a failed payment and create a recovery case.
    ///
    /// The reporter must have already transferred `amount` tokens to this
    /// contract before calling this function.
    ///
    /// * `reporter`   – authorised reporter (must sign)
    /// * `payer`      – original payer (receives refund if case is refunded)
    /// * `recipient`  – intended recipient (receives funds if case is retried)
    /// * `token`      – token contract address
    /// * `amount`     – amount held in custody (> 0)
    /// * `reference`  – opaque reference bytes (e.g. original payment ID)
    ///
    /// Returns the new case ID.
    pub fn report_failure(
        env: Env,
        reporter: Address,
        payer: Address,
        recipient: Address,
        token: Address,
        amount: i128,
        reference: Bytes,
    ) -> u64 {
        require_not_paused(&env);
        bump_instance(&env);

        require_reporter(&env, &reporter);

        if amount <= 0 {
            panic_with_error!(env, RecoveryError::InvalidAmount);
        }

        let case_id = next_id(&env);
        let current_ledger = env.ledger().sequence();
        let expiry_ledgers: u32 = env
            .storage()
            .instance()
            .get(&KEY_EXPIRY)
            .unwrap_or(DEFAULT_EXPIRY_LEDGERS);

        let case = RecoveryCase {
            id: case_id,
            reporter: reporter.clone(),
            payer: payer.clone(),
            recipient: recipient.clone(),
            token,
            amount,
            reference: reference.clone(),
            created_ledger: current_ledger,
            expiry_ledger: current_ledger + expiry_ledgers,
            status: CaseStatus::Pending,
            note: Bytes::new(&env),
        };

        save_case(&env, &case);

        env.events().publish(
            (symbol_short!("recovery"), symbol_short!("reported")),
            (case_id, reporter, payer, recipient, amount, reference),
        );

        case_id
    }

    // -----------------------------------------------------------------------
    // Core: resolve a case (admin only)
    // -----------------------------------------------------------------------

    /// Resolve a pending recovery case.
    ///
    /// * `case_id`    – the case to resolve
    /// * `resolution` – Refund | Retry | Escalate
    /// * `note`       – optional resolver note (stored on the case)
    pub fn resolve(env: Env, case_id: u64, resolution: Resolution, note: Bytes) {
        require_not_paused(&env);
        require_admin(&env);
        bump_instance(&env);

        let mut case = load_case(&env, case_id);

        if case.status != CaseStatus::Pending {
            panic_with_error!(env, RecoveryError::CaseNotPending);
        }

        reentrancy_enter(&env);

        // Determine new status and token destination.
        let (new_status, token_dest) = match resolution {
            Resolution::Refund => (CaseStatus::Refunded, Some(case.payer.clone())),
            Resolution::Retry => (CaseStatus::Retried, Some(case.recipient.clone())),
            Resolution::Escalate => (CaseStatus::Escalated, None),
        };

        // Effects: update state before any token transfer.
        case.status = new_status;
        case.note = note;
        save_case(&env, &case);

        // Interaction: transfer tokens if applicable.
        if let Some(dest) = token_dest {
            TokenClient::new(&env, &case.token).transfer(
                &env.current_contract_address(),
                &dest,
                &case.amount,
            );
        }

        reentrancy_exit(&env);

        env.events().publish(
            (symbol_short!("recovery"), symbol_short!("resolved")),
            (case_id, resolution as u32, new_status as u32),
        );
    }

    // -----------------------------------------------------------------------
    // Core: expire a stale case (permissionless)
    // -----------------------------------------------------------------------

    /// Force-expire a case that has passed its expiry ledger.
    ///
    /// Anyone may call this.  Funds are automatically refunded to the payer.
    pub fn expire_case(env: Env, case_id: u64) {
        require_not_paused(&env);
        bump_instance(&env);

        let mut case = load_case(&env, case_id);

        if case.status != CaseStatus::Pending {
            panic_with_error!(env, RecoveryError::CaseNotPending);
        }

        if env.ledger().sequence() <= case.expiry_ledger {
            panic_with_error!(env, RecoveryError::CaseNotExpired);
        }

        reentrancy_enter(&env);

        // Effects before interaction.
        case.status = CaseStatus::Expired;
        save_case(&env, &case);

        // Refund payer.
        TokenClient::new(&env, &case.token).transfer(
            &env.current_contract_address(),
            &case.payer,
            &case.amount,
        );

        reentrancy_exit(&env);

        env.events().publish(
            (symbol_short!("recovery"), symbol_short!("expired")),
            (case_id, case.payer, case.amount),
        );
    }

    // -----------------------------------------------------------------------
    // Admin: circuit breaker / upgrade / transfer admin
    // -----------------------------------------------------------------------

    pub fn pause(env: Env) {
        require_admin(&env);
        env.storage().instance().set(&KEY_PAUSED, &true);
        env.events()
            .publish((symbol_short!("recovery"), symbol_short!("paused")), ());
    }

    pub fn unpause(env: Env) {
        require_admin(&env);
        env.storage().instance().set(&KEY_PAUSED, &false);
        env.events()
            .publish((symbol_short!("recovery"), symbol_short!("unpaused")), ());
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
            (symbol_short!("recovery"), symbol_short!("adm_xfer")),
            new_admin,
        );
    }

    /// Update the default expiry window for new cases.
    pub fn set_expiry_ledgers(env: Env, expiry_ledgers: u32) {
        require_admin(&env);
        bump_instance(&env);
        env.storage().instance().set(&KEY_EXPIRY, &expiry_ledgers);
        env.events().publish(
            (symbol_short!("recovery"), symbol_short!("exp_upd")),
            expiry_ledgers,
        );
    }

    // -----------------------------------------------------------------------
    // Views
    // -----------------------------------------------------------------------

    pub fn get_case(env: Env, case_id: u64) -> RecoveryCase {
        load_case(&env, case_id)
    }

    pub fn case_count(env: Env) -> u64 {
        env.storage().instance().get(&KEY_COUNTER).unwrap_or(0)
    }

    pub fn is_reporter(env: Env, reporter: Address) -> bool {
        env.storage()
            .persistent()
            .get::<DataKey, bool>(&DataKey::Reporter(reporter))
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
            .unwrap_or_else(|| panic_with_error!(env, RecoveryError::NotInitialized))
    }

    pub fn get_expiry_ledgers(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&KEY_EXPIRY)
            .unwrap_or(DEFAULT_EXPIRY_LEDGERS)
    }

    pub fn get_version(env: Env) -> u32 {
        env.storage().instance().get(&KEY_VERSION).unwrap_or(1)
    }
}


