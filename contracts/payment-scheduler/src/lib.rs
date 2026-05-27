#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, symbol_short,
    Address, Bytes, Env, Symbol,
};

const KEY_ADMIN: Symbol = symbol_short!("admin");
const KEY_VERSION: Symbol = symbol_short!("version");

#[contracttype]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum ScheduleType {
    OneTime = 1,
    Recurring = 2,
}

#[contracttype]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum ScheduleStatus {
    Active = 1,
    Paused = 2,
    Completed = 3,
    Cancelled = 4,
}

#[contracttype]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum RecurringInterval {
    Daily = 1,
    Weekly = 2,
    Monthly = 3,
}

#[contracttype]
#[derive(Clone)]
pub struct Schedule {
    pub id: u64,
    pub from_addr: Address,
    pub to_addr: Address,
    pub token: Address,
    pub amount: i128,
    pub schedule_type: ScheduleType,
    pub start_ledger: u32,
    pub next_execution_ledger: u32,
    pub interval_ledgers: u32,
    pub recurring_interval: Option<RecurringInterval>,
    pub max_executions: u32,
    pub executions_so_far: u32,
    pub status: ScheduleStatus,
    pub memo: Bytes,
}

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Schedule(u64),
    NextScheduleId,
    OwnerSchedule(Address, u64),
    OwnerScheduleCount(Address),
}

#[contracterror]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum SchedulerError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    ScheduleNotFound = 4,
    ScheduleNotActive = 5,
    ScheduleCompleted = 6,
    ScheduleCancelled = 7,
    InvalidAmount = 8,
    InvalidInterval = 9,
    MaxExecutionsReached = 10,
    InvalidLedger = 11,
}

fn bump_instance(env: &Env) {
    let ttl = 6_307_200u32;
    let threshold = 100_000u32;
    env.storage().instance().extend_ttl(threshold, ttl);
}

fn bump_persistent<K>(env: &Env, key: &K)
where
    K: soroban_sdk::IntoVal<Env, soroban_sdk::Val>,
{
    env.storage()
        .persistent()
        .extend_ttl(key, 50_000u32, 3_153_600u32);
}

fn require_admin(env: &Env) -> Address {
    let admin: Address = env
        .storage()
        .instance()
        .get(&KEY_ADMIN)
        .unwrap_or_else(|| panic_with_error!(env, SchedulerError::NotInitialized));
    admin.require_auth();
    admin
}

fn save_schedule(env: &Env, schedule: &Schedule) {
    env.storage()
        .persistent()
        .set(&DataKey::Schedule(schedule.id), schedule);
    bump_persistent(env, &DataKey::Schedule(schedule.id));
}

fn load_schedule(env: &Env, schedule_id: u64) -> Schedule {
    env.storage()
        .persistent()
        .get(&DataKey::Schedule(schedule_id))
        .unwrap_or_else(|| panic_with_error!(env, SchedulerError::ScheduleNotFound))
}

#[contract]
pub struct PaymentScheduler;

