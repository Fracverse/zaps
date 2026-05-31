#![cfg(test)]

use super::*;
use soroban_sdk::{
    contract, contractimpl, symbol_short,
    testutils::{Address as _, Ledger},
    Address, Env, Error as SdkError,
};

fn sdk_err(e: OracleError) -> SdkError {
    SdkError::from_contract_error(e as u32)
}

#[contract]
struct MockSep40;

#[contracttype]
enum MockKey {
    Decimals,
    Price(Asset),
}

#[contractimpl]
impl MockSep40 {
    pub fn set_decimals(env: Env, decimals: u32) {
        env.storage().instance().set(&MockKey::Decimals, &decimals);
    }

    pub fn set_price(env: Env, asset: Asset, price: i128, timestamp: u64) {
        env.storage()
            .persistent()
            .set(&MockKey::Price(asset), &PriceData { price, timestamp });
    }

    pub fn decimals(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&MockKey::Decimals)
            .unwrap_or(7)
    }

    pub fn lastprice(env: Env, asset: Asset) -> Option<PriceData> {
        env.storage().persistent().get(&MockKey::Price(asset))
    }
}

struct Setup {
    env: Env,
    client: PriceOracleClient<'static>,
    admin: Address,
    manual1: Address,
    manual2: Address,
    asset: Asset,
}

impl Setup {
    fn new() -> Self {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_000);

        let admin = Address::generate(&env);
        let manual1 = Address::generate(&env);
        let manual2 = Address::generate(&env);
        let contract = env.register_contract(None, PriceOracle);
        let client = PriceOracleClient::new(&env, &contract);
        let base = Asset::Other(symbol_short!("USD"));
        client.initialize(&admin, &base, &7, &60, &600);

        let client: PriceOracleClient<'static> = unsafe { core::mem::transmute(client) };
        let asset = Asset::Other(symbol_short!("XLM"));

        Self {
            env,
            client,
            admin,
            manual1,
            manual2,
            asset,
        }
    }

    fn add_manual_sources(&self) {
        self.client
            .add_source(&self.manual1, &SourceType::Manual, &300, &1_000);
        self.client
            .add_source(&self.manual2, &SourceType::Manual, &300, &1_000);
    }
}

#[test]
fn initialize_sets_sep40_metadata() {
    let s = Setup::new();
    assert_eq!(s.client.decimals(), 7);
    assert_eq!(s.client.resolution(), 60);
    assert_eq!(s.client.base(), Asset::Other(symbol_short!("USD")));
    assert_eq!(s.client.get_sources().len(), 0);
}

#[test]
fn double_initialize_rejected() {
    let s = Setup::new();
    assert_eq!(
        s.client
            .try_initialize(&s.admin, &Asset::Other(symbol_short!("USD")), &7, &60, &600),
        Err(Ok(sdk_err(OracleError::AlreadyInitialized)))
    );
}

#[test]
fn manual_sources_are_median_aggregated() {
    let s = Setup::new();
    s.add_manual_sources();

    s.client.submit_price(&s.manual1, &s.asset, &1_100_0000);
    s.client.submit_price(&s.manual2, &s.asset, &1_300_0000);

    let result = s.client.get_price(&s.asset);
    assert_eq!(result.price, 1_200_0000);
    assert_eq!(result.sources_used, 2);
    assert_eq!(result.decimals, 7);
    assert!(!result.is_fallback);
}

#[test]
fn stale_manual_price_is_excluded() {
    let s = Setup::new();
    s.add_manual_sources();

    s.client.submit_price(&s.manual1, &s.asset, &1_000_0000);
    s.env.ledger().set_timestamp(1_400);
    s.client.submit_price(&s.manual2, &s.asset, &2_000_0000);

    let result = s.client.get_price_with_min_sources(&s.asset, &1);
    assert_eq!(result.price, 2_000_0000);
    assert_eq!(result.sources_used, 1);
}

#[test]
fn untrusted_manual_submit_rejected() {
    let s = Setup::new();
    let rogue = Address::generate(&s.env);
    assert_eq!(
        s.client.try_submit_price(&rogue, &s.asset, &1_000_0000),
        Err(Ok(sdk_err(OracleError::Unauthorized)))
    );
}

#[test]
fn invalid_manual_price_rejected() {
    let s = Setup::new();
    s.add_manual_sources();
    assert_eq!(
        s.client.try_submit_price(&s.manual1, &s.asset, &0),
        Err(Ok(sdk_err(OracleError::InvalidPrice)))
    );
}

#[test]
fn sep40_source_is_read_and_normalized() {
    let s = Setup::new();
    let mock_id = s.env.register_contract(None, MockSep40);
    let mock = MockSep40Client::new(&s.env, &mock_id);
    mock.set_decimals(&6);
    mock.set_price(&s.asset, &1_250_000, &1_000);

    s.client
        .add_source(&mock_id, &SourceType::Sep40, &300, &1_000);
    let result = s.client.get_price(&s.asset);

    assert_eq!(result.price, 1_250_0000);
    assert_eq!(result.sources_used, 1);
}

#[test]
fn outlier_is_removed_by_deviation_filter() {
    let s = Setup::new();
    s.add_manual_sources();
    let manual3 = Address::generate(&s.env);
    s.client
        .add_source(&manual3, &SourceType::Manual, &300, &500);

    s.client.submit_price(&s.manual1, &s.asset, &1_000_0000);
    s.client.submit_price(&s.manual2, &s.asset, &1_020_0000);
    s.client.submit_price(&manual3, &s.asset, &2_000_0000);

    let result = s.client.get_price_with_min_sources(&s.asset, &2);
    assert_eq!(result.price, 1_010_0000);
    assert_eq!(result.sources_used, 2);
}

#[test]
fn last_good_price_used_as_fallback() {
    let s = Setup::new();
    s.add_manual_sources();

    s.client.submit_price(&s.manual1, &s.asset, &1_000_0000);
    let fresh = s.client.get_price(&s.asset);
    assert!(!fresh.is_fallback);

    s.env.ledger().set_timestamp(1_350);
    let fallback = s.client.get_price(&s.asset);
    assert_eq!(fallback.price, 1_000_0000);
    assert!(fallback.is_fallback);
}

#[test]
fn stale_last_good_fallback_rejected() {
    let s = Setup::new();
    s.add_manual_sources();

    s.client.submit_price(&s.manual1, &s.asset, &1_000_0000);
    s.client.get_price(&s.asset);

    s.env.ledger().set_timestamp(2_000);
    assert_eq!(
        s.client.try_get_price(&s.asset),
        Err(Ok(sdk_err(OracleError::StalePrice)))
    );
}

#[test]
fn source_management_updates_config() {
    let s = Setup::new();
    s.client
        .add_source(&s.manual1, &SourceType::Manual, &300, &1_000);
    assert_eq!(s.client.get_sources().len(), 1);

    let config = s.client.get_source_config(&s.manual1);
    assert_eq!(config.source_type, SourceType::Manual);
    assert!(config.enabled);

    s.client.set_source_enabled(&s.manual1, &false);
    assert!(!s.client.get_source_config(&s.manual1).enabled);

    s.client.remove_source(&s.manual1);
    assert_eq!(s.client.get_sources().len(), 0);
}
