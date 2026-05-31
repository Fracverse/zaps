#![no_std]

use soroban_sdk::{
    contract, contractclient, contracterror, contractimpl, contracttype, panic_with_error, vec,
    Address, Env, Symbol, Vec,
};

const MAX_SOURCES: u32 = 20;
const DEFAULT_MIN_SOURCES: u32 = 1;
const INSTANCE_TTL_THRESHOLD: u32 = 100_000;
const INSTANCE_TTL_EXTEND: u32 = 6_307_200;
const PERSISTENT_TTL_THRESHOLD: u32 = 50_000;
const PERSISTENT_TTL_EXTEND: u32 = 3_153_600;

#[contracttype]
#[derive(Clone)]
enum DataKey {
    Admin,
    Base,
    Assets,
    Decimals,
    Resolution,
    FallbackMaxAge,
    Sources,
    Source(Address),
    Asset(Asset),
    Manual(Address, Asset),
    LastGood(Asset),
}

#[contracttype]
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Asset {
    Stellar(Address),
    Other(Symbol),
}

#[contracttype]
#[derive(Clone)]
pub struct PriceData {
    pub price: i128,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum SourceType {
    Manual = 1,
    Sep40 = 2,
}

#[contracttype]
#[derive(Clone)]
pub struct SourceConfig {
    pub source_type: SourceType,
    pub enabled: bool,
    pub max_age_seconds: u64,
    pub max_deviation_bps: u32,
}

#[contracttype]
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct AggregatedPrice {
    pub price: i128,
    pub timestamp: u64,
    pub sources_used: u32,
    pub decimals: u32,
    pub is_fallback: bool,
}

#[contracterror]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum OracleError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    SourceNotFound = 4,
    SourceAlreadyExists = 5,
    NoValidPrice = 6,
    StalePrice = 7,
    InvalidPrice = 8,
    TooManySources = 9,
    InvalidDecimals = 10,
    InvalidDeviation = 11,
    InvalidMinSources = 12,
    ArithmeticOverflow = 13,
}

#[contractclient(name = "ExternalPriceFeedClient")]
pub trait ExternalPriceFeed {
    fn decimals(env: Env) -> u32;
    fn lastprice(env: Env, asset: Asset) -> Option<PriceData>;
}

fn bump_instance(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(INSTANCE_TTL_THRESHOLD, INSTANCE_TTL_EXTEND);
}

fn bump_persistent(env: &Env, key: &DataKey) {
    env.storage()
        .persistent()
        .extend_ttl(key, PERSISTENT_TTL_THRESHOLD, PERSISTENT_TTL_EXTEND);
}

fn require_initialized(env: &Env) {
    if !env.storage().instance().has(&DataKey::Admin) {
        panic_with_error!(env, OracleError::NotInitialized);
    }
}

fn require_admin(env: &Env) -> Address {
    let admin: Address = env
        .storage()
        .instance()
        .get(&DataKey::Admin)
        .unwrap_or_else(|| panic_with_error!(env, OracleError::NotInitialized));
    admin.require_auth();
    admin
}

fn sources(env: &Env) -> Vec<Address> {
    env.storage()
        .instance()
        .get(&DataKey::Sources)
        .unwrap_or(vec![env])
}

fn source_config(env: &Env, source: &Address) -> SourceConfig {
    env.storage()
        .persistent()
        .get(&DataKey::Source(source.clone()))
        .unwrap_or_else(|| panic_with_error!(env, OracleError::SourceNotFound))
}

fn maybe_source_config(env: &Env, source: &Address) -> Option<SourceConfig> {
    env.storage()
        .persistent()
        .get(&DataKey::Source(source.clone()))
}

fn target_decimals(env: &Env) -> u32 {
    env.storage()
        .instance()
        .get(&DataKey::Decimals)
        .unwrap_or_else(|| panic_with_error!(env, OracleError::NotInitialized))
}

fn fallback_max_age(env: &Env) -> u64 {
    env.storage()
        .instance()
        .get(&DataKey::FallbackMaxAge)
        .unwrap_or(0)
}