#[contractimpl]
impl PaymentScheduler {
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&KEY_ADMIN) {
            panic_with_error!(env, SchedulerError::AlreadyInitialized);
        }
        admin.require_auth();

        env.storage().instance().set(&KEY_ADMIN, &admin);
        env.storage().instance().set(&KEY_VERSION, &1u32);
        bump_instance(&env);
    }

    pub fn create_schedule(
        env: Env,
        from: Address,
        to: Address,
        token: Address,
        amount: i128,
        schedule_type: ScheduleType,
        start_delay_ledgers: u32,
        interval_ledgers: u32,
        recurring_interval: Option<RecurringInterval>,
        max_executions: u32,
        memo: Bytes,
    ) -> u64 {
        from.require_auth();

        if amount <= 0 {
            panic_with_error!(env, SchedulerError::InvalidAmount);
        }
        if !env.storage().instance().has(&KEY_ADMIN) {
            panic_with_error!(env, SchedulerError::NotInitialized);
        }

        let next_id: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::NextScheduleId)
            .unwrap_or(0);
        let schedule_id = next_id + 1;

        let current_ledger = env.ledger().sequence();
        let schedule = Schedule {
            id: schedule_id,
            from_addr: from.clone(),
            to_addr: to.clone(),
            token,
            amount,
            schedule_type,
            start_ledger: current_ledger,
            next_execution_ledger: current_ledger + start_delay_ledgers,
            interval_ledgers,
            recurring_interval,
            max_executions,
            executions_so_far: 0,
            status: ScheduleStatus::Active,
            memo,
        };

        save_schedule(&env, &schedule);
        env.storage()
            .persistent()
            .set(&DataKey::NextScheduleId, &schedule_id);

        let owner_count: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::OwnerScheduleCount(from.clone()))
            .unwrap_or(0);
        env.storage()
            .persistent()
            .set(&DataKey::OwnerSchedule(from.clone(), owner_count), &schedule_id);
        env.storage()
            .persistent()
            .set(&DataKey::OwnerScheduleCount(from), &(owner_count + 1));

        env.events().publish(
            (symbol_short!("scheduler"), symbol_short!("created")),
            (schedule_id, from, to, amount as u128),
        );

        schedule_id
    }

    pub fn cancel_schedule(env: Env, schedule_id: u64, caller: Address) {
        caller.require_auth();

        let mut schedule = load_schedule(&env, schedule_id);

        if caller != schedule.from_addr {
            let admin: Address = require_admin(&env);
            if caller != admin {
                panic_with_error!(env, SchedulerError::Unauthorized);
            }
        }

        if schedule.status == ScheduleStatus::Cancelled {
            panic_with_error!(env, SchedulerError::ScheduleCancelled);
        }
        if schedule.status == ScheduleStatus::Completed {
            panic_with_error!(env, SchedulerError::ScheduleCompleted);
        }

        schedule.status = ScheduleStatus::Cancelled;
        save_schedule(&env, &schedule);

        env.events().publish(
            (symbol_short!("scheduler"), symbol_short!("cancelled")),
            (schedule_id, caller),
        );
    }

    pub fn pause_schedule(env: Env, schedule_id: u64, caller: Address) {
        caller.require_auth();

        let mut schedule = load_schedule(&env, schedule_id);

        if caller != schedule.from_addr {
            panic_with_error!(env, SchedulerError::Unauthorized);
        }
        if schedule.status != ScheduleStatus::Active {
            panic_with_error!(env, SchedulerError::ScheduleNotActive);
        }

        schedule.status = ScheduleStatus::Paused;
        save_schedule(&env, &schedule);

        env.events().publish(
            (symbol_short!("scheduler"), symbol_short!("paused")),
            (schedule_id, caller),
        );
    }

    pub fn resume_schedule(env: Env, schedule_id: u64, caller: Address) {
        caller.require_auth();

        let mut schedule = load_schedule(&env, schedule_id);

        if caller != schedule.from_addr {
            panic_with_error!(env, SchedulerError::Unauthorized);
        }
        if schedule.status != ScheduleStatus::Paused {
            panic_with_error!(env, SchedulerError::ScheduleNotActive);
        }

        schedule.status = ScheduleStatus::Active;
        save_schedule(&env, &schedule);

        env.events().publish(
            (symbol_short!("scheduler"), symbol_short!("resumed")),
            (schedule_id, caller),
        );
    }

    pub fn modify_schedule(
        env: Env,
        schedule_id: u64,
        caller: Address,
        amount: i128,
        interval_ledgers: u32,
        max_executions: u32,
    ) {
        caller.require_auth();

        let mut schedule = load_schedule(&env, schedule_id);

        if caller != schedule.from_addr {
            panic_with_error!(env, SchedulerError::Unauthorized);
        }
        if schedule.status != ScheduleStatus::Active && schedule.status != ScheduleStatus::Paused {
            panic_with_error!(env, SchedulerError::ScheduleNotActive);
        }
        if amount <= 0 {
            panic_with_error!(env, SchedulerError::InvalidAmount);
        }

        schedule.amount = amount;
        schedule.interval_ledgers = interval_ledgers;
        schedule.max_executions = max_executions;
        save_schedule(&env, &schedule);

        env.events().publish(
            (symbol_short!("scheduler"), symbol_short!("modified")),
            (schedule_id, caller),
        );
    }

    pub fn mark_executed(env: Env, schedule_id: u64, caller: Address) {
        caller.require_auth();

        let mut schedule = load_schedule(&env, schedule_id);

        if caller != schedule.from_addr {
            panic_with_error!(env, SchedulerError::Unauthorized);
        }
        if schedule.status != ScheduleStatus::Active {
            panic_with_error!(env, SchedulerError::ScheduleNotActive);
        }

        schedule.executions_so_far += 1;

        if schedule.max_executions > 0 && schedule.executions_so_far >= schedule.max_executions {
            schedule.status = ScheduleStatus::Completed;
        } else {
            schedule.next_execution_ledger = env.ledger().sequence() + schedule.interval_ledgers;
        }

        save_schedule(&env, &schedule);

        env.events().publish(
            (symbol_short!("scheduler"), symbol_short!("executed")),
            (schedule_id, schedule.executions_so_far),
        );
    }

    pub fn get_schedule(env: Env, schedule_id: u64) -> Schedule {
        load_schedule(&env, schedule_id)
    }

    pub fn get_owner_schedule(env: Env, owner: Address, index: u32) -> u64 {
        env.storage()
            .persistent()
            .get(&DataKey::OwnerSchedule(owner, index))
            .unwrap_or_else(|| panic_with_error!(env, SchedulerError::ScheduleNotFound))
    }

    pub fn get_owner_schedule_count(env: Env, owner: Address) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::OwnerScheduleCount(owner))
            .unwrap_or(0)
    }

    pub fn transfer_admin(env: Env, new_admin: Address) {
        require_admin(&env);
        env.storage().instance().set(&KEY_ADMIN, &new_admin);
        env.events().publish(
            (symbol_short!("scheduler"), symbol_short!("adm_xfer")),
            new_admin,
        );
    }
}

mod test;
