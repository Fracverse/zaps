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

struct Setup {
    env: Env,
    client: PaymentSchedulerClient<'static>,
    contract: Address,
    admin: Address,
    payer: Address,
    recipient: Address,
    token: Address,
}

impl Setup {
    fn new() -> Self {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_000);

        let admin = Address::generate(&env);
        let payer = Address::generate(&env);
        let recipient = Address::generate(&env);
        let token = env
            .register_stellar_asset_contract_v2(admin.clone())
            .address();
        StellarAssetClient::new(&env, &token).mint(&payer, &1_000_000);

        let contract = env.register_contract(None, PaymentScheduler);
        let client = PaymentSchedulerClient::new(&env, &contract);
        client.initialize(&admin);
        let client: PaymentSchedulerClient<'static> = unsafe { core::mem::transmute(client) };

        Self {
            env,
            client,
            contract,
            admin,
            payer,
            recipient,
            token,
        }
    }

    fn one_time(&self, offset: u64, amount: i128) -> u64 {
        self.client.schedule(
            &self.payer,
            &self.recipient,
            &self.token,
            &amount,
            &(self.env.ledger().timestamp() + offset),
            &0,
            &ScheduleKind::OneTime,
            &1,
        )
    }

    fn recurring(&self, offset: u64, interval: u64, amount: i128, max: u32) -> u64 {
        self.client.schedule(
            &self.payer,
            &self.recipient,
            &self.token,
            &amount,
            &(self.env.ledger().timestamp() + offset),
            &interval,
            &ScheduleKind::Recurring,
            &max,
        )
    }

    fn token_client(&self) -> TokenClient<'_> {
        TokenClient::new(&self.env, &self.token)
    }
}

#[test]
fn double_initialize_rejected() {
    let s = Setup::new();
    assert_eq!(
        s.client.try_initialize(&s.admin),
        Err(Ok(sdk_err(Error::AlreadyInitialized)))
    );
}

#[test]
fn one_time_schedule_escrows_funds() {
    let s = Setup::new();
    let id = s.one_time(60, 250);
    let schedule = s.client.get_schedule(&id);

    assert_eq!(schedule.payer, s.payer);
    assert_eq!(schedule.recipient, s.recipient);
    assert_eq!(schedule.amount, 250);
    assert_eq!(schedule.execute_after, 1_060);
    assert_eq!(schedule.kind, ScheduleKind::OneTime);
    assert_eq!(schedule.status, ScheduleStatus::Pending);
    assert_eq!(schedule.max_executions, 1);
    assert_eq!(s.token_client().balance(&s.contract), 250);
}

#[test]
fn recurring_schedule_escrows_all_occurrences() {
    let s = Setup::new();
    let id = s.recurring(60, 30, 100, 4);
    let schedule = s.client.get_schedule(&id);

    assert_eq!(schedule.kind, ScheduleKind::Recurring);
    assert_eq!(schedule.interval_seconds, 30);
    assert_eq!(schedule.max_executions, 4);
    assert_eq!(s.token_client().balance(&s.contract), 400);
}

#[test]
fn schedule_validation_rejects_invalid_inputs() {
    let s = Setup::new();

    assert_eq!(
        s.client.try_schedule(
            &s.payer,
            &s.recipient,
            &s.token,
            &0,
            &1_100,
            &0,
            &ScheduleKind::OneTime,
            &1,
        ),
        Err(Ok(sdk_err(Error::InvalidAmount)))
    );

    assert_eq!(
        s.client.try_schedule(
            &s.payer,
            &s.recipient,
            &s.token,
            &100,
            &999,
            &0,
            &ScheduleKind::OneTime,
            &1,
        ),
        Err(Ok(sdk_err(Error::InvalidExecuteAfter)))
    );

    assert_eq!(
        s.client.try_schedule(
            &s.payer,
            &s.recipient,
            &s.token,
            &100,
            &1_100,
            &0,
            &ScheduleKind::Recurring,
            &2,
        ),
        Err(Ok(sdk_err(Error::InvalidInterval)))
    );

    assert_eq!(
        s.client.try_schedule(
            &s.payer,
            &s.recipient,
            &s.token,
            &100,
            &1_100,
            &0,
            &ScheduleKind::OneTime,
            &2,
        ),
        Err(Ok(sdk_err(Error::InvalidExecutionLimit)))
    );
}

#[test]
fn execute_one_time_pays_recipient_and_completes() {
    let s = Setup::new();
    let id = s.one_time(60, 250);

    s.env.ledger().set_timestamp(1_060);
    s.client.execute(&id);

    let schedule = s.client.get_schedule(&id);
    assert_eq!(schedule.status, ScheduleStatus::Executed);
    assert_eq!(schedule.executions, 1);
    assert_eq!(s.token_client().balance(&s.recipient), 250);
    assert_eq!(s.token_client().balance(&s.contract), 0);
}