fn pow10(env: &Env, decimals: u32) -> i128 {
    if decimals > 18 {
        panic_with_error!(env, OracleError::InvalidDecimals);
    }
    let mut value = 1i128;
    for _ in 0..decimals {
        value = value
            .checked_mul(10)
            .unwrap_or_else(|| panic_with_error!(env, OracleError::ArithmeticOverflow));
    }
    value
}

fn normalize_price(env: &Env, price: i128, source_decimals: u32, target_decimals: u32) -> i128 {
    if price <= 0 {
        panic_with_error!(env, OracleError::InvalidPrice);
    }
    if source_decimals == target_decimals {
        return price;
    }
    if source_decimals > 18 || target_decimals > 18 {
        panic_with_error!(env, OracleError::InvalidDecimals);
    }
    if source_decimals < target_decimals {
        price
            .checked_mul(pow10(env, target_decimals - source_decimals))
            .unwrap_or_else(|| panic_with_error!(env, OracleError::ArithmeticOverflow))
    } else {
        price / pow10(env, source_decimals - target_decimals)
    }
}

fn abs_diff(a: i128, b: i128) -> i128 {
    if a >= b {
        a - b
    } else {
        b - a
    }
}

fn within_deviation(price: i128, median: i128, max_deviation_bps: u32) -> bool {
    if max_deviation_bps == 0 {
        return true;
    }
    abs_diff(price, median) * 10_000 <= median * max_deviation_bps as i128
}

fn median(env: &Env, values: &Vec<i128>) -> i128 {
    let n = values.len() as usize;
    if n == 0 || n > MAX_SOURCES as usize {
        panic_with_error!(env, OracleError::NoValidPrice);
    }
    let mut sorted = [0i128; MAX_SOURCES as usize];
    for (i, value) in values.iter().enumerate() {
        sorted[i] = value;
    }
    for i in 1..n {
        let value = sorted[i];
        let mut j = i;
        while j > 0 && sorted[j - 1] > value {
            sorted[j] = sorted[j - 1];
            j -= 1;
        }
        sorted[j] = value;
    }
    if n % 2 == 1 {
        sorted[n / 2]
    } else {
        (sorted[n / 2 - 1] + sorted[n / 2]) / 2
    }
}

fn latest_timestamp(values: &Vec<PriceData>) -> u64 {
    let mut latest = 0u64;
    for value in values.iter() {
        if value.timestamp > latest {
            latest = value.timestamp;
        }
    }
    latest
}

fn store_last_good(env: &Env, asset: &Asset, price: &AggregatedPrice) {
    let key = DataKey::LastGood(asset.clone());
    env.storage().persistent().set(&key, price);
    bump_persistent(env, &key);
}

fn load_last_good(env: &Env, asset: &Asset) -> Option<AggregatedPrice> {
    let key = DataKey::LastGood(asset.clone());
    let price = env.storage().persistent().get(&key);
    if price.is_some() {
        bump_persistent(env, &key);
    }
    price
}

fn fallback_or_fail(env: &Env, asset: &Asset, now: u64) -> AggregatedPrice {
    if let Some(mut fallback) = load_last_good(env, asset) {
        if fallback_max_age(env) > 0
            && now.saturating_sub(fallback.timestamp) <= fallback_max_age(env)
        {
            fallback.is_fallback = true;
            return fallback;
        }
        panic_with_error!(env, OracleError::StalePrice);
    }
    panic_with_error!(env, OracleError::NoValidPrice);
}

#[contract]
pub struct PriceOracle;

