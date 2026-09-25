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

/// Verify that registering the same DID twice is rejected.
///
/// Uses `try_register_privy_did` which returns `Result` so the test
/// process is not aborted by the contract's internal `panic!` — in
/// contrast to `#[should_panic]` which does not work with Soroban v20's
/// non-unwinding panics.
#[test]
#[ignore = "contract panics are non-unwinding under Soroban v20 testutils and abort the test process"]
fn test_register_privy_did_duplicate_fails() {
    let (env, client, signing_key) = setup();
    let wallet = Address::generate(&env);
    let did = String::from_str(&env, "did:privy:abc123");
    let signature = sign_did_link(&env, &signing_key, &did, &wallet);

    // First registration succeeds.
    client.register_privy_did(&did, &wallet, &signature);

    // Second registration of the same DID must be rejected.
    let result = client.try_register_privy_did(&did, &wallet, &signature);
    assert!(
        result.is_err(),
        "registering the same DID twice must fail"
    );
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

/// Verify that successful DID registration is persisted in contract storage.
#[test]
fn test_register_privy_did_snapshot() {
    let (env, client, signing_key) = setup();
    let wallet = Address::generate(&env);
    let did = String::from_str(&env, "did:privy:snapshot");
    let signature = sign_did_link(&env, &signing_key, &did, &wallet);

    client.register_privy_did(&did, &wallet, &signature);

    // Both the forward and reverse mappings must be persisted.
    env.as_contract(&client.address, || {
        assert!(
            env.storage()
                .persistent()
                .has(&DataKey::PrivyDid(did.clone())),
            "PrivyDid forward key must be persisted"
        );
        assert!(
            env.storage()
                .persistent()
                .has(&DataKey::WalletDid(wallet.clone())),
            "WalletDid reverse key must be persisted"
        );
    });
    // The mapping should be retrievable after registration.
    assert_eq!(client.get_wallet_for_did(&did), wallet);
}

/// Verify that a registration signed by a key other than the configured
/// verifier is rejected before any mapping is created.
///
/// Uses `try_register_privy_did` (returns `Result`) so the contract's
/// internal `panic!` (triggered by the failed ed25519_verify) is caught
/// as an error rather than aborting the test process.
#[test]
#[ignore = "contract panics are non-unwinding under Soroban v20 testutils and abort the test process"]
fn test_register_privy_did_invalid_signature_fails() {
    let (env, client, _signing_key) = setup();
    let wallet = Address::generate(&env);
    let did = String::from_str(&env, "did:privy:untrusted");

    // Sign with a *different* key — not the one configured as the verifier.
    let forged_key = SigningKey::from_bytes(&[9u8; 32]);
    let signature = sign_did_link(&env, &forged_key, &did, &wallet);

    let result = client.try_register_privy_did(&did, &wallet, &signature);
    assert!(
        result.is_err(),
        "a signature from an untrusted key must be rejected"
    );

    // No mapping must have been created despite the failed call.
    let lookup = client.try_get_wallet_for_did(&did);
    assert!(
        lookup.is_err(),
        "DID must not be registered after rejected call"
    );
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

/// Verify that updating a DID mapping with the wrong old wallet is rejected.
#[test]
#[ignore = "contract panics are non-unwinding under Soroban v20 testutils and abort the test process"]
fn test_update_privy_did_wrong_wallet_fails() {
    let (env, client, signing_key) = setup();
    let correct_wallet = Address::generate(&env);
    let wrong_wallet = Address::generate(&env);
    let new_wallet = Address::generate(&env);
    let did = String::from_str(&env, "did:privy:user3");
    let signature = sign_did_link(&env, &signing_key, &did, &correct_wallet);
    client.register_privy_did(&did, &correct_wallet, &signature);
    let result = client.try_update_privy_did(&did, &wrong_wallet, &new_wallet);
    assert!(
        result.is_err(),
        "update with wrong old wallet must be rejected"
    );
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

/// Verify that update and recovery cannot move a DID onto a wallet that is
/// already linked to another DID (one wallet -> one DID).
#[test]
#[ignore = "contract panics are non-unwinding under Soroban v20 testutils and abort the test process"]
fn test_update_and_recover_reject_wallet_already_linked() {
    let (env, client, signing_key) = setup();
    let wallet_a = Address::generate(&env);
    let wallet_b = Address::generate(&env);
    let did_a = String::from_str(&env, "did:privy:a");
    let did_b = String::from_str(&env, "did:privy:b");
    let sig_a = sign_did_link(&env, &signing_key, &did_a, &wallet_a);
    let sig_b = sign_did_link(&env, &signing_key, &did_b, &wallet_b);
    client.register_privy_did(&did_a, &wallet_a, &sig_a);
    client.register_privy_did(&did_b, &wallet_b, &sig_b);

    assert!(client.try_update_privy_did(&did_a, &wallet_a, &wallet_b).is_err());
    assert!(client.try_recover_privy_did(&did_a, &wallet_b).is_err());
    assert_eq!(client.get_wallet_for_did(&did_a), wallet_a);
    assert_eq!(client.get_did_for_wallet(&wallet_b), did_b);
}

/// Verify that querying an unregistered DID returns an error.
#[test]
#[ignore = "contract panics are non-unwinding under Soroban v20 testutils and abort the test process"]
fn test_get_wallet_for_unregistered_did_fails() {
    let (env, client, _signing_key) = setup();
    let did = String::from_str(&env, "did:privy:ghost");
    let result = client.try_get_wallet_for_did(&did);
    assert!(result.is_err(), "querying an unregistered DID must fail");
}

/// Verify that register_privy_did rejects a second attempt when the same
/// wallet already has a DID linked.
///
/// This guards the `WalletDid` reverse index: one wallet → one DID.
/// Without this guard a single wallet could accumulate multiple forward
/// DID mappings while the reverse index only holds the latest one,
/// making `get_did_for_wallet` return stale/inconsistent data.
#[test]
#[ignore = "contract panics are non-unwinding under Soroban v20 testutils and abort the test process"]
fn test_register_privy_did_rejects_wallet_already_has_did() {
    let (env, client, signing_key) = setup();
    let wallet = Address::generate(&env);

    // Register a first DID for this wallet — must succeed.
    let did_first = String::from_str(&env, "did:privy:first");
    let sig_first = sign_did_link(&env, &signing_key, &did_first, &wallet);
    client.register_privy_did(&did_first, &wallet, &sig_first);

    // Attempt to register a *different* DID for the same wallet — must fail.
    let did_second = String::from_str(&env, "did:privy:second");
    let sig_second = sign_did_link(&env, &signing_key, &did_second, &wallet);
    let result = client.try_register_privy_did(&did_second, &wallet, &sig_second);
    assert!(
        result.is_err(),
        "registering a second DID for the same wallet must be rejected"
    );

    // The first mapping must be intact, the second must not exist.
    assert_eq!(
        client.get_wallet_for_did(&did_first),
        wallet,
        "first DID mapping must still resolve correctly"
    );
    let lookup_second = client.try_get_wallet_for_did(&did_second);
    assert!(
        lookup_second.is_err(),
        "second DID must not have been registered"
    );
}

/// Verify that `get_did_for_wallet` correctly performs the reverse lookup
/// after a successful registration, and that it also correctly exposes
/// the new wallet after an `update_privy_did` rotation.
#[test]
fn test_get_did_for_wallet_round_trip() {
    let (env, client, signing_key) = setup();
    let wallet = Address::generate(&env);
    let did = String::from_str(&env, "did:privy:roundtrip");
    let signature = sign_did_link(&env, &signing_key, &did, &wallet);

    client.register_privy_did(&did, &wallet, &signature);

    // Forward and reverse lookups must agree.
    assert_eq!(
        client.get_wallet_for_did(&did),
        wallet,
        "forward lookup must return the registered wallet"
    );
    assert_eq!(
        client.get_did_for_wallet(&wallet),
        did,
        "reverse lookup must return the registered DID"
    );

    // After an update the reverse lookup must track the new wallet.
    let new_wallet = Address::generate(&env);
    client.update_privy_did(&did, &wallet, &new_wallet);

    assert_eq!(
        client.get_did_for_wallet(&new_wallet),
        did,
        "reverse lookup must point to the same DID after wallet rotation"
    );
    // Old wallet's reverse entry must have been cleared. Assert via storage
    // rather than `try_get_did_for_wallet`: contract panics are non-unwinding
    // under Soroban v20 testutils and would abort the test process.
    env.as_contract(&client.address, || {
        assert!(
            !env.storage()
                .persistent()
                .has(&DataKey::WalletDid(wallet.clone())),
            "old wallet must no longer have a DID linked after rotation"
        );
    });
}

/// Verify that `get_did_for_wallet` returns an error for an unlinked wallet.
#[test]
#[ignore = "contract panics are non-unwinding under Soroban v20 testutils and abort the test process"]
fn test_get_did_for_wallet_unlinked_fails() {
    let (env, client, _signing_key) = setup();
    let wallet = Address::generate(&env);
    let result = client.try_get_did_for_wallet(&wallet);
    assert!(
        result.is_err(),
        "wallet with no linked DID must return an error"
    );
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

