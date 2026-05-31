#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, symbol_short,
    token::Client as TokenClient, Address, Env, Symbol,
};

const INSTANCE_TTL_THRESHOLD: u32 = 100_000;
const INSTANCE_TTL_EXTEND: u32 = 6_307_200;
const PERSISTENT_TTL_THRESHOLD: u32 = 50_000;
const PERSISTENT_TTL_EXTEND: u32 = 3_153_600;

#[contracttype]
#[derive(Clone)]
enum DataKey {
    Admin,
    Paused,
    Counter,
    Schedule(u64),
}

#[contracttype]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum ScheduleKind {
    OneTime = 1,
    Recurring = 2,
}

#[contracttype]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum ScheduleStatus {
    Pending = 1,
    Executed = 2,
    Cancelled = 3,
}

#[contracttype]
#[derive(Clone)]
pub struct Schedule {
    pub payer: Address,
    pub recipient: Address,
    pub token: Address,
    pub amount: i128,
    pub execute_after: u64,
    pub interval_seconds: u64,
    pub kind: ScheduleKind,
    pub status: ScheduleStatus,
    pub executions: u32,
    pub max_executions: u32,
}

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
    InvalidExecuteAfter = 10,
    InvalidExecutionLimit = 11,
    ArithmeticOverflow = 12,
}

fn bump_instance(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(INSTANCE_TTL_THRESHOLD, INSTANCE_TTL_EXTEND);
}

fn bump_schedule(env: &Env, id: u64) {
    env.storage().persistent().extend_ttl(
        &DataKey::Schedule(id),
        PERSISTENT_TTL_THRESHOLD,
        PERSISTENT_TTL_EXTEND,
    );
}

fn require_initialized(env: &Env) {
    if !env.storage().instance().has(&DataKey::Admin) {
        panic_with_error!(env, Error::NotInitialized);
    }
}

fn require_not_paused(env: &Env) {
    require_initialized(env);
    let paused = env
        .storage()
        .instance()
        .get(&DataKey::Paused)
        .unwrap_or(false);
    if paused {
        panic_with_error!(env, Error::ContractPaused);
    }
}

fn require_admin(env: &Env) -> Address {
    let admin: Address = env
        .storage()
        .instance()
        .get(&DataKey::Admin)
        .unwrap_or_else(|| panic_with_error!(env, Error::NotInitialized));
    admin.require_auth();
    admin
}

fn checked_reserved(env: &Env, amount: i128, executions: u32) -> i128 {
    if amount <= 0 {
        panic_with_error!(env, Error::InvalidAmount);
    }
    amount
        .checked_mul(executions as i128)
        .unwrap_or_else(|| panic_with_error!(env, Error::ArithmeticOverflow))
}

fn validate_schedule(
    env: &Env,
    amount: i128,
    execute_after: u64,
    interval_seconds: u64,
    kind: ScheduleKind,
    max_executions: u32,
) {
    if amount <= 0 {
        panic_with_error!(env, Error::InvalidAmount);
    }
    if execute_after < env.ledger().timestamp() {
        panic_with_error!(env, Error::InvalidExecuteAfter);
    }
    match kind {
        ScheduleKind::OneTime => {
            if max_executions != 1 || interval_seconds != 0 {
                panic_with_error!(env, Error::InvalidExecutionLimit);
            }
        }
        ScheduleKind::Recurring => {
            if interval_seconds == 0 {
                panic_with_error!(env, Error::InvalidInterval);
            }
            if max_executions == 0 {
                panic_with_error!(env, Error::InvalidExecutionLimit);
            }
        }
    }
}

fn load_schedule(env: &Env, id: u64) -> Schedule {
    let schedule = env
        .storage()
        .persistent()
        .get(&DataKey::Schedule(id))
        .unwrap_or_else(|| panic_with_error!(env, Error::ScheduleNotFound));
    bump_schedule(env, id);
    schedule
}

fn save_schedule(env: &Env, id: u64, schedule: &Schedule) {
    env.storage()
        .persistent()
        .set(&DataKey::Schedule(id), schedule);
    bump_schedule(env, id);
}

