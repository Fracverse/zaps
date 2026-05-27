#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::Address as _,
    Address, Bytes, Env,
};

fn setup() -> (Env, PaymentHistoryClient<'static>, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let payer = Address::generate(&env);
    let payee = Address::generate(&env);

    let contract_id = env.register_contract(None, PaymentHistory);
    let client = PaymentHistoryClient::new(&env, &contract_id);
    client.initialize(&admin, &10_000, &100_000);

    let client: PaymentHistoryClient<'static> = unsafe { core::mem::transmute(client) };

    (env, client, payer, payee)
}

#[test]
fn test_record_payment() {
    let (env, client, payer, payee) = setup();
    let token = Address::generate(&env);

    let record_id = client.record_payment(
        &payer, &payer, &payee, &token, &1000,
        &Bytes::new(&env), &None,
    );

    let record = client.get_record(&record_id);
    assert_eq!(record.amount, 1000);
    assert_eq!(record.payer, payer);
    assert_eq!(record.payee, payee);
}

#[test]
fn test_get_payment_count() {
    let (env, client, payer, payee) = setup();
    let token = Address::generate(&env);

    client.record_payment(&payer, &payer, &payee, &token, &500, &Bytes::new(&env), &None);
    client.record_payment(&payer, &payer, &payee, &token, &300, &Bytes::new(&env), &None);

    assert_eq!(client.get_payment_count(), 2);
}

#[test]
fn test_query() {
    let (env, client, payer, payee) = setup();
    let token = Address::generate(&env);

    client.record_payment(&payer, &payer, &payee, &token, &1000, &Bytes::new(&env), &None);

    let filter = PaymentFilter {
        payer: Some(payer.clone()),
        payee: None,
        merchant_id: None,
        token: None,
        min_amount: None,
        max_amount: None,
        from_ledger: None,
        to_ledger: None,
    };

    let result = client.query(&filter, &0, &10);
    assert_eq!(result.records.len(), 1);
    assert_eq!(result.total, 1);
}

#[test]
fn test_transfer_admin() {
    let (_, client, _, _) = setup();
    let new_admin = Address::generate(&client.env);
    client.transfer_admin(&new_admin);
}
