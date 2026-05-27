#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::Address as _,
    token, Address, Bytes, Env,
};

fn setup() -> (Env, PaymentSchedulerClient<'static>, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let from = Address::generate(&env);
    let to = Address::generate(&env);

    let sac = env.register_stellar_asset_contract_v2(admin.clone());
    let token = sac.address();
    let sac_client = token::StellarAssetClient::new(&env, &token);
    sac_client.mint(&from, &1_000_000);

    let contract_id = env.register_contract(None, PaymentScheduler);
    let client = PaymentSchedulerClient::new(&env, &contract_id);
    client.initialize(&admin);

    let client: PaymentSchedulerClient<'static> = unsafe { core::mem::transmute(client) };

    (env, client, admin, from, to)
}

#[test]
fn test_create_schedule() {
    let (env, client, _, from, to) = setup();
    let token = Address::generate(&env);

    let schedule_id = client.create_schedule(
        &from, &to, &token, &1000,
        &ScheduleType::OneTime,
        &100, &0, &None, &1,
        &Bytes::new(&env),
    );

    let schedule = client.get_schedule(&schedule_id);
    assert_eq!(schedule.amount, 1000);
    assert_eq!(schedule.status, ScheduleStatus::Active);
}

#[test]
fn test_cancel_schedule() {
    let (env, client, _, from, to) = setup();
    let token = Address::generate(&env);

    let schedule_id = client.create_schedule(
        &from, &to, &token, &500,
        &ScheduleType::OneTime,
        &100, &0, &None, &1,
        &Bytes::new(&env),
    );

    client.cancel_schedule(&schedule_id, &from);
    let schedule = client.get_schedule(&schedule_id);
    assert_eq!(schedule.status, ScheduleStatus::Cancelled);
}

#[test]
fn test_pause_resume_schedule() {
    let (env, client, _, from, to) = setup();
    let token = Address::generate(&env);

    let schedule_id = client.create_schedule(
        &from, &to, &token, &500,
        &ScheduleType::Recurring,
        &100, &1000, &Some(RecurringInterval::Daily), &10,
        &Bytes::new(&env),
    );

    client.pause_schedule(&schedule_id, &from);
    assert_eq!(client.get_schedule(&schedule_id).status, ScheduleStatus::Paused);

    client.resume_schedule(&schedule_id, &from);
    assert_eq!(client.get_schedule(&schedule_id).status, ScheduleStatus::Active);
}

#[test]
fn test_modify_schedule() {
    let (env, client, _, from, to) = setup();
    let token = Address::generate(&env);

    let schedule_id = client.create_schedule(
        &from, &to, &token, &500,
        &ScheduleType::Recurring,
        &100, &1000, &None, &10,
        &Bytes::new(&env),
    );

    client.modify_schedule(&schedule_id, &from, &2000, &500, &5);
    let schedule = client.get_schedule(&schedule_id);
    assert_eq!(schedule.amount, 2000);
    assert_eq!(schedule.max_executions, 5);
}

#[test]
fn test_mark_executed() {
    let (env, client, _, from, to) = setup();
    let token = Address::generate(&env);

    let schedule_id = client.create_schedule(
        &from, &to, &token, &500,
        &ScheduleType::OneTime,
        &100, &0, &None, &1,
        &Bytes::new(&env),
    );

    client.mark_executed(&schedule_id, &from);
    let schedule = client.get_schedule(&schedule_id);
    assert_eq!(schedule.executions_so_far, 1);
    assert_eq!(schedule.status, ScheduleStatus::Completed);
}

#[test]
fn test_transfer_admin() {
    let (_, client, _, _, _) = setup();
    let new_admin = Address::generate(&client.env);
    client.transfer_admin(&new_admin);
}
