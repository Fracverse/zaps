#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token::{Client as TokenClient, StellarAssetClient},
    Address, Env, Error as SdkError,
};

fn sdk_err(e: Error) -> SdkError {
    SdkError::from_contract_error(e as u32)
}

fn create_token(env: &Env, admin: &Address) -> Address {
    env.register_stellar_asset_contract_v2(admin.clone())
        .address()
}

fn mint(env: &Env, token: &Address, to: &Address, amount: i128) {
    StellarAssetClient::new(env, token).mint(to, &amount);
}

struct Setup {
    env: Env,
    client: PaymentSchedulerClient<'static>,
    admin: Address,
    payer: Address,
    recipient: Address,
    token: Address,
}

impl Setup {
    fn new() -> Self {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let payer = Address::generate(&env);
        let recipient = Address::generate(&env);
        let token = create_token(&env, &admin);
        mint(&env, &token, &payer, 1_000_000);

        let contract_id = env.register_contract(None, PaymentScheduler);
        let client = PaymentSchedulerClient::new(&env, &contract_id);
        client.initialize(&admin);

        let client: PaymentSchedulerClient<'static> =
            unsafe { core::mem::transmute(client) };

        Setup { env, client, admin, payer, recipient, token }
    }

    /// Schedule a one-time payment due at current ledger + `offset`.
    fn schedule_one_time(&self, offset: u32) -> u64 {
        let due = self.env.ledger().sequence() + offset;
        self.client.schedule(
            &self.payer,
            &self.recipient,
            &self.token,
            &100i128,
            &due,
            &0u32,
            &ScheduleKind::OneTime,
        )
    }

    /// Schedule a recurring payment due at current ledger + `offset` with `interval`.
    fn schedule_recurring(&self, offset: u32, interval: u32) -> u64 {
        let due = self.env.ledger().sequence() + offset;
        self.client.schedule(
            &self.payer,
            &self.recipient,
            &self.token,
            &100i128,
            &due,
            &interval,
            &ScheduleKind::Recurring,
        )
    }
}

// ---------------------------------------------------------------------------
// Initialisation
// ---------------------------------------------------------------------------

#[test]
fn test_double_init_rejected() {
    let s = Setup::new();
    assert_eq!(
        s.client.try_initialize(&s.admin),
        Err(Ok(sdk_err(Error::AlreadyInitialized)))
    );
}

// ---------------------------------------------------------------------------
// Schedule creation
// ---------------------------------------------------------------------------

#[test]
fn test_schedule_stores_correctly() {
    let s = Setup::new();
    let id = s.schedule_one_time(100);
    let sched = s.client.get_schedule(&id);
    assert_eq!(sched.payer, s.payer);
    assert_eq!(sched.recipient, s.recipient);
    assert_eq!(sched.amount, 100);
    assert_eq!(sched.kind, ScheduleKind::OneTime);
    assert_eq!(sched.status, ScheduleStatus::Pending);
    assert_eq!(sched.executions, 0);
}

#[test]
fn test_schedule_invalid_amount_rejected() {
    let s = Setup::new();
    let due = s.env.ledger().sequence() + 100;
    assert_eq!(
        s.client.try_schedule(
            &s.payer, &s.recipient, &s.token,
            &0i128, &due, &0u32, &ScheduleKind::OneTime
        ),
        Err(Ok(sdk_err(Error::InvalidAmount)))
    );
}

#[test]
fn test_schedule_past_execute_at_rejected() {
    let s = Setup::new();
    s.env.ledger().with_mut(|l| l.sequence_number = 100);
    assert_eq!(
        s.client.try_schedule(
            &s.payer, &s.recipient, &s.token,
            &100i128, &50u32, &0u32, &ScheduleKind::OneTime
        ),
        Err(Ok(sdk_err(Error::InvalidExecuteAt)))
    );
}

#[test]
fn test_recurring_zero_interval_rejected() {
    let s = Setup::new();
    let due = s.env.ledger().sequence() + 100;
    assert_eq!(
        s.client.try_schedule(
            &s.payer, &s.recipient, &s.token,
            &100i128, &due, &0u32, &ScheduleKind::Recurring
        ),
        Err(Ok(sdk_err(Error::InvalidInterval)))
    );
}

#[test]
fn test_schedule_counter_increments() {
    let s = Setup::new();
    assert_eq!(s.client.schedule_count(), 0);
    s.schedule_one_time(100);
    assert_eq!(s.client.schedule_count(), 1);
    s.schedule_one_time(200);
    assert_eq!(s.client.schedule_count(), 2);
}

// ---------------------------------------------------------------------------
// Execution
// ---------------------------------------------------------------------------

#[test]
fn test_execute_before_due_rejected() {
    let s = Setup::new();
    let id = s.schedule_one_time(100);
    assert_eq!(
        s.client.try_execute(&id),
        Err(Ok(sdk_err(Error::NotDue)))
    );
}

#[test]
fn test_execute_one_time_transfers_and_marks_executed() {
    let s = Setup::new();
    let id = s.schedule_one_time(100);
    s.env.ledger().with_mut(|l| l.sequence_number += 100);

    s.client.execute(&id);

    assert_eq!(TokenClient::new(&s.env, &s.token).balance(&s.recipient), 100);
    let sched = s.client.get_schedule(&id);
    assert_eq!(sched.status, ScheduleStatus::Executed);
    assert_eq!(sched.executions, 1);
}

#[test]
fn test_execute_one_time_twice_rejected() {
    let s = Setup::new();
    let id = s.schedule_one_time(100);
    s.env.ledger().with_mut(|l| l.sequence_number += 100);
    s.client.execute(&id);
    assert_eq!(
        s.client.try_execute(&id),
        Err(Ok(sdk_err(Error::NotPending)))
    );
}

