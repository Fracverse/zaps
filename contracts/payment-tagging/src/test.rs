#![cfg(test)]

use super::*;
use soroban_sdk::{testutils::Address as _, Address, Bytes, Env};

fn setup() -> (Env, PaymentTaggingClient<'static>, Address, Address, Bytes) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, PaymentTagging);
    let client = PaymentTaggingClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let merchant_owner = Address::generate(&env);
    let merchant_id = Bytes::from_slice(&env, b"merchant-001");
    client.initialize(&admin);
    client.register_merchant(&merchant_id, &merchant_owner);
    (env, client, admin, merchant_owner, merchant_id)
}

fn tag(env: &Env, s: &[u8]) -> Bytes {
    Bytes::from_slice(env, s)
}

fn payment(env: &Env, s: &[u8]) -> Bytes {
    Bytes::from_slice(env, s)
}

#[test]
fn test_initialize() {
    let (_, client, _, _, merchant_id) = setup();
    let m = client.get_merchant(&merchant_id);
    assert!(m.active);
}

#[test]
#[should_panic(expected = "AlreadyInitialized")]
fn test_double_initialize() {
    let (_, client, admin, _, _) = setup();
    client.initialize(&admin);
}

#[test]
fn test_add_and_get_tags() {
    let (env, client, _, owner, merchant_id) = setup();
    let pid = payment(&env, b"pay-001");
    client.add_tag(&owner, &pid, &merchant_id, &tag(&env, b"food"));
    client.add_tag(&owner, &pid, &merchant_id, &tag(&env, b"lunch"));
    let tags = client.get_tags(&pid);
    assert_eq!(tags.len(), 2);
}

#[test]
fn test_has_tag() {
    let (env, client, _, owner, merchant_id) = setup();
    let pid = payment(&env, b"pay-002");
    client.add_tag(&owner, &pid, &merchant_id, &tag(&env, b"travel"));
    assert!(client.has_tag(&pid, &tag(&env, b"travel")));
    assert!(!client.has_tag(&pid, &tag(&env, b"food")));
}

#[test]
fn test_remove_tag() {
    let (env, client, _, owner, merchant_id) = setup();
    let pid = payment(&env, b"pay-003");
    client.add_tag(&owner, &pid, &merchant_id, &tag(&env, b"food"));
    client.add_tag(&owner, &pid, &merchant_id, &tag(&env, b"lunch"));
    client.remove_tag(&owner, &pid, &merchant_id, &tag(&env, b"food"));
    let tags = client.get_tags(&pid);
    assert_eq!(tags.len(), 1);
    assert!(!client.has_tag(&pid, &tag(&env, b"food")));
}

#[test]
#[should_panic(expected = "TagAlreadyExists")]
fn test_duplicate_tag() {
    let (env, client, _, owner, merchant_id) = setup();
    let pid = payment(&env, b"pay-004");
    client.add_tag(&owner, &pid, &merchant_id, &tag(&env, b"food"));
    client.add_tag(&owner, &pid, &merchant_id, &tag(&env, b"food"));
}

#[test]
#[should_panic(expected = "TagNotFound")]
fn test_remove_nonexistent_tag() {
    let (env, client, _, owner, merchant_id) = setup();
    let pid = payment(&env, b"pay-005");
    client.remove_tag(&owner, &pid, &merchant_id, &tag(&env, b"ghost"));
}

#[test]
#[should_panic(expected = "Unauthorized")]
fn test_unauthorized_add_tag() {
    let (env, client, _, _, merchant_id) = setup();
    let rogue = Address::generate(&env);
    let pid = payment(&env, b"pay-006");
    client.add_tag(&rogue, &pid, &merchant_id, &tag(&env, b"food"));
}

#[test]
#[should_panic(expected = "InvalidTagName")]
fn test_empty_tag_name() {
    let (env, client, _, owner, merchant_id) = setup();
    let pid = payment(&env, b"pay-007");
    client.add_tag(&owner, &pid, &merchant_id, &Bytes::new(&env));
}

#[test]
fn test_deactivate_merchant() {
    let (_, client, _, _, merchant_id) = setup();
    client.deactivate_merchant(&merchant_id);
    let m = client.get_merchant(&merchant_id);
    assert!(!m.active);
}

#[test]
#[should_panic(expected = "MerchantNotFound")]
fn test_get_unknown_merchant() {
    let (env, client, _, _, _) = setup();
    client.get_merchant(&Bytes::from_slice(&env, b"unknown"));
}
