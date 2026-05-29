#![no_std]

//! # Payment Scheduler Contract
//!
//! Schedules one-time and recurring payments for future execution.
//!
//! ## Lifecycle
//! 1. Admin calls `initialize`.
//! 2. A payer calls `schedule` to create a one-time or recurring schedule.
//! 3. Anyone calls `execute` once the due ledger is reached.
//! 4. The payer (or admin) may call `cancel` before execution.
//! 5. The payer may call `modify` to update amount/interval before execution.
//!
//! ## Access Control
//! - Admin: initialize, pause/unpause, upgrade.
//! - Payer: schedule, cancel, modify their own schedules.
//! - Anyone: execute a due schedule (permissionless keeper).

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, symbol_short,
    token::Client as TokenClient, Address, Env,
};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

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
    Paused,
    Counter,
    Schedule(u64),
}

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Whether a schedule fires once or repeats.
#[contracttype]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum ScheduleKind {
    OneTime = 1,
    Recurring = 2,
}

/// Lifecycle state of a schedule.
#[contracttype]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum ScheduleStatus {
    Pending = 1,
    Executed = 2,
    Cancelled = 3,
}

/// A payment schedule entry.
#[contracttype]
#[derive(Clone)]
pub struct Schedule {
    pub payer: Address,
    pub recipient: Address,
    pub token: Address,
    pub amount: i128,
    /// Ledger at which the next execution is due.
    pub execute_at: u32,
    /// For recurring schedules: ledgers between executions. 0 for one-time.
    pub interval_ledgers: u32,
    pub kind: ScheduleKind,
    pub status: ScheduleStatus,
    pub executions: u32,
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[contracterror]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Error {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    ContractPaused = 4,
    ScheduleNotFound = 5,
    NotPending = 6,
    NotDue = 7,
    InvalidAmount = 8,
    InvalidInterval = 9,
    InvalidExecuteAt = 10,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn bump_instance(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(TTL_THRESHOLD, TTL_EXTEND);
}

fn bump_persistent(env: &Env, key: &Key) {
    env.storage()
        .persistent()
        .extend_ttl(key, PERSISTENT_TTL_THRESHOLD, PERSISTENT_TTL_EXTEND);
}

fn require_not_paused(env: &Env) {
    if env
        .storage()
        .instance()
        .get(&Key::Paused)
        .unwrap_or(false)
    {
        panic_with_error!(env, Error::ContractPaused);
    }
}

fn require_admin(env: &Env) -> Address {
    let admin: Address = env
        .storage()
        .instance()
        .get(&Key::Admin)
        .unwrap_or_else(|| panic_with_error!(env, Error::NotInitialized));
    admin.require_auth();
    admin
}

fn load_schedule(env: &Env, id: u64) -> Schedule {
    env.storage()
        .persistent()
        .get(&Key::Schedule(id))
        .unwrap_or_else(|| panic_with_error!(env, Error::ScheduleNotFound))
}

fn save_schedule(env: &Env, id: u64, s: &Schedule) {
    env.storage().persistent().set(&Key::Schedule(id), s);
    bump_persistent(env, &Key::Schedule(id));
}

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

#[contract]
pub struct PaymentScheduler;

#[contractimpl]
impl PaymentScheduler {
    // -----------------------------------------------------------------------
    // Initialisation
    // -----------------------------------------------------------------------

    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&Key::Admin) {
            panic_with_error!(env, Error::AlreadyInitialized);
        }
        admin.require_auth();
        env.storage().instance().set(&Key::Admin, &admin);
        env.storage().instance().set(&Key::Paused, &false);
        env.storage().instance().set(&Key::Counter, &0u64);
        bump_instance(&env);
    }

    // -----------------------------------------------------------------------
    // Schedule creation
    // -----------------------------------------------------------------------

    /// Create a payment schedule.
    ///
    /// * `payer`            – address that will be charged (must sign)
    /// * `recipient`        – address that receives the payment
    /// * `token`            – token contract address
    /// * `amount`           – amount per execution (> 0)
    /// * `execute_at`       – ledger at which first execution is due (>= current)
    /// * `interval_ledgers` – for recurring: ledgers between executions (> 0); ignored for one-time
    /// * `kind`             – `OneTime` or `Recurring`
    ///
    /// Returns the new schedule ID.
    pub fn schedule(
        env: Env,
        payer: Address,
        recipient: Address,
        token: Address,
        amount: i128,
        execute_at: u32,
        interval_ledgers: u32,
        kind: ScheduleKind,
    ) -> u64 {
        require_not_paused(&env);
        bump_instance(&env);

        payer.require_auth();

        if amount <= 0 {
            panic_with_error!(env, Error::InvalidAmount);
        }
        if execute_at < env.ledger().sequence() {
            panic_with_error!(env, Error::InvalidExecuteAt);
        }
        if kind == ScheduleKind::Recurring && interval_ledgers == 0 {
            panic_with_error!(env, Error::InvalidInterval);
        }

        let id: u64 = env
            .storage()
            .instance()
            .get(&Key::Counter)
            .unwrap_or(0u64)
            + 1;

        let s = Schedule {
            payer: payer.clone(),
            recipient: recipient.clone(),
            token,
            amount,
            execute_at,
            interval_ledgers,
            kind,
            status: ScheduleStatus::Pending,
            executions: 0,
        };

        save_schedule(&env, id, &s);
        env.storage().instance().set(&Key::Counter, &id);

        env.events().publish(
            (symbol_short!("sched"), symbol_short!("created")),
            (id, payer, recipient, amount, execute_at),
        );

        id
    }

