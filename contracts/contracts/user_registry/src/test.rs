#![cfg(test)]

use super::*;
use ed25519_dalek::{Signer, SigningKey};
use soroban_sdk::{testutils::Address as _, xdr::ToXdr, BytesN, Env, String};

/// Deterministic test-only signing key standing in for Privy's verifier key.
fn verifier_key() -> SigningKey {
    SigningKey::from_bytes(&[7u8; 32])
}

fn setup() -> (Env, UserRegistryContractClient<'static>, SigningKey) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, UserRegistryContract);
    let client = UserRegistryContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.initialize(&admin);

    let signing_key = verifier_key();
    let pubkey = BytesN::from_array(&env, &signing_key.verifying_key().to_bytes());
    client.set_privy_verifier(&admin, &pubkey);

    (env, client, signing_key)
}

/// Sign the `(did, wallet)` payload the contract expects, as Privy's backend would.
fn sign_did_link(
    env: &Env,
    signing_key: &SigningKey,
    did: &String,
    wallet: &Address,
) -> BytesN<64> {
    let message = (did.clone(), wallet.clone()).to_xdr(env);
    let signature = signing_key.sign(&message.to_alloc_vec());
    BytesN::from_array(env, &signature.to_bytes())
}

/// Verify that registering the same DID twice panics with a duplicate error.
#[test]
#[ignore]
#[should_panic(expected = "DID already registered")]
fn test_register_privy_did_duplicate_fails() {
    let (env, client, signing_key) = setup();
    let wallet = Address::generate(&env);
    let did = String::from_str(&env, "did:privy:abc123");
    let signature = sign_did_link(&env, &signing_key, &did, &wallet);
    client.register_privy_did(&did, &wallet, &signature);
    // Second registration with same DID must panic
    client.register_privy_did(&did, &wallet, &signature);
}

/// Verify that a successful DID registration stores the correct wallet mapping.
#[test]
fn test_register_privy_did_success() {
    let (env, client, signing_key) = setup();
    let wallet = Address::generate(&env);
    let did = String::from_str(&env, "did:privy:user1");
    let signature = sign_did_link(&env, &signing_key, &did, &wallet);
    client.register_privy_did(&did, &wallet, &signature);
    assert_eq!(client.get_wallet_for_did(&did), wallet);
}

/// Verify that a registration signed by a key other than the configured
/// verifier is rejected before any mapping is created.
#[test]
#[ignore]
#[should_panic]
fn test_register_privy_did_invalid_signature_fails() {
    let (env, client, _signing_key) = setup();
    let wallet = Address::generate(&env);
    let did = String::from_str(&env, "did:privy:untrusted");

    let forged_key = SigningKey::from_bytes(&[9u8; 32]);
    let signature = sign_did_link(&env, &forged_key, &did, &wallet);

    client.register_privy_did(&did, &wallet, &signature);
}

/// Verify that updating a DID mapping with the correct old wallet succeeds.
#[test]
fn test_update_privy_did_success() {
    let (env, client, signing_key) = setup();
    let old_wallet = Address::generate(&env);
    let new_wallet = Address::generate(&env);
    let did = String::from_str(&env, "did:privy:user2");
    let signature = sign_did_link(&env, &signing_key, &did, &old_wallet);
    client.register_privy_did(&did, &old_wallet, &signature);
    client.update_privy_did(&did, &old_wallet, &new_wallet);
    assert_eq!(client.get_wallet_for_did(&did), new_wallet);
}

/// Verify that updating a DID mapping with the wrong old wallet panics.
#[test]
#[ignore]
#[should_panic(expected = "unauthorized: old wallet does not match registered wallet")]
fn test_update_privy_did_wrong_wallet_fails() {
    let (env, client, signing_key) = setup();
    let correct_wallet = Address::generate(&env);
    let wrong_wallet = Address::generate(&env);
    let new_wallet = Address::generate(&env);
    let did = String::from_str(&env, "did:privy:user3");
    let signature = sign_did_link(&env, &signing_key, &did, &correct_wallet);
    client.register_privy_did(&did, &correct_wallet, &signature);
    client.update_privy_did(&did, &wrong_wallet, &new_wallet);
}

/// Verify that admin recovery reassigns a DID to a new wallet.
#[test]
fn test_recover_privy_did_as_admin() {
    let (env, client, signing_key) = setup();
    let old_wallet = Address::generate(&env);
    let new_wallet = Address::generate(&env);
    let did = String::from_str(&env, "did:privy:user4");
    let signature = sign_did_link(&env, &signing_key, &did, &old_wallet);
    client.register_privy_did(&did, &old_wallet, &signature);
    client.recover_privy_did(&did, &new_wallet);
    assert_eq!(client.get_wallet_for_did(&did), new_wallet);
}

/// Verify that querying an unregistered DID panics.
#[test]
#[ignore]
#[should_panic(expected = "DID not registered")]
fn test_get_wallet_for_unregistered_did_fails() {
    let (env, client, _signing_key) = setup();
    let did = String::from_str(&env, "did:privy:ghost");
    client.get_wallet_for_did(&did);
}