#[test]
fn test_execute_recurring_advances_execute_at() {
    let s = Setup::new();
    let id = s.schedule_recurring(100, 500);
    s.env.ledger().with_mut(|l| l.sequence_number += 100);

    s.client.execute(&id);

    let sched = s.client.get_schedule(&id);
    assert_eq!(sched.status, ScheduleStatus::Pending);
    assert_eq!(sched.executions, 1);
    // next due = current ledger + interval
    let current = s.env.ledger().sequence();
    assert_eq!(sched.execute_at, current + 500);
}

#[test]
fn test_execute_recurring_multiple_times() {
    let s = Setup::new();
    let id = s.schedule_recurring(100, 500);

    for i in 1u32..=3 {
        s.env.ledger().with_mut(|l| l.sequence_number += 500);
        s.client.execute(&id);
        assert_eq!(s.client.get_schedule(&id).executions, i);
    }

    assert_eq!(
        TokenClient::new(&s.env, &s.token).balance(&s.recipient),
        300
    );
}

// ---------------------------------------------------------------------------
// Cancellation
// ---------------------------------------------------------------------------

#[test]
fn test_cancel_by_payer() {
    let s = Setup::new();
    let id = s.schedule_one_time(100);
    s.client.cancel(&s.payer, &id);
    assert_eq!(s.client.get_schedule(&id).status, ScheduleStatus::Cancelled);
}

#[test]
fn test_cancel_by_admin() {
    let s = Setup::new();
    let id = s.schedule_one_time(100);
    s.client.cancel(&s.admin, &id);
    assert_eq!(s.client.get_schedule(&id).status, ScheduleStatus::Cancelled);
}

#[test]
fn test_cancel_by_unauthorized_rejected() {
    let s = Setup::new();
    let id = s.schedule_one_time(100);
    let other = Address::generate(&s.env);
    assert_eq!(
        s.client.try_cancel(&other, &id),
        Err(Ok(sdk_err(Error::Unauthorized)))
    );
}

#[test]
fn test_cancel_already_cancelled_rejected() {
    let s = Setup::new();
    let id = s.schedule_one_time(100);
    s.client.cancel(&s.payer, &id);
    assert_eq!(
        s.client.try_cancel(&s.payer, &id),
        Err(Ok(sdk_err(Error::NotPending)))
    );
}

#[test]
fn test_execute_cancelled_schedule_rejected() {
    let s = Setup::new();
    let id = s.schedule_one_time(100);
    s.client.cancel(&s.payer, &id);
    s.env.ledger().with_mut(|l| l.sequence_number += 100);
    assert_eq!(
        s.client.try_execute(&id),
        Err(Ok(sdk_err(Error::NotPending)))
    );
}

// ---------------------------------------------------------------------------
// Modification
// ---------------------------------------------------------------------------

#[test]
fn test_modify_amount() {
    let s = Setup::new();
    let id = s.schedule_one_time(100);
    s.client.modify(&s.payer, &id, &200i128, &0u32);
    assert_eq!(s.client.get_schedule(&id).amount, 200);
}

#[test]
fn test_modify_interval_recurring() {
    let s = Setup::new();
    let id = s.schedule_recurring(100, 500);
    s.client.modify(&s.payer, &id, &0i128, &1000u32);
    assert_eq!(s.client.get_schedule(&id).interval_ledgers, 1000);
}

#[test]
fn test_modify_interval_on_one_time_rejected() {
    let s = Setup::new();
    let id = s.schedule_one_time(100);
    assert_eq!(
        s.client.try_modify(&s.payer, &id, &0i128, &500u32),
        Err(Ok(sdk_err(Error::InvalidInterval)))
    );
}

#[test]
fn test_modify_by_unauthorized_rejected() {
    let s = Setup::new();
    let id = s.schedule_one_time(100);
    let other = Address::generate(&s.env);
    assert_eq!(
        s.client.try_modify(&other, &id, &200i128, &0u32),
        Err(Ok(sdk_err(Error::Unauthorized)))
    );
}

#[test]
fn test_modify_executed_schedule_rejected() {
    let s = Setup::new();
    let id = s.schedule_one_time(100);
    s.env.ledger().with_mut(|l| l.sequence_number += 100);
    s.client.execute(&id);
    assert_eq!(
        s.client.try_modify(&s.payer, &id, &200i128, &0u32),
        Err(Ok(sdk_err(Error::NotPending)))
    );
}

// ---------------------------------------------------------------------------
// Pause / unpause
// ---------------------------------------------------------------------------

#[test]
fn test_pause_blocks_schedule_and_execute() {
    let s = Setup::new();
    let id = s.schedule_one_time(100);

    s.client.pause();
    assert!(s.client.is_paused());

    let due = s.env.ledger().sequence() + 100;
    assert_eq!(
        s.client.try_schedule(
            &s.payer, &s.recipient, &s.token,
            &100i128, &due, &0u32, &ScheduleKind::OneTime
        ),
        Err(Ok(sdk_err(Error::ContractPaused)))
    );

    s.env.ledger().with_mut(|l| l.sequence_number += 100);
    assert_eq!(
        s.client.try_execute(&id),
        Err(Ok(sdk_err(Error::ContractPaused)))
    );

    s.client.unpause();
    assert!(!s.client.is_paused());
    s.client.execute(&id); // should succeed now
}

// ---------------------------------------------------------------------------
// Admin transfer
// ---------------------------------------------------------------------------

#[test]
fn test_transfer_admin() {
    let s = Setup::new();
    let new_admin = Address::generate(&s.env);
    s.client.transfer_admin(&new_admin);
    assert_eq!(s.client.get_admin(), new_admin);
}