    // -----------------------------------------------------------------------
    // Cancellation
    // -----------------------------------------------------------------------

    /// Cancel a pending schedule. Only the payer or admin may cancel.
    pub fn cancel(env: Env, caller: Address, id: u64) {
        require_not_paused(&env);
        bump_instance(&env);

        caller.require_auth();

        let mut s = load_schedule(&env, id);

        // Allow payer or admin.
        let admin: Address = env
            .storage()
            .instance()
            .get(&Key::Admin)
            .unwrap_or_else(|| panic_with_error!(env, Error::NotInitialized));
        if caller != s.payer && caller != admin {
            panic_with_error!(env, Error::Unauthorized);
        }

        if s.status != ScheduleStatus::Pending {
            panic_with_error!(env, Error::NotPending);
        }

        s.status = ScheduleStatus::Cancelled;
        save_schedule(&env, id, &s);

        env.events().publish(
            (symbol_short!("sched"), symbol_short!("cancelled")),
            (id, caller),
        );
    }

    // -----------------------------------------------------------------------
    // Modification
    // -----------------------------------------------------------------------

    /// Modify a pending schedule's amount and/or interval. Only the payer may modify.
    ///
    /// Pass `0` for `new_amount` or `new_interval` to leave that field unchanged.
    pub fn modify(env: Env, payer: Address, id: u64, new_amount: i128, new_interval: u32) {
        require_not_paused(&env);
        bump_instance(&env);

        payer.require_auth();

        let mut s = load_schedule(&env, id);

        if s.payer != payer {
            panic_with_error!(env, Error::Unauthorized);
        }
        if s.status != ScheduleStatus::Pending {
            panic_with_error!(env, Error::NotPending);
        }

        if new_amount > 0 {
            s.amount = new_amount;
        }
        if new_interval > 0 {
            if s.kind == ScheduleKind::OneTime {
                panic_with_error!(env, Error::InvalidInterval);
            }
            s.interval_ledgers = new_interval;
        }

        save_schedule(&env, id, &s);

        env.events().publish(
            (symbol_short!("sched"), symbol_short!("modified")),
            (id, s.amount, s.interval_ledgers),
        );
    }

    // -----------------------------------------------------------------------
    // Execution (permissionless keeper)
    // -----------------------------------------------------------------------

    /// Execute a due schedule. Anyone may call this.
    ///
    /// For one-time schedules the status becomes `Executed` after success.
    /// For recurring schedules the `execute_at` advances by `interval_ledgers`.
    pub fn execute(env: Env, id: u64) {
        require_not_paused(&env);
        bump_instance(&env);

        let mut s = load_schedule(&env, id);

        if s.status != ScheduleStatus::Pending {
            panic_with_error!(env, Error::NotPending);
        }
        if env.ledger().sequence() < s.execute_at {
            panic_with_error!(env, Error::NotDue);
        }

        // Transfer from payer to recipient.
        TokenClient::new(&env, &s.token).transfer(&s.payer, &s.recipient, &s.amount);

        s.executions += 1;

        match s.kind {
            ScheduleKind::OneTime => {
                s.status = ScheduleStatus::Executed;
            }
            ScheduleKind::Recurring => {
                s.execute_at = env.ledger().sequence() + s.interval_ledgers;
                // status stays Pending for next execution
            }
        }

        save_schedule(&env, id, &s);

        env.events().publish(
            (symbol_short!("sched"), symbol_short!("executed")),
            (id, s.payer, s.recipient, s.amount, s.executions),
        );
    }

    // -----------------------------------------------------------------------
    // Admin: pause / unpause / upgrade / transfer_admin
    // -----------------------------------------------------------------------

    pub fn pause(env: Env) {
        require_admin(&env);
        env.storage().instance().set(&Key::Paused, &true);
        env.events()
            .publish((symbol_short!("sched"), symbol_short!("paused")), ());
    }

    pub fn unpause(env: Env) {
        require_admin(&env);
        env.storage().instance().set(&Key::Paused, &false);
        env.events()
            .publish((symbol_short!("sched"), symbol_short!("unpaused")), ());
    }

    pub fn upgrade(env: Env, new_wasm_hash: soroban_sdk::BytesN<32>) {
        require_admin(&env);
        env.deployer().update_current_contract_wasm(new_wasm_hash);
    }

    pub fn transfer_admin(env: Env, new_admin: Address) {
        require_admin(&env);
        env.storage().instance().set(&Key::Admin, &new_admin);
        env.events().publish(
            (symbol_short!("sched"), symbol_short!("adm_xfer")),
            new_admin,
        );
    }

    // -----------------------------------------------------------------------
    // Views
    // -----------------------------------------------------------------------

    pub fn get_schedule(env: Env, id: u64) -> Schedule {
        load_schedule(&env, id)
    }

    pub fn get_admin(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&Key::Admin)
            .unwrap_or_else(|| panic_with_error!(env, Error::NotInitialized))
    }

    pub fn is_paused(env: Env) -> bool {
        env.storage().instance().get(&Key::Paused).unwrap_or(false)
    }

    pub fn schedule_count(env: Env) -> u64 {
        env.storage().instance().get(&Key::Counter).unwrap_or(0)
    }
}

mod test;
