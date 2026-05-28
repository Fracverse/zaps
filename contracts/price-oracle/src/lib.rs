#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, vec, Address, Env, Symbol,
    Vec,
};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const KEY_ADMIN: Symbol = symbol_short!("admin");
const KEY_SOURCES: Symbol = symbol_short!("sources");
/// Default max age for a price entry before it is considered stale (in ledgers, ~5 s each).
/// 720 ledgers ≈ 1 hour.
const DEFAULT_MAX_AGE: u32 = 720;
const INSTANCE_TTL_EXTEND: u32 = 6_307_200;
const INSTANCE_TTL_THRESHOLD: u32 = 100_000;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

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
}

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// A single price submission from one oracle source.
#[contracttype]
#[derive(Clone)]
pub struct PriceEntry {
    /// Price scaled by 1_000_000 (6 decimal places).
    pub price: i128,
    /// Ledger sequence at which this price was recorded.
    pub timestamp: u32,
    /// Source address that submitted this price.
    pub source: Address,
}

/// Aggregated price result returned to callers.
#[contracttype]
#[derive(Clone)]
pub struct AggregatedPrice {
    pub price: i128,
    pub sources_used: u32,
    pub timestamp: u32,
}

// ---------------------------------------------------------------------------
// Storage helpers
// ---------------------------------------------------------------------------

fn storage_key_price(asset: &Symbol) -> (Symbol, Symbol) {
    (symbol_short!("price"), asset.clone())
}

fn require_admin(env: &Env) {
    let admin: Address = env.storage().instance().get(&KEY_ADMIN).unwrap();
    admin.require_auth();
}

fn is_initialized(env: &Env) -> bool {
    env.storage().instance().has(&KEY_ADMIN)
}

fn bump(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(INSTANCE_TTL_THRESHOLD, INSTANCE_TTL_EXTEND);
}

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

#[contract]
pub struct PriceOracle;

#[contractimpl]
impl PriceOracle {
    /// Initialize the oracle with an admin and an optional list of trusted sources.
    pub fn initialize(env: Env, admin: Address, sources: Vec<Address>) {
        if is_initialized(&env) {
            panic_with_error!(&env, OracleError::AlreadyInitialized);
        }
        admin.require_auth();
        env.storage().instance().set(&KEY_ADMIN, &admin);
        env.storage().instance().set(&KEY_SOURCES, &sources);
        bump(&env);
    }

    // ------------------------------------------------------------------
    // Admin: source management
    // ------------------------------------------------------------------

    /// Add a trusted oracle source.
    pub fn add_source(env: Env, source: Address) {
        if !is_initialized(&env) {
            panic_with_error!(&env, OracleError::NotInitialized);
        }
        require_admin(&env);
        let mut sources: Vec<Address> = env
            .storage()
            .instance()
            .get(&KEY_SOURCES)
            .unwrap_or(vec![&env]);
        if sources.len() >= 10 {
            panic_with_error!(&env, OracleError::TooManySources);
        }
        for s in sources.iter() {
            if s == source {
                panic_with_error!(&env, OracleError::SourceAlreadyExists);
            }
        }
        sources.push_back(source);
        env.storage().instance().set(&KEY_SOURCES, &sources);
        bump(&env);
    }

    /// Remove a trusted oracle source.
    pub fn remove_source(env: Env, source: Address) {
        if !is_initialized(&env) {
            panic_with_error!(&env, OracleError::NotInitialized);
        }
        require_admin(&env);
        let sources: Vec<Address> = env
            .storage()
            .instance()
            .get(&KEY_SOURCES)
            .unwrap_or(vec![&env]);
        let mut new_sources: Vec<Address> = vec![&env];
        let mut found = false;
        for s in sources.iter() {
            if s == source {
                found = true;
            } else {
                new_sources.push_back(s);
            }
        }
        if !found {
            panic_with_error!(&env, OracleError::SourceNotFound);
        }
        env.storage().instance().set(&KEY_SOURCES, &new_sources);
        bump(&env);
    }

    // ------------------------------------------------------------------
    // Price submission
    // ------------------------------------------------------------------

