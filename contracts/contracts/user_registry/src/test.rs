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

/// Verify that successful DID registration is reflected in the ledger snapshot.
#[test]
fn test_register_privy_did_snapshot() {
    let (env, client, signing_key) = setup();
    let wallet = Address::generate(&env);
    let did = String::from_str(&env, "did:privy:snapshot");
    let signature = sign_did_link(&env, &signing_key, &did, &wallet);

    client.register_privy_did(&did, &wallet, &signature);

    let snapshot = env.snapshot();
    let snapshot_str = snapshot.to_string();

    // The persistent storage snapshot should include the registered DID.
    assert!(snapshot_str.contains("did:privy:snapshot"));
    // The mapping should be retrievable after the snapshot.
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

// ── Issue #776: 2-step admin ownership transfer ─────────────────────────────

/// Build a freshly initialized contract along with the old (current) admin and
/// a distinct proposed successor admin.
fn admin_setup() -> (Env, UserRegistryContractClient<'static>, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, UserRegistryContract);
    let client = UserRegistryContractClient::new(&env, &contract_id);

    let old_admin = Address::generate(&env);
    client.initialize(&old_admin);

    let new_admin = Address::generate(&env);

    (env, client, old_admin, new_admin)
}

/// Read the currently stored contract admin directly from persistent storage.
fn stored_admin(env: &Env, client: &UserRegistryContractClient<'static>) -> Address {
    env.as_contract(&client.address, || {
        env.storage()
            .persistent()
            .get::<_, Address>(&DataKey::Admin)
            .unwrap()
    })
}

/// Proposing a successor must NOT move ownership: the old admin keeps the
/// stored `DataKey::Admin` and still holds admin privileges until claim.
#[test]
fn test_2step_transfer_old_admin_keeps_ownership_until_claim() {
    let (env, client, old_admin, new_admin) = admin_setup();

    client.propose_admin(&old_admin, &new_admin);

    // Ownership has not moved yet.
    assert_eq!(stored_admin(&env, &client), old_admin);

    // The old admin still holds privileges: an admin-only call succeeds.
    client.set_privy_verifier(&old_admin, &BytesN::from_array(&env, &[7u8; 32]));
}

/// Claiming ownership moves `DataKey::Admin` to the proposed successor, who
/// then holds admin privileges.
#[test]
fn test_2step_transfer_claim_admin_moves_ownership() {
    let (env, client, old_admin, new_admin) = admin_setup();

    client.propose_admin(&old_admin, &new_admin);
    let claimed = client.try_claim_admin(&new_admin);
    assert!(claimed.is_ok(), "proposed admin must be able to claim: {claimed:?}");

    // Ownership moved to the successor.
    assert_eq!(stored_admin(&env, &client), new_admin);

    // The new admin now holds privileges: an admin-only call succeeds.
    client.set_privy_verifier(&new_admin, &BytesN::from_array(&env, &[9u8; 32]));
}

/// Only the proposed successor may claim; any other caller is rejected.
///
/// Ignored on Soroban v20 plus this toolchain because contract panics are
/// non-unwinding (they abort the test process instead of returning an error),
/// the same reason the repo ignores its other panic/rejection tests. Run with
/// `cargo test -- --ignored` once a panicking SDK/testutils is available.
#[test]
#[ignore]
#[should_panic(expected = "only the proposed admin can claim")]
fn test_claim_admin_rejects_unauthorized_caller() {
    let (env, client, old_admin, new_admin) = admin_setup();
    let evil = Address::generate(&env);

    client.propose_admin(&old_admin, &new_admin);
    // Neither an unrelated address nor the old admin can claim.
    client.claim_admin(&evil);
}

/// Only the current admin may propose a successor.
#[test]
#[ignore]
#[should_panic(expected = "only admin can propose new admin")]
fn test_propose_admin_rejects_non_admin() {
    let (env, client, _old_admin, _new_admin) = admin_setup();
    let imposter = Address::generate(&env);
    let target = Address::generate(&env);

    client.propose_admin(&imposter, &target);
}

/// Claiming with no active proposal panics.
#[test]
#[ignore]
#[should_panic(expected = "no pending admin proposal")]
fn test_claim_admin_without_proposal_fails() {
    let (_env, client, _old_admin, new_admin) = admin_setup();
    client.claim_admin(&new_admin);
}
