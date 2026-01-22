#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Events, Ledger},
    token, Address, Env, BytesN, Error,
};

#[test]
fn test_lock_funds_success() {
    let env = Env::default();
    let contract_id = env.register_contract(None, EscrowContract);
    let client = EscrowContractClient::new(&env, &contract_id);

    env.mock_all_auths();

    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let token = Address::generate(&env);
    let escrow_id = BytesN::from_array(&env, &[1u8; 32]);
    let amount: i128 = 1_000_000;
    let timeout_ledger: u32 = 1_000_000;
    let memo = BytesN::from_array(&env, &[0u8; 32]);

    let sac = token::StellarAssetClient::new(&env, &token);
    sac.mint(&buyer, &amount);

    client.lock_funds(&escrow_id, &buyer, &seller, &token, &amount, &timeout_ledger, &memo);

    let stored = client.get_escrow(&escrow_id);
    assert_eq!(stored.buyer, buyer);
    assert_eq!(stored.seller, seller);
    assert_eq!(stored.token, token);
    assert_eq!(stored.amount, amount);
    assert_eq!(stored.state, EscrowState::Locked);
    assert_eq!(stored.memo, memo);

    assert!(client.is_locked(&escrow_id));
}

#[test]
fn test_lock_funds_duplicate_id_fails() {
    let env = Env::default();
    let contract_id = env.register_contract(None, EscrowContract);
    let client = EscrowContractClient::new(&env, &contract_id);

    env.mock_all_auths();

    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let token = Address::generate(&env);
    let escrow_id = BytesN::from_array(&env, &[2u8; 32]);
    let amount: i128 = 500_000;

    let sac = token::StellarAssetClient::new(&env, &token);
    sac.mint(&buyer, &(amount * 2));

    client.lock_funds(&escrow_id, &buyer, &seller, &token, &amount, &1_000_000, &BytesN::from_array(&env, &[0u8; 32]));

    let result = client.try_lock_funds(&escrow_id, &buyer, &seller, &token, &amount, &1_000_000, &BytesN::from_array(&env, &[0u8; 32]));

    assert!(result.is_err());
    // Optional: more precise
    // assert!(matches!(result, Err(Error::Contract(_))));
}

#[test]
fn test_lock_funds_zero_amount_fails() {
    let env = Env::default();
    let contract_id = env.register_contract(None, EscrowContract);
    let client = EscrowContractClient::new(&env, &contract_id);

    env.mock_all_auths();

    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let token = Address::generate(&env);
    let escrow_id = BytesN::from_array(&env, &[3u8; 32]);

    let result = client.try_lock_funds(&escrow_id, &buyer, &seller, &token, &0, &1_000_000, &BytesN::from_array(&env, &[0u8; 32]));

    assert!(result.is_err());
}

#[test]
fn test_release_funds_by_seller_success() {
    let env = Env::default();
    let contract_id = env.register_contract(None, EscrowContract);
    let client = EscrowContractClient::new(&env, &contract_id);

    env.mock_all_auths();

    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let token = Address::generate(&env);
    let escrow_id = BytesN::from_array(&env, &[4u8; 32]);
    let amount: i128 = 750_000;

    let sac = token::StellarAssetClient::new(&env, &token);
    sac.mint(&buyer, &amount);

    client.lock_funds(&escrow_id, &buyer, &seller, &token, &amount, &1_000_000, &BytesN::from_array(&env, &[0u8; 32]));

    client.release_funds(&escrow_id, &seller);

    let stored = client.get_escrow(&escrow_id);
    assert_eq!(stored.state, EscrowState::Released);
    assert!(!client.is_locked(&escrow_id));
}

#[test]
fn test_release_funds_unauthorized_fails() {
    let env = Env::default();
    let contract_id = env.register_contract(None, EscrowContract);
    let client = EscrowContractClient::new(&env, &contract_id);

    env.mock_all_auths();

    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let random = Address::generate(&env);
    let token = Address::generate(&env);
    let escrow_id = BytesN::from_array(&env, &[5u8; 32]);
    let amount: i128 = 300_000;

    let sac = token::StellarAssetClient::new(&env, &token);
    sac.mint(&buyer, &amount);

    client.lock_funds(&escrow_id, &buyer, &seller, &token, &amount, &1_000_000, &BytesN::from_array(&env, &[0u8; 32]));

    let result = client.try_release_funds(&escrow_id, &random);
    assert!(result.is_err());
}

#[test]
fn test_refund_funds_by_buyer_after_timeout_success() {
    let env = Env::default();
    let contract_id = env.register_contract(None, EscrowContract);
    let client = EscrowContractClient::new(&env, &contract_id);

    env.mock_all_auths();

    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let token = Address::generate(&env);
    let escrow_id = BytesN::from_array(&env, &[6u8; 32]);
    let amount: i128 = 1_200_000;

    let sac = token::StellarAssetClient::new(&env, &token);
    sac.mint(&buyer, &amount);

    client.lock_funds(&escrow_id, &buyer, &seller, &token, &amount, &1_000_000, &BytesN::from_array(&env, &[0u8; 32]));

    let creation_time = env.ledger().timestamp();
    env.ledger().set_timestamp(creation_time + 8 * 24 * 60 * 60);

    client.refund_funds(&escrow_id, &buyer);

    let stored = client.get_escrow(&escrow_id);
    assert_eq!(stored.state, EscrowState::Refunded);
    assert!(!client.is_locked(&escrow_id));
}

#[test]
fn test_refund_before_timeout_only_by_buyer_or_arbitrator() {
    let env = Env::default();
    let contract_id = env.register_contract(None, EscrowContract);
    let client = EscrowContractClient::new(&env, &contract_id);

    env.mock_all_auths();

    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let random = Address::generate(&env);
    let token = Address::generate(&env);
    let escrow_id = BytesN::from_array(&env, &[7u8; 32]);
    let amount: i128 = 900_000;

    let sac = token::StellarAssetClient::new(&env, &token);
    sac.mint(&buyer, &amount);

    client.lock_funds(&escrow_id, &buyer, &seller, &token, &amount, &1_000_000, &BytesN::from_array(&env, &[0u8; 32]));

    let result = client.try_refund_funds(&escrow_id, &random);
    assert!(result.is_err());

    client.refund_funds(&escrow_id, &buyer);

    let stored = client.get_escrow(&escrow_id);
    assert_eq!(stored.state, EscrowState::Refunded);
}