#[test]
fn execute_before_due_rejected() {
    let s = Setup::new();
    let id = s.one_time(60, 100);

    assert_eq!(s.client.try_execute(&id), Err(Ok(sdk_err(Error::NotDue))));
}

#[test]
fn recurring_execution_advances_until_limit() {
    let s = Setup::new();
    let id = s.recurring(10, 30, 100, 3);

    for expected in 1..=3 {
        s.env
            .ledger()
            .set_timestamp(s.env.ledger().timestamp() + 30);
        s.client.execute(&id);
        assert_eq!(s.client.get_schedule(&id).executions, expected);
    }

    let schedule = s.client.get_schedule(&id);
    assert_eq!(schedule.status, ScheduleStatus::Executed);
    assert_eq!(s.token_client().balance(&s.recipient), 300);
    assert_eq!(s.token_client().balance(&s.contract), 0);
}

#[test]
fn cancel_by_payer_refunds_remaining_escrow() {
    let s = Setup::new();
    let id = s.recurring(10, 30, 100, 3);

    s.env.ledger().set_timestamp(1_010);
    s.client.execute(&id);
    s.client.cancel(&s.payer, &id);

    let schedule = s.client.get_schedule(&id);
    assert_eq!(schedule.status, ScheduleStatus::Cancelled);
    assert_eq!(s.token_client().balance(&s.recipient), 100);
    assert_eq!(s.token_client().balance(&s.contract), 0);
    assert_eq!(s.token_client().balance(&s.payer), 999_900);
}

#[test]
fn cancel_by_admin_allowed_but_other_caller_rejected() {
    let s = Setup::new();
    let id = s.one_time(60, 100);
    let other = Address::generate(&s.env);

    assert_eq!(
        s.client.try_cancel(&other, &id),
        Err(Ok(sdk_err(Error::Unauthorized)))
    );

    s.client.cancel(&s.admin, &id);
    assert_eq!(s.client.get_schedule(&id).status, ScheduleStatus::Cancelled);
}

#[test]
fn modify_amount_tops_up_and_refunds() {
    let s = Setup::new();
    let id = s.recurring(60, 30, 100, 3);

    s.client.modify(&s.payer, &id, &None, &150, &0, &0, &0);
    assert_eq!(s.client.get_schedule(&id).amount, 150);
    assert_eq!(s.token_client().balance(&s.contract), 450);

    s.client.modify(&s.payer, &id, &None, &50, &0, &0, &0);
    assert_eq!(s.client.get_schedule(&id).amount, 50);
    assert_eq!(s.token_client().balance(&s.contract), 150);
}

#[test]
fn modify_recipient_time_interval_and_limit() {
    let s = Setup::new();
    let id = s.recurring(60, 30, 100, 3);
    let new_recipient = Address::generate(&s.env);

    s.client.modify(
        &s.payer,
        &id,
        &Some(new_recipient.clone()),
        &0,
        &1_200,
        &45,
        &2,
    );

    let schedule = s.client.get_schedule(&id);
    assert_eq!(schedule.recipient, new_recipient);
    assert_eq!(schedule.execute_after, 1_200);
    assert_eq!(schedule.interval_seconds, 45);
    assert_eq!(schedule.max_executions, 2);
    assert_eq!(s.token_client().balance(&s.contract), 200);
}

#[test]
fn unauthorized_or_completed_modify_rejected() {
    let s = Setup::new();
    let id = s.one_time(10, 100);
    let other = Address::generate(&s.env);

    assert_eq!(
        s.client.try_modify(&other, &id, &None, &200, &0, &0, &0),
        Err(Ok(sdk_err(Error::Unauthorized)))
    );

    s.env.ledger().set_timestamp(1_010);
    s.client.execute(&id);
    assert_eq!(
        s.client.try_modify(&s.payer, &id, &None, &200, &0, &0, &0),
        Err(Ok(sdk_err(Error::NotPending)))
    );
}

#[test]
fn pause_blocks_mutating_user_flows() {
    let s = Setup::new();
    let id = s.one_time(10, 100);

    s.client.pause();
    assert!(s.client.is_paused());

    assert_eq!(
        s.client.try_execute(&id),
        Err(Ok(sdk_err(Error::ContractPaused)))
    );
    assert_eq!(
        s.client.try_cancel(&s.payer, &id),
        Err(Ok(sdk_err(Error::ContractPaused)))
    );

    s.client.unpause();
    assert!(!s.client.is_paused());
}

#[test]
fn transfer_admin_updates_admin() {
    let s = Setup::new();
    let new_admin = Address::generate(&s.env);
    s.client.transfer_admin(&new_admin);
    assert_eq!(s.client.get_admin(), new_admin);
}
