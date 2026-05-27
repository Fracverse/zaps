#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::Address as _,
    Address, Bytes, Env,
};

fn setup() -> (Env, PaymentTaggingClient<'static>, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let merchant = Address::generate(&env);

    let contract_id = env.register_contract(None, PaymentTagging);
    let client = PaymentTaggingClient::new(&env, &contract_id);
    client.initialize(&admin);

    let client: PaymentTaggingClient<'static> = unsafe { core::mem::transmute(client) };

    (env, client, admin, merchant)
}

#[test]
fn test_create_tag() {
    let (env, client, _, merchant) = setup();
    let tag_name = Bytes::from_slice(&env, b"important");

    client.create_tag(&merchant, &tag_name);
    let tag = client.get_tag(&tag_name);
    assert_eq!(tag.merchant, merchant);
    assert!(tag.active);
}

#[test]
fn test_toggle_tag() {
    let (env, client, _, merchant) = setup();
    let tag_name = Bytes::from_slice(&env, b"urgent");

    client.create_tag(&merchant, &tag_name);
    client.toggle_tag(&merchant, &tag_name, &false);
    let tag = client.get_tag(&tag_name);
    assert!(!tag.active);
}

#[test]
fn test_tag_payment() {
    let (env, client, _, merchant) = setup();
    let tag_name = Bytes::from_slice(&env, b"high_value");

    client.create_tag(&merchant, &tag_name);
    client.tag_payment(&merchant, &1, &tag_name);

    let tags = client.get_payment_tags(&1, &merchant);
    assert_eq!(tags.len(), 1);
}

#[test]
fn test_untag_payment() {
    let (env, client, _, merchant) = setup();
    let tag_name = Bytes::from_slice(&env, b"test_tag");

    client.create_tag(&merchant, &tag_name);
    client.tag_payment(&merchant, &1, &tag_name);
    client.untag_payment(&merchant, &1, &tag_name);

    let tags = client.get_payment_tags(&1, &merchant);
    assert_eq!(tags.len(), 0);
}

#[test]
fn test_merchant_tags() {
    let (env, client, _, merchant) = setup();
    let tag1 = Bytes::from_slice(&env, b"tag_a");
    let tag2 = Bytes::from_slice(&env, b"tag_b");

    client.create_tag(&merchant, &tag1);
    client.create_tag(&merchant, &tag2);

    assert_eq!(client.get_merchant_tag_count(&merchant), 2);
}

#[test]
fn test_transfer_admin() {
    let (_, client, _, _) = setup();
    let new_admin = Address::generate(&client.env);
    client.transfer_admin(&new_admin);
}