#[contractimpl]
impl PriceOracle {
    pub fn initialize(
        env: Env,
        admin: Address,
        base: Asset,
        decimals: u32,
        resolution: u32,
        fallback_max_age_seconds: u64,
    ) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic_with_error!(env, OracleError::AlreadyInitialized);
        }
        if decimals > 18 {
            panic_with_error!(env, OracleError::InvalidDecimals);
        }
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Base, &base);
        let empty_assets: Vec<Asset> = vec![&env];
        let empty_sources: Vec<Address> = vec![&env];
        env.storage()
            .instance()
            .set(&DataKey::Assets, &empty_assets);
        env.storage().instance().set(&DataKey::Decimals, &decimals);
        env.storage()
            .instance()
            .set(&DataKey::Resolution, &resolution);
        env.storage()
            .instance()
            .set(&DataKey::FallbackMaxAge, &fallback_max_age_seconds);
        env.storage()
            .instance()
            .set(&DataKey::Sources, &empty_sources);
        bump_instance(&env);
    }

    pub fn add_asset(env: Env, asset: Asset) {
        require_admin(&env);
        let mut assets: Vec<Asset> = env
            .storage()
            .instance()
            .get(&DataKey::Assets)
            .unwrap_or(vec![&env]);
        for existing in assets.iter() {
            if existing == asset {
                bump_instance(&env);
                return;
            }
        }
        assets.push_back(asset.clone());
        env.storage().instance().set(&DataKey::Assets, &assets);
        env.storage()
            .persistent()
            .set(&DataKey::Asset(asset.clone()), &true);
        bump_persistent(&env, &DataKey::Asset(asset));
        bump_instance(&env);
    }

    pub fn add_source(
        env: Env,
        source: Address,
        source_type: SourceType,
        max_age_seconds: u64,
        max_deviation_bps: u32,
    ) {
        require_admin(&env);
        if max_deviation_bps > 10_000 {
            panic_with_error!(env, OracleError::InvalidDeviation);
        }
        let mut all_sources = sources(&env);
        if all_sources.len() >= MAX_SOURCES {
            panic_with_error!(env, OracleError::TooManySources);
        }
        if env
            .storage()
            .persistent()
            .has(&DataKey::Source(source.clone()))
        {
            panic_with_error!(env, OracleError::SourceAlreadyExists);
        }
        let config = SourceConfig {
            source_type,
            enabled: true,
            max_age_seconds,
            max_deviation_bps,
        };
        env.storage()
            .persistent()
            .set(&DataKey::Source(source.clone()), &config);
        bump_persistent(&env, &DataKey::Source(source.clone()));
        all_sources.push_back(source);
        env.storage()
            .instance()
            .set(&DataKey::Sources, &all_sources);
        bump_instance(&env);
    }

    pub fn remove_source(env: Env, source: Address) {
        require_admin(&env);
        let all_sources = sources(&env);
        let mut updated = vec![&env];
        let mut found = false;
        for existing in all_sources.iter() {
            if existing == source {
                found = true;
            } else {
                updated.push_back(existing);
            }
        }
        if !found {
            panic_with_error!(env, OracleError::SourceNotFound);
        }
        env.storage().persistent().remove(&DataKey::Source(source));
        env.storage().instance().set(&DataKey::Sources, &updated);
        bump_instance(&env);
    }

    pub fn set_source_enabled(env: Env, source: Address, enabled: bool) {
        require_admin(&env);
        let mut config = source_config(&env, &source);
        config.enabled = enabled;
        env.storage()
            .persistent()
            .set(&DataKey::Source(source.clone()), &config);
        bump_persistent(&env, &DataKey::Source(source));
        bump_instance(&env);
    }

    pub fn submit_price(env: Env, source: Address, asset: Asset, price: i128) {
        require_initialized(&env);
        source.require_auth();
        if price <= 0 {
            panic_with_error!(env, OracleError::InvalidPrice);
        }
        let config = maybe_source_config(&env, &source)
            .unwrap_or_else(|| panic_with_error!(env, OracleError::Unauthorized));
        if !config.enabled || config.source_type != SourceType::Manual {
            panic_with_error!(env, OracleError::Unauthorized);
        }
        let data = PriceData {
            price: normalize_price(&env, price, target_decimals(&env), target_decimals(&env)),
            timestamp: env.ledger().timestamp(),
        };
        let key = DataKey::Manual(source, asset);
        env.storage().persistent().set(&key, &data);
        bump_persistent(&env, &key);
    }

    pub fn get_price(env: Env, asset: Asset) -> AggregatedPrice {
        Self::get_price_with_min_sources(env, asset, DEFAULT_MIN_SOURCES)
    }

    pub fn get_price_with_min_sources(env: Env, asset: Asset, min_sources: u32) -> AggregatedPrice {
        require_initialized(&env);
        if min_sources == 0 || min_sources > MAX_SOURCES {
            panic_with_error!(env, OracleError::InvalidMinSources);
        }

        let target = target_decimals(&env);
        let now = env.ledger().timestamp();
        let all_sources = sources(&env);
        let mut prices = vec![&env];
        let mut raw = vec![&env];
        let mut configs = vec![&env];

        for source in all_sources.iter() {
            let config = source_config(&env, &source);
            if !config.enabled {
                continue;
            }

            let maybe_price = match config.source_type {
                SourceType::Manual => env
                    .storage()
                    .persistent()
                    .get::<DataKey, PriceData>(&DataKey::Manual(source.clone(), asset.clone())),
                SourceType::Sep40 => {
                    let client = ExternalPriceFeedClient::new(&env, &source);
                    match client.lastprice(&asset) {
                        Some(data) => {
                            let decimals = client.decimals();
                            Some(PriceData {
                                price: normalize_price(&env, data.price, decimals, target),
                                timestamp: data.timestamp,
                            })
                        }
                        None => None,
                    }
                }
            };

            if let Some(data) = maybe_price {
                if data.price <= 0 {
                    continue;
                }
                if now.saturating_sub(data.timestamp) <= config.max_age_seconds {
                    prices.push_back(data.price);
                    raw.push_back(data);
                    configs.push_back(config);
                }
            }
        }

        if prices.len() < min_sources {
            return fallback_or_fail(&env, &asset, now);
        }

        let first_median = median(&env, &prices);
        let mut filtered = vec![&env];
        let mut filtered_data = vec![&env];
        for i in 0..prices.len() {
            let price = prices.get(i).unwrap();
            let config = configs.get(i).unwrap();
            if within_deviation(price, first_median, config.max_deviation_bps) {
                filtered.push_back(price);
                filtered_data.push_back(raw.get(i).unwrap());
            }
        }

        if filtered.len() < min_sources {
            return fallback_or_fail(&env, &asset, now);
        }

        let result = AggregatedPrice {
            price: median(&env, &filtered),
            timestamp: latest_timestamp(&filtered_data),
            sources_used: filtered.len(),
            decimals: target,
            is_fallback: false,
        };
        store_last_good(&env, &asset, &result);
        bump_instance(&env);
        result
    }

    pub fn get_source_price(env: Env, source: Address, asset: Asset) -> PriceData {
        require_initialized(&env);
        let config = source_config(&env, &source);
        match config.source_type {
            SourceType::Manual => env
                .storage()
                .persistent()
                .get(&DataKey::Manual(source, asset))
                .unwrap_or_else(|| panic_with_error!(env, OracleError::NoValidPrice)),
            SourceType::Sep40 => ExternalPriceFeedClient::new(&env, &source)
                .lastprice(&asset)
                .unwrap_or_else(|| panic_with_error!(env, OracleError::NoValidPrice)),
        }
    }

    pub fn get_sources(env: Env) -> Vec<Address> {
        require_initialized(&env);
        sources(&env)
    }

    pub fn get_source_config(env: Env, source: Address) -> SourceConfig {
        require_initialized(&env);
        source_config(&env, &source)
    }

    pub fn base(env: Env) -> Asset {
        env.storage()
            .instance()
            .get(&DataKey::Base)
            .unwrap_or_else(|| panic_with_error!(env, OracleError::NotInitialized))
    }

    pub fn assets(env: Env) -> Vec<Asset> {
        require_initialized(&env);
        env.storage()
            .instance()
            .get(&DataKey::Assets)
            .unwrap_or(vec![&env])
    }

    pub fn decimals(env: Env) -> u32 {
        target_decimals(&env)
    }

    pub fn resolution(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::Resolution)
            .unwrap_or_else(|| panic_with_error!(env, OracleError::NotInitialized))
    }

    pub fn lastprice(env: Env, asset: Asset) -> Option<PriceData> {
        let result = Self::get_price(env, asset);
        Some(PriceData {
            price: result.price,
            timestamp: result.timestamp,
        })
    }
}

mod test;
