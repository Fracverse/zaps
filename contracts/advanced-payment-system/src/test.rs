#![cfg(test)]

use crate::*;
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{Address, Env};

#[test]
fn test_initialize() {
    let env = Env::default();
    let admin = Address::random(&env);
    let token = Address::random(&env);
    
    let config = PaymentConfig {
        escrow_enabled: true,
        splitting_enabled: true,
        governance_enabled: true,
        dispute_timeout_days: 7,
        min_arbitrator_reputation: 50,
    };

    AdvancedPaymentSystem::initialize(env.clone(), admin.clone(), token.clone(), config.clone());
    
    let retrieved_admin = AdvancedPaymentSystem::get_admin(env.clone());
    assert_eq!(retrieved_admin, admin);
    
    let retrieved_config = AdvancedPaymentSystem::get_config(env);
    assert_eq!(retrieved_config.escrow_enabled, true);
}

#[test]
fn test_create_escrow() {
    let env = Env::default();
    let admin = Address::random(&env);
    let token = Address::random(&env);
    let buyer = Address::random(&env);
    let seller = Address::random(&env);
    
    let config = PaymentConfig {
        escrow_enabled: true,
        splitting_enabled: true,
        governance_enabled: true,
        dispute_timeout_days: 7,
        min_arbitrator_reputation: 50,
    };

    AdvancedPaymentSystem::initialize(env.clone(), admin, token, config);
    
    let escrow_id = soroban_sdk::BytesN::from_array(&env, &[1u8; 32]);
    
    // Note: In real tests, you'd need to mock token transfers
    // This is a simplified test structure
}

#[test]
fn test_proposal_creation() {
    let env = Env::default();
    let admin = Address::random(&env);
    let token = Address::random(&env);
    let proposer = Address::random(&env);
    
    let config = PaymentConfig {
        escrow_enabled: true,
        splitting_enabled: true,
        governance_enabled: true,
        dispute_timeout_days: 7,
        min_arbitrator_reputation: 50,
    };

    AdvancedPaymentSystem::initialize(env.clone(), admin, token, config);
    
    let payload = soroban_sdk::Bytes::new(&env);
    let description = soroban_sdk::Bytes::new(&env);
    
    let proposal_id = AdvancedPaymentSystem::create_proposal(
        env.clone(),
        proposer,
        payload,
        description,
    );
    
    assert_eq!(proposal_id, 1);
}