    /// Submit a price for an asset. Caller must be a registered source.
    pub fn submit_price(env: Env, source: Address, asset: Symbol, price: i128) {
        if !is_initialized(&env) {
            panic_with_error!(&env, OracleError::NotInitialized);
        }
        source.require_auth();
        if price <= 0 {
            panic_with_error!(&env, OracleError::InvalidPrice);
        }
        // Verify source is trusted
        let sources: Vec<Address> = env
            .storage()
            .instance()
            .get(&KEY_SOURCES)
            .unwrap_or(vec![&env]);
        let mut trusted = false;
        for s in sources.iter() {
            if s == source {
                trusted = true;
                break;
            }
        }
        if !trusted {
            panic_with_error!(&env, OracleError::Unauthorized);
        }

        let entry = PriceEntry {
            price,
            timestamp: env.ledger().sequence(),
            source: source.clone(),
        };
        let key = storage_key_price(&asset);
        // Store per-source price using persistent storage
        let source_key = (key.clone(), source);
        env.storage().persistent().set(&source_key, &entry);
        env.storage()
            .persistent()
            .extend_ttl(&source_key, INSTANCE_TTL_THRESHOLD, INSTANCE_TTL_EXTEND);
        bump(&env);
    }

    // ------------------------------------------------------------------
    // Price retrieval
    // ------------------------------------------------------------------

    /// Get the median-aggregated price for an asset.
    /// Returns an error if no valid (non-stale) prices exist.
    pub fn get_price(env: Env, asset: Symbol) -> AggregatedPrice {
        Self::get_price_with_max_age(env, asset, DEFAULT_MAX_AGE)
    }

    /// Get price with a custom staleness threshold (in ledgers).
    pub fn get_price_with_max_age(env: Env, asset: Symbol, max_age: u32) -> AggregatedPrice {
        if !is_initialized(&env) {
            panic_with_error!(&env, OracleError::NotInitialized);
        }
        let sources: Vec<Address> = env
            .storage()
            .instance()
            .get(&KEY_SOURCES)
            .unwrap_or(vec![&env]);
        let current_ledger = env.ledger().sequence();
        let key = storage_key_price(&asset);

        let mut prices: Vec<i128> = vec![&env];
        let mut latest_ts: u32 = 0;

        for source in sources.iter() {
            let source_key = (key.clone(), source);
            if let Some(entry) = env
                .storage()
                .persistent()
                .get::<_, PriceEntry>(&source_key)
            {
                let age = current_ledger.saturating_sub(entry.timestamp);
                if age <= max_age {
                    prices.push_back(entry.price);
                    if entry.timestamp > latest_ts {
                        latest_ts = entry.timestamp;
                    }
                }
            }
        }

        if prices.is_empty() {
            panic_with_error!(&env, OracleError::NoValidPrice);
        }

        let median = Self::median(&env, &prices);
        bump(&env);

        AggregatedPrice {
            price: median,
            sources_used: prices.len(),
            timestamp: latest_ts,
        }
    }

    /// Returns the latest raw price from a specific source (no staleness check).
    pub fn get_source_price(env: Env, source: Address, asset: Symbol) -> PriceEntry {
        if !is_initialized(&env) {
            panic_with_error!(&env, OracleError::NotInitialized);
        }
        let key = storage_key_price(&asset);
        let source_key = (key, source);
        env.storage()
            .persistent()
            .get(&source_key)
            .unwrap_or_else(|| panic_with_error!(&env, OracleError::NoValidPrice))
    }

    /// List all registered sources.
    pub fn get_sources(env: Env) -> Vec<Address> {
        if !is_initialized(&env) {
            panic_with_error!(&env, OracleError::NotInitialized);
        }
        env.storage()
            .instance()
            .get(&KEY_SOURCES)
            .unwrap_or(vec![&env])
    }

    // ------------------------------------------------------------------
    // Internal helpers
    // ------------------------------------------------------------------

    /// Compute median of a non-empty Vec<i128> using insertion sort.
    fn median(env: &Env, values: &Vec<i128>) -> i128 {
        let n = values.len() as usize;
        // Copy into a fixed-size array (max 10 sources)
        let mut arr = [0i128; 10];
        for (i, v) in values.iter().enumerate() {
            arr[i] = v;
        }
        // Insertion sort
        for i in 1..n {
            let key = arr[i];
            let mut j = i;
            while j > 0 && arr[j - 1] > key {
                arr[j] = arr[j - 1];
                j -= 1;
            }
            arr[j] = key;
        }
        if n % 2 == 1 {
            arr[n / 2]
        } else {
            (arr[n / 2 - 1] + arr[n / 2]) / 2
        }
    }
}
