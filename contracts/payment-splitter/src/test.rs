#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::Address as _,
    token, Address, Env, Vec,
};

fn setup() -> (Env, PaymentSplitterClient<'static>, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    let charlie = Address::generate(&env);

    let sac = env.register_stellar_asset_contract_v2(admin.clone());
    let token = sac.address();
    let sac_client = token::StellarAssetClient::new(&env, &token);
    sac_client.mint(&admin, &1_000_000);

    let contract_id = env.register_contract(None, PaymentSplitter);
    let client = PaymentSplitterClient::new(&env, &contract_id);

    let mut recipients = Vec::new(&env);
    recipients.push_back(Recipient { address: bob.clone(), share: 60_00 });
    recipients.push_back(Recipient { address: charlie.clone(), share: 40_00 });

    client.initialize(&admin, &token, &SplitType::Percentage, &recipients);

    let client: PaymentSplitterClient<'static> = unsafe { core::mem::transmute(client) };

    (env, client, admin, bob, charlie)
}

#[test]
fn test_initialize() {
    let (_, client, _, _, _) = setup();
    let config = client.get_config();
    assert_eq!(config.recipients.len(), 2);
}

#[test]
fn test_split_percentage() {
    let (env, client, admin, bob, charlie) = setup();
    let token_addr = client.get_token();
    let token = token::Client::new(&env, &token_addr);

    client.split(&admin, &1000);

    assert_eq!(token.balance(&bob), 600);
    assert_eq!(token.balance(&charlie), 400);
}

#[test]
fn test_set_recipients() {
    let (env, client, admin, bob, _) = setup();
    let charlie = Address::generate(&env);

    let mut recipients = Vec::new(&env);
    recipients.push_back(Recipient { address: bob.clone(), share: 50_00 });
    recipients.push_back(Recipient { address: charlie.clone(), share: 50_00 });

    client.set_recipients(&SplitType::Percentage, &recipients);

    let config = client.get_config();
    assert_eq!(config.recipients.len(), 2);
}

#[test]
fn test_transfer_admin() {
    let (_, client, _, _, _) = setup();
    let new_admin = Address::generate(&client.env);
    client.transfer_admin(&new_admin);
    assert_eq!(client.get_admin(), new_admin);
}
