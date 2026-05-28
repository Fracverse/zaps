#![cfg(test)]

use super::*;
use soroban_sdk::{symbol_short, testutils::Address as _, vec, Address, Env};

fn setup() -> (Env, PriceOracleClient<'static>, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, PriceOracle);
    let client = PriceOracleClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let source1 = Address::generate(&env);
    let source2 = Address::generate(&env);
    client.initialize(&admin, &vec![&env, source1.clone(), source2.clone()]);
    (env, client, admin, source1, source2)
}

#[test]
fn test_initialize() {
    let (_, client, _, source1, source2) = setup();
    let sources = client.get_sources();
    assert_eq!(sources.len(), 2);
    assert!(sources.contains(&source1));
    assert!(sources.contains(&source2));
}

#[test]
#[should_panic(expected = "AlreadyInitialized")]
fn test_double_initialize() {
    let (env, client, admin, _, _) = setup();
    client.initialize(&admin, &vec![&env]);
}

#[test]
fn test_submit_and_get_price() {
    let (_, client, _, source1, source2) = setup();
    let asset = symbol_short!("XLM");
    client.submit_price(&source1, &asset, &1_100_000); // 1.10
    client.submit_price(&source2, &asset, &1_200_000); // 1.20
    let result = client.get_price(&asset);
    // median of [1_100_000, 1_200_000] = 1_150_000
    assert_eq!(result.price, 1_150_000);
    assert_eq!(result.sources_used, 2);
}

#[test]
fn test_single_source_price() {
    let (_, client, _, source1, _) = setup();
    let asset = symbol_short!("USDC");
    client.submit_price(&source1, &asset, &1_000_000);
    let result = client.get_price(&asset);
    assert_eq!(result.price, 1_000_000);
    assert_eq!(result.sources_used, 1);
}

#[test]
#[should_panic(expected = "NoValidPrice")]
fn test_no_price_submitted() {
    let (_, client, _, _, _) = setup();
    client.get_price(&symbol_short!("BTC"));
}

#[test]
#[should_panic(expected = "InvalidPrice")]
fn test_invalid_price_zero() {
    let (_, client, _, source1, _) = setup();
    client.submit_price(&source1, &symbol_short!("XLM"), &0);
}

#[test]
#[should_panic(expected = "Unauthorized")]
fn test_untrusted_source_rejected() {
    let (env, client, _, _, _) = setup();
    let rogue = Address::generate(&env);
    client.submit_price(&rogue, &symbol_short!("XLM"), &1_000_000);
}

#[test]
fn test_add_remove_source() {
    let (env, client, _, source1, source2) = setup();
    let source3 = Address::generate(&env);
    client.add_source(&source3);
    assert_eq!(client.get_sources().len(), 3);
    client.remove_source(&source2);
    assert_eq!(client.get_sources().len(), 2);
    assert!(!client.get_sources().contains(&source2));
}

#[test]
#[should_panic(expected = "SourceNotFound")]
fn test_remove_nonexistent_source() {
    let (env, client, _, _, _) = setup();
    let unknown = Address::generate(&env);
    client.remove_source(&unknown);
}

#[test]
fn test_stale_price_excluded() {
    let (env, client, _, source1, source2) = setup();
    let asset = symbol_short!("XLM");
    // source1 submits at ledger 0 (default)
    client.submit_price(&source1, &asset, &1_000_000);
    // Advance ledger past DEFAULT_MAX_AGE (720)
    env.ledger().set_sequence_number(800);
    // source2 submits fresh price
    client.submit_price(&source2, &asset, &2_000_000);
    // Only source2's price should be used (max_age=720, source1 is 800 ledgers old)
    let result = client.get_price_with_max_age(&asset, &720);
    assert_eq!(result.sources_used, 1);
    assert_eq!(result.price, 2_000_000);
}

#[test]
fn test_get_source_price() {
    let (_, client, _, source1, _) = setup();
    let asset = symbol_short!("XLM");
    client.submit_price(&source1, &asset, &1_500_000);
    let entry = client.get_source_price(&source1, &asset);
    assert_eq!(entry.price, 1_500_000);
    assert_eq!(entry.source, source1);
}