fn remaining_executions(schedule: &Schedule) -> u32 {
    schedule.max_executions.saturating_sub(schedule.executions)
}

fn emit(env: &Env, action: Symbol, id: u64) {
    env.events()
        .publish((symbol_short!("schedule"), action), id);
}

#[contract]
pub struct PaymentScheduler;

#[contractimpl]
impl PaymentScheduler {
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic_with_error!(env, Error::AlreadyInitialized);
        }
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Paused, &false);
        env.storage().instance().set(&DataKey::Counter, &0u64);
        bump_instance(&env);
    }

    pub fn schedule(
        env: Env,
        payer: Address,
        recipient: Address,
        token: Address,
        amount: i128,
        execute_after: u64,
        interval_seconds: u64,
        kind: ScheduleKind,
        max_executions: u32,
    ) -> u64 {
        require_not_paused(&env);
        payer.require_auth();
        validate_schedule(
            &env,
            amount,
            execute_after,
            interval_seconds,
            kind,
            max_executions,
        );

        let reserved = checked_reserved(&env, amount, max_executions);
        let contract = env.current_contract_address();
        TokenClient::new(&env, &token).transfer(&payer, &contract, &reserved);

        let id = env
            .storage()
            .instance()
            .get(&DataKey::Counter)
            .unwrap_or(0u64)
            .checked_add(1)
            .unwrap_or_else(|| panic_with_error!(env, Error::ArithmeticOverflow));

        let schedule = Schedule {
            payer,
            recipient,
            token,
            amount,
            execute_after,
            interval_seconds,
            kind,
            status: ScheduleStatus::Pending,
            executions: 0,
            max_executions,
        };

        save_schedule(&env, id, &schedule);
        env.storage().instance().set(&DataKey::Counter, &id);
        bump_instance(&env);
        emit(&env, symbol_short!("created"), id);
        id
    }

    pub fn execute(env: Env, id: u64) {
        require_not_paused(&env);
        let mut schedule = load_schedule(&env, id);

        if schedule.status != ScheduleStatus::Pending {
            panic_with_error!(env, Error::NotPending);
        }
        if env.ledger().timestamp() < schedule.execute_after {
            panic_with_error!(env, Error::NotDue);
        }

        let contract = env.current_contract_address();
        TokenClient::new(&env, &schedule.token).transfer(
            &contract,
            &schedule.recipient,
            &schedule.amount,
        );

        schedule.executions = schedule
            .executions
            .checked_add(1)
            .unwrap_or_else(|| panic_with_error!(env, Error::ArithmeticOverflow));

        if schedule.executions >= schedule.max_executions {
            schedule.status = ScheduleStatus::Executed;
        } else {
            schedule.execute_after = env
                .ledger()
                .timestamp()
                .checked_add(schedule.interval_seconds)
                .unwrap_or_else(|| panic_with_error!(env, Error::ArithmeticOverflow));
        }

        save_schedule(&env, id, &schedule);
        bump_instance(&env);
        emit(&env, symbol_short!("executed"), id);
    }

    pub fn cancel(env: Env, caller: Address, id: u64) {
        require_not_paused(&env);
        caller.require_auth();

        let mut schedule = load_schedule(&env, id);
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(env, Error::NotInitialized));

        if caller != schedule.payer && caller != admin {
            panic_with_error!(env, Error::Unauthorized);
        }
        if schedule.status != ScheduleStatus::Pending {
            panic_with_error!(env, Error::NotPending);
        }

        let refund = checked_reserved(&env, schedule.amount, remaining_executions(&schedule));
        schedule.status = ScheduleStatus::Cancelled;
        save_schedule(&env, id, &schedule);

        if refund > 0 {
            TokenClient::new(&env, &schedule.token).transfer(
                &env.current_contract_address(),
                &schedule.payer,
                &refund,
            );
        }
        bump_instance(&env);
        emit(&env, symbol_short!("cancelled"), id);
    }

    pub fn modify(
        env: Env,
        payer: Address,
        id: u64,
        new_recipient: Option<Address>,
        new_amount: i128,
        new_execute_after: u64,
        new_interval_seconds: u64,
        new_max_executions: u32,
    ) {
        require_not_paused(&env);
        payer.require_auth();

        let mut schedule = load_schedule(&env, id);
        if schedule.payer != payer {
            panic_with_error!(env, Error::Unauthorized);
        }
        if schedule.status != ScheduleStatus::Pending {
            panic_with_error!(env, Error::NotPending);
        }

        let old_reserved = checked_reserved(&env, schedule.amount, remaining_executions(&schedule));

        if let Some(recipient) = new_recipient {
            schedule.recipient = recipient;
        }
        if new_amount < 0 {
            panic_with_error!(env, Error::InvalidAmount);
        }
        if new_amount > 0 {
            schedule.amount = new_amount;
        }
        if new_execute_after > 0 {
            if new_execute_after < env.ledger().timestamp() {
                panic_with_error!(env, Error::InvalidExecuteAfter);
            }
            schedule.execute_after = new_execute_after;
        }
        if new_interval_seconds > 0 {
            if schedule.kind == ScheduleKind::OneTime {
                panic_with_error!(env, Error::InvalidInterval);
            }
            schedule.interval_seconds = new_interval_seconds;
        }
        if new_max_executions > 0 {
            if schedule.kind == ScheduleKind::OneTime && new_max_executions != 1 {
                panic_with_error!(env, Error::InvalidExecutionLimit);
            }
            if new_max_executions <= schedule.executions {
                panic_with_error!(env, Error::InvalidExecutionLimit);
            }
            schedule.max_executions = new_max_executions;
        }

        validate_schedule(
            &env,
            schedule.amount,
            schedule.execute_after,
            schedule.interval_seconds,
            schedule.kind,
            schedule.max_executions,
        );

        let new_reserved = checked_reserved(&env, schedule.amount, remaining_executions(&schedule));
        let token = TokenClient::new(&env, &schedule.token);
        let contract = env.current_contract_address();
        if new_reserved > old_reserved {
            let top_up = new_reserved - old_reserved;
            token.transfer(&payer, &contract, &top_up);
        } else if old_reserved > new_reserved {
            let refund = old_reserved - new_reserved;
            token.transfer(&contract, &schedule.payer, &refund);
        }

        save_schedule(&env, id, &schedule);
        bump_instance(&env);
        emit(&env, symbol_short!("modified"), id);
    }

    pub fn pause(env: Env) {
        require_admin(&env);
        env.storage().instance().set(&DataKey::Paused, &true);
        bump_instance(&env);
        env.events()
            .publish((symbol_short!("schedule"), symbol_short!("paused")), ());
    }

    pub fn unpause(env: Env) {
        require_admin(&env);
        env.storage().instance().set(&DataKey::Paused, &false);
        bump_instance(&env);
        env.events()
            .publish((symbol_short!("schedule"), symbol_short!("unpaused")), ());
    }

    pub fn transfer_admin(env: Env, new_admin: Address) {
        require_admin(&env);
        env.storage().instance().set(&DataKey::Admin, &new_admin);
        bump_instance(&env);
        env.events().publish(
            (symbol_short!("schedule"), symbol_short!("adm_xfer")),
            new_admin,
        );
    }

    pub fn upgrade(env: Env, new_wasm_hash: soroban_sdk::BytesN<32>) {
        require_admin(&env);
        env.deployer().update_current_contract_wasm(new_wasm_hash);
    }

    pub fn get_schedule(env: Env, id: u64) -> Schedule {
        load_schedule(&env, id)
    }

    pub fn get_admin(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(env, Error::NotInitialized))
    }

    pub fn is_paused(env: Env) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::Paused)
            .unwrap_or(false)
    }

    pub fn schedule_count(env: Env) -> u64 {
        env.storage().instance().get(&DataKey::Counter).unwrap_or(0)
    }
}

mod test;
