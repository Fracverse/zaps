#![cfg(test)]

use super::*;
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events},
    token::StellarAssetClient,
    Address, Env, Symbol, TryFromVal, Vec,
};

fn setup_env() -> (Env, Address, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let payer1 = Address::generate(&env);
    let payer2 = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    StellarAssetClient::new(&env, &token).mint(&payer1, &10_000);
    StellarAssetClient::new(&env, &token).mint(&payer2, &1_000);
    (env, admin, payer1, payer2, token)
}

#[test]
fn test_batch_payments_success_and_event_emission() {
    let (env, _admin, payer1, _payer2, token) = setup_env();
    let recipient1 = Address::generate(&env);
    let recipient2 = Address::generate(&env);
    let contract_id = env.register_contract(None, PaymentBatchContract);
    let client = PaymentBatchContractClient::new(&env, &contract_id);

    let mut requests = Vec::new(&env);
    requests.push_back(BatchPaymentRequest {
        recipient: recipient1.clone(),
        asset: token.clone(),
        amount: 500,
    });
    requests.push_back(BatchPaymentRequest {
        recipient: recipient2.clone(),
        asset: token.clone(),
        amount: 250,
    });

    let results = client.batch_payments(&payer1, &requests);
    assert_eq!(results.len(), 2);
    assert!(results.get(0).unwrap().success);
    assert!(results.get(1).unwrap().success);

    let events = env.events().all();
    let batch_events = events
        .iter()
        .filter(|(_, topics, _)| {
            let t0 = <Symbol as TryFromVal<Env, _>>::try_from_val(&env, &topics.get(0).unwrap());
            matches!(t0, Ok(s) if s == symbol_short!("batchpay"))
        })
        .count();
    assert_eq!(batch_events, 2);
}

#[test]
fn test_batch_payment_partial_failure() {
    let (env, _admin, payer1, _payer2, token) = setup_env();
    let recipient1 = Address::generate(&env);
    let recipient2 = Address::generate(&env);
    let contract_id = env.register_contract(None, PaymentBatchContract);
    let client = PaymentBatchContractClient::new(&env, &contract_id);

    let mut requests = Vec::new(&env);
    requests.push_back(BatchPaymentRequest {
        recipient: recipient1.clone(),
        asset: token.clone(),
        amount: 500,
    });
    requests.push_back(BatchPaymentRequest {
        recipient: recipient2.clone(),
        asset: token.clone(),
        amount: 20_000,
    });

    let results = client.batch_payments(&payer1, &requests);
    assert_eq!(results.len(), 2);
    assert!(results.get(0).unwrap().success);
    assert!(!results.get(1).unwrap().success);
    assert_eq!(
        results.get(1).unwrap().error_code,
        Some(BatchPaymentError::TokenTransferFailed as u32)
    );
}

#[test]
#[should_panic]
fn test_batch_size_limit() {
    let (env, _admin, payer1, _payer2, token) = setup_env();
    let recipient = Address::generate(&env);
    let contract_id = env.register_contract(None, PaymentBatchContract);
    let client = PaymentBatchContractClient::new(&env, &contract_id);

    let mut requests = Vec::new(&env);
    for _ in 0..(MAX_BATCH_SIZE + 1) {
        requests.push_back(BatchPaymentRequest {
            recipient: recipient.clone(),
            asset: token.clone(),
            amount: 1,
        });
    }

    let _ = client.batch_payments(&payer1, &requests);
}
