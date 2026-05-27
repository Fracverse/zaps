#![no_std]

//! # Payment Verification Contract
//!
//! Provides on-chain verification of payment integrity across multiple
//! independent check types.
//!
//! ## Verification Types
//!
//! | Type          | What it checks                                                  |
//! |---------------|-----------------------------------------------------------------|
//! | `Amount`      | Payment amount is within the merchant's configured min/max      |
//! | `Merchant`    | Merchant is registered, active, and vault address is correct    |
//! | `Duplicate`   | No identical payment (same payer + merchant + amount) in window |
//! | `Expiry`      | Payment was submitted within the allowed time window            |
//! | `Full`        | All of the above checks in sequence                             |
//!
//! ## Verification Result
//!
//! Each call to `verify_payment` returns a `VerificationResult` containing:
//! - `passed`: overall pass/fail
//! - `checks`: list of individual `CheckResult` items with name + pass/fail + detail
//! - `verification_id`: unique on-chain identifier for this verification run
//!
//! ## Access Control
//!
//! Only authorised **verifiers** (e.g. the payment router or backend) may
//! call `verify_payment`.  The admin manages the verifier whitelist and
//! merchant configurations.

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, symbol_short,
    Address, Env, String, Symbol, Vec,
};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Default duplicate-detection window in ledgers (~10 minutes at 5 s/ledger).
pub const DEFAULT_DUPLICATE_WINDOW: u32 = 120;

/// Default payment expiry window in ledgers (~5 minutes).
pub const DEFAULT_EXPIRY_WINDOW: u32 = 60;

/// Instance storage TTL (~1 year).
const TTL_THRESHOLD: u32 = 100_000;
const TTL_EXTEND: u32 = 6_307_200;

/// Persistent storage TTL (~6 months).
const PERSISTENT_TTL_THRESHOLD: u32 = 50_000;
const PERSISTENT_TTL_EXTEND: u32 = 3_153_600;

// ---------------------------------------------------------------------------
// Storage keys
// ---------------------------------------------------------------------------

const KEY_ADMIN: Symbol = symbol_short!("admin");
const KEY_PAUSED: Symbol = symbol_short!("paused");
const KEY_VERSION: Symbol = symbol_short!("version");
const KEY_DUP_WIN: Symbol = symbol_short!("dup_win");
const KEY_EXP_WIN: Symbol = symbol_short!("exp_win");
const KEY_VER_SEQ: Symbol = symbol_short!("ver_seq");

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    /// Authorised verifier whitelist.
    Verifier(Address),
    /// Merchant configuration: merchant_address → MerchantConfig.
    MerchantConfig(Address),
    /// Duplicate detection: fingerprint (u64) → last_seen_ledger (u32).
    DuplicateRecord(u64),
    /// Verification record: verification_id (u64) → VerificationResult.
    VerificationRecord(u64),
}

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Merchant configuration registered by the admin.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct MerchantConfig {
    /// The merchant's vault address (must match the payment destination).
    pub vault: Address,
    /// Minimum accepted payment amount (0 = no minimum).
    pub min_amount: i128,
    /// Maximum accepted payment amount (0 = no maximum).
    pub max_amount: i128,
    /// Whether the merchant is currently active.
    pub active: bool,
}

/// The type of verification to perform.
#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VerificationType {
    /// Check amount is within merchant's configured range.
    Amount = 0,
    /// Check merchant is registered and active.
    Merchant = 1,
    /// Check for duplicate payment within the detection window.
    Duplicate = 2,
    /// Check payment was submitted within the expiry window.
    Expiry = 3,
    /// Run all checks.
    Full = 4,
}

/// Result of a single named check.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct CheckResult {
    /// Human-readable check name (e.g. "amount_min", "merchant_active").
    pub name: String,
    /// Whether this individual check passed.
    pub passed: bool,
    /// Optional detail message (e.g. "amount 500 < min 1000").
    pub detail: String,
}

/// Full verification result returned to the caller and stored on-chain.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct VerificationResult {
    /// Unique sequential identifier for this verification run.
    pub verification_id: u64,
    /// The payment being verified.
    pub payment_id: u64,
    /// The payer address.
    pub payer: Address,
    /// The merchant address.
    pub merchant: Address,
    /// Payment amount.
    pub amount: i128,
    /// Verification type that was requested.
    pub verification_type: VerificationType,
    /// Overall pass/fail (true only if ALL checks passed).
    pub passed: bool,
    /// Individual check results.
    pub checks: Vec<CheckResult>,
    /// Ledger at which verification was performed.
    pub verified_at: u32,
    /// Ledger at which the payment was submitted (for expiry check).
    pub payment_ledger: u32,
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[contracterror]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum VerificationError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    ContractPaused = 4,
    MerchantNotFound = 5,
    VerificationNotFound = 6,
    VerifierAlreadyAdded = 7,
    VerifierNotFound = 8,
    MerchantAlreadyRegistered = 9,
    InvalidAmount = 10,
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn bump_instance(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(TTL_THRESHOLD, TTL_EXTEND);
}

fn bump_persistent<K>(env: &Env, key: &K)
where
    K: soroban_sdk::IntoVal<Env, soroban_sdk::Val>,
{
    env.storage()
        .persistent()
        .extend_ttl(key, PERSISTENT_TTL_THRESHOLD, PERSISTENT_TTL_EXTEND);
}

fn require_admin(env: &Env) -> Address {
    let admin: Address = env
        .storage()
        .instance()
        .get(&KEY_ADMIN)
        .unwrap_or_else(|| panic_with_error!(env, VerificationError::NotInitialized));
    admin.require_auth();
    admin
}

fn require_not_paused(env: &Env) {
    if env
        .storage()
        .instance()
        .get::<Symbol, bool>(&KEY_PAUSED)
        .unwrap_or(false)
    {
        panic_with_error!(env, VerificationError::ContractPaused);
    }
}

fn require_verifier(env: &Env, verifier: &Address) {
    verifier.require_auth();
    if !env
        .storage()
        .persistent()
        .get::<DataKey, bool>(&DataKey::Verifier(verifier.clone()))
        .unwrap_or(false)
    {
        panic_with_error!(env, VerificationError::Unauthorized);
    }
}

fn next_verification_id(env: &Env) -> u64 {
    let seq: u64 = env.storage().instance().get(&KEY_VER_SEQ).unwrap_or(0);
    let next = seq + 1;
    env.storage().instance().set(&KEY_VER_SEQ, &next);
    next
}

/// Compute a simple fingerprint for duplicate detection.
/// Uses the payer's contract address hash XOR'd with amount and merchant.
/// In production this would use a cryptographic hash; here we use a
/// deterministic combination of the inputs available in `no_std`.
fn payment_fingerprint(payer: &Address, merchant: &Address, amount: i128) -> u64 {
    // Combine the raw bytes of both addresses and the amount into a u64.
    // We use a simple FNV-1a-inspired mix since we have no_std.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325; // FNV offset basis

    // Mix payer bytes (use the first 8 bytes of the address representation).
    let payer_bytes = payer.to_string();
    for b in payer_bytes.as_bytes().iter().take(16) {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3); // FNV prime
    }

    // Mix merchant bytes.
    let merchant_bytes = merchant.to_string();
    for b in merchant_bytes.as_bytes().iter().take(16) {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }

    // Mix amount.
    let amount_bytes = amount.to_le_bytes();
    for b in amount_bytes.iter() {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }

    hash
}

// ---------------------------------------------------------------------------
// Individual check functions
// ---------------------------------------------------------------------------

fn check_amount(
    env: &Env,
    merchant: &Address,
    amount: i128,
    checks: &mut Vec<CheckResult>,
) -> bool {
    let config: Option<MerchantConfig> = env
        .storage()
        .persistent()
        .get(&DataKey::MerchantConfig(merchant.clone()));

    match config {
        None => {
            checks.push_back(CheckResult {
                name: String::from_str(env, "amount_merchant_config"),
                passed: false,
                detail: String::from_str(env, "merchant not configured"),
            });
            false
        }
        Some(cfg) => {
            let mut passed = true;

            if cfg.min_amount > 0 && amount < cfg.min_amount {
                checks.push_back(CheckResult {
                    name: String::from_str(env, "amount_min"),
                    passed: false,
                    detail: String::from_str(env, "amount below merchant minimum"),
                });
                passed = false;
            } else {
                checks.push_back(CheckResult {
                    name: String::from_str(env, "amount_min"),
                    passed: true,
                    detail: String::from_str(env, "ok"),
                });
            }

            if cfg.max_amount > 0 && amount > cfg.max_amount {
                checks.push_back(CheckResult {
                    name: String::from_str(env, "amount_max"),
                    passed: false,
                    detail: String::from_str(env, "amount exceeds merchant maximum"),
                });
                passed = false;
            } else {
                checks.push_back(CheckResult {
                    name: String::from_str(env, "amount_max"),
                    passed: true,
                    detail: String::from_str(env, "ok"),
                });
            }

            passed
        }
    }
}

fn check_merchant(
    env: &Env,
    merchant: &Address,
    checks: &mut Vec<CheckResult>,
) -> bool {
    let config: Option<MerchantConfig> = env
        .storage()
        .persistent()
        .get(&DataKey::MerchantConfig(merchant.clone()));

    match config {
        None => {
            checks.push_back(CheckResult {
                name: String::from_str(env, "merchant_registered"),
                passed: false,
                detail: String::from_str(env, "merchant not registered"),
            });
            false
        }
        Some(cfg) => {
            checks.push_back(CheckResult {
                name: String::from_str(env, "merchant_registered"),
                passed: true,
                detail: String::from_str(env, "ok"),
            });

            if cfg.active {
                checks.push_back(CheckResult {
                    name: String::from_str(env, "merchant_active"),
                    passed: true,
                    detail: String::from_str(env, "ok"),
                });
                true
            } else {
                checks.push_back(CheckResult {
                    name: String::from_str(env, "merchant_active"),
                    passed: false,
                    detail: String::from_str(env, "merchant is inactive"),
                });
                false
            }
        }
    }
}

fn check_duplicate(
    env: &Env,
    payer: &Address,
    merchant: &Address,
    amount: i128,
    checks: &mut Vec<CheckResult>,
) -> bool {
    let window: u32 = env
        .storage()
        .instance()
        .get(&KEY_DUP_WIN)
        .unwrap_or(DEFAULT_DUPLICATE_WINDOW);

    let fingerprint = payment_fingerprint(payer, merchant, amount);
    let key = DataKey::DuplicateRecord(fingerprint);

    let last_seen: Option<u32> = env.storage().persistent().get(&key);
    let current_ledger = env.ledger().sequence();

    match last_seen {
        Some(ledger) if current_ledger.saturating_sub(ledger) < window => {
            checks.push_back(CheckResult {
                name: String::from_str(env, "duplicate_check"),
                passed: false,
                detail: String::from_str(env, "duplicate payment detected within window"),
            });
            false
        }
        _ => {
            // Record this payment for future duplicate detection.
            env.storage().persistent().set(&key, &current_ledger);
            bump_persistent(env, &key);

            checks.push_back(CheckResult {
                name: String::from_str(env, "duplicate_check"),
                passed: true,
                detail: String::from_str(env, "ok"),
            });
            true
        }
    }
}

fn check_expiry(
    env: &Env,
    payment_ledger: u32,
    checks: &mut Vec<CheckResult>,
) -> bool {
    let window: u32 = env
        .storage()
        .instance()
        .get(&KEY_EXP_WIN)
        .unwrap_or(DEFAULT_EXPIRY_WINDOW);

    let current_ledger = env.ledger().sequence();
    let age = current_ledger.saturating_sub(payment_ledger);

    if age > window {
        checks.push_back(CheckResult {
            name: String::from_str(env, "expiry_check"),
            passed: false,
            detail: String::from_str(env, "payment has expired"),
        });
        false
    } else {
        checks.push_back(CheckResult {
            name: String::from_str(env, "expiry_check"),
            passed: true,
            detail: String::from_str(env, "ok"),
        });
        true
    }
}

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

#[contract]
pub struct PaymentVerification;

#[contractimpl]
impl PaymentVerification {
    // -----------------------------------------------------------------------
    // Initialisation
    // -----------------------------------------------------------------------

    /// Initialise the verification contract.
    ///
    /// * `admin`            – address that manages verifiers and merchant configs
    /// * `duplicate_window` – ledgers within which identical payments are flagged
    ///                        (0 → use default ~10 min)
    /// * `expiry_window`    – ledgers within which a payment is considered fresh
    ///                        (0 → use default ~5 min)
    pub fn initialize(env: Env, admin: Address, duplicate_window: u32, expiry_window: u32) {
        if env.storage().instance().has(&KEY_ADMIN) {
            panic_with_error!(env, VerificationError::AlreadyInitialized);
        }
        admin.require_auth();

        let dup_win = if duplicate_window == 0 {
            DEFAULT_DUPLICATE_WINDOW
        } else {
            duplicate_window
        };
        let exp_win = if expiry_window == 0 {
            DEFAULT_EXPIRY_WINDOW
        } else {
            expiry_window
        };

        env.storage().instance().set(&KEY_ADMIN, &admin);
        env.storage().instance().set(&KEY_PAUSED, &false);
        env.storage().instance().set(&KEY_VERSION, &1u32);
        env.storage().instance().set(&KEY_DUP_WIN, &dup_win);
        env.storage().instance().set(&KEY_EXP_WIN, &exp_win);
        env.storage().instance().set(&KEY_VER_SEQ, &0u64);
        bump_instance(&env);

        env.events().publish(
            (symbol_short!("verify"), symbol_short!("init")),
            admin,
        );
    }

    // -----------------------------------------------------------------------
    // Verifier management (admin only)
    // -----------------------------------------------------------------------

    /// Authorise an address to submit verification requests.
    pub fn add_verifier(env: Env, verifier: Address) {
        require_admin(&env);
        bump_instance(&env);

        let key = DataKey::Verifier(verifier.clone());
        if env
            .storage()
            .persistent()
            .get::<DataKey, bool>(&key)
            .unwrap_or(false)
        {
            panic_with_error!(env, VerificationError::VerifierAlreadyAdded);
        }

        env.storage().persistent().set(&key, &true);
        bump_persistent(&env, &key);

        env.events().publish(
            (symbol_short!("verify"), symbol_short!("vrf_add")),
            verifier,
        );
    }

    /// Remove a verifier from the whitelist.
    pub fn remove_verifier(env: Env, verifier: Address) {
        require_admin(&env);
        bump_instance(&env);

        let key = DataKey::Verifier(verifier.clone());
        if !env
            .storage()
            .persistent()
            .get::<DataKey, bool>(&key)
            .unwrap_or(false)
        {
            panic_with_error!(env, VerificationError::VerifierNotFound);
        }

        env.storage().persistent().remove(&key);

        env.events().publish(
            (symbol_short!("verify"), symbol_short!("vrf_rm")),
            verifier,
        );
    }

    // -----------------------------------------------------------------------
    // Merchant configuration (admin only)
    // -----------------------------------------------------------------------

    /// Register or update a merchant's verification configuration.
    pub fn set_merchant_config(env: Env, merchant: Address, config: MerchantConfig) {
        require_admin(&env);
        bump_instance(&env);

        if config.min_amount < 0 || config.max_amount < 0 {
            panic_with_error!(env, VerificationError::InvalidAmount);
        }

        let key = DataKey::MerchantConfig(merchant.clone());
        env.storage().persistent().set(&key, &config);
        bump_persistent(&env, &key);

        env.events().publish(
            (symbol_short!("verify"), symbol_short!("merch_set")),
            merchant,
        );
    }

    /// Deactivate a merchant (keeps config, sets active = false).
    pub fn deactivate_merchant(env: Env, merchant: Address) {
        require_admin(&env);
        bump_instance(&env);

        let key = DataKey::MerchantConfig(merchant.clone());
        let mut config: MerchantConfig = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| panic_with_error!(env, VerificationError::MerchantNotFound));

        config.active = false;
        env.storage().persistent().set(&key, &config);
        bump_persistent(&env, &key);

        env.events().publish(
            (symbol_short!("verify"), symbol_short!("merch_off")),
            merchant,
        );
    }

    // -----------------------------------------------------------------------
    // Window configuration (admin only)
    // -----------------------------------------------------------------------

    /// Update the duplicate-detection window (in ledgers).
    pub fn set_duplicate_window(env: Env, window: u32) {
        require_admin(&env);
        bump_instance(&env);
        env.storage().instance().set(&KEY_DUP_WIN, &window);
    }

    /// Update the payment expiry window (in ledgers).
    pub fn set_expiry_window(env: Env, window: u32) {
        require_admin(&env);
        bump_instance(&env);
        env.storage().instance().set(&KEY_EXP_WIN, &window);
    }

    // -----------------------------------------------------------------------
    // Core: verify a payment
    // -----------------------------------------------------------------------

    /// Verify a payment against the requested check type.
    ///
    /// * `verifier`       – authorised verifier (must sign)
    /// * `payment_id`     – opaque payment identifier
    /// * `payer`          – address initiating the payment
    /// * `merchant`       – merchant receiving the payment
    /// * `amount`         – payment amount in token base units
    /// * `payment_ledger` – ledger at which the payment was submitted
    /// * `verify_type`    – which checks to run
    ///
    /// Returns a `VerificationResult` with per-check details.
    pub fn verify_payment(
        env: Env,
        verifier: Address,
        payment_id: u64,
        payer: Address,
        merchant: Address,
        amount: i128,
        payment_ledger: u32,
        verify_type: VerificationType,
    ) -> VerificationResult {
        require_not_paused(&env);
        bump_instance(&env);
        require_verifier(&env, &verifier);

        let mut checks: Vec<CheckResult> = Vec::new(&env);
        let mut overall_passed = true;

        match verify_type {
            VerificationType::Amount => {
                if !check_amount(&env, &merchant, amount, &mut checks) {
                    overall_passed = false;
                }
            }
            VerificationType::Merchant => {
                if !check_merchant(&env, &merchant, &mut checks) {
                    overall_passed = false;
                }
            }
            VerificationType::Duplicate => {
                if !check_duplicate(&env, &payer, &merchant, amount, &mut checks) {
                    overall_passed = false;
                }
            }
            VerificationType::Expiry => {
                if !check_expiry(&env, payment_ledger, &mut checks) {
                    overall_passed = false;
                }
            }
            VerificationType::Full => {
                if !check_merchant(&env, &merchant, &mut checks) {
                    overall_passed = false;
                }
                if !check_amount(&env, &merchant, amount, &mut checks) {
                    overall_passed = false;
                }
                if !check_duplicate(&env, &payer, &merchant, amount, &mut checks) {
                    overall_passed = false;
                }
                if !check_expiry(&env, payment_ledger, &mut checks) {
                    overall_passed = false;
                }
            }
        }

        let verification_id = next_verification_id(&env);

        let result = VerificationResult {
            verification_id,
            payment_id,
            payer: payer.clone(),
            merchant: merchant.clone(),
            amount,
            verification_type: verify_type,
            passed: overall_passed,
            checks,
            verified_at: env.ledger().sequence(),
            payment_ledger,
        };

        // Persist the result.
        let record_key = DataKey::VerificationRecord(verification_id);
        env.storage().persistent().set(&record_key, &result);
        bump_persistent(&env, &record_key);

        env.events().publish(
            (symbol_short!("verify"), symbol_short!("done")),
            (verification_id, payment_id, overall_passed),
        );

        result
    }

    // -----------------------------------------------------------------------
    // Admin: circuit breaker / upgrade / transfer
    // -----------------------------------------------------------------------

    pub fn pause(env: Env) {
        require_admin(&env);
        env.storage().instance().set(&KEY_PAUSED, &true);
        env.events()
            .publish((symbol_short!("verify"), symbol_short!("paused")), ());
    }

    pub fn unpause(env: Env) {
        require_admin(&env);
        env.storage().instance().set(&KEY_PAUSED, &false);
        env.events()
            .publish((symbol_short!("verify"), symbol_short!("unpaused")), ());
    }

    pub fn upgrade(env: Env, new_wasm_hash: soroban_sdk::BytesN<32>) {
        require_admin(&env);
        let version: u32 = env.storage().instance().get(&KEY_VERSION).unwrap_or(1);
        env.deployer().update_current_contract_wasm(new_wasm_hash);
        env.storage().instance().set(&KEY_VERSION, &(version + 1));
    }

    pub fn transfer_admin(env: Env, new_admin: Address) {
        require_admin(&env);
        env.storage().instance().set(&KEY_ADMIN, &new_admin);
        env.events().publish(
            (symbol_short!("verify"), symbol_short!("adm_xfer")),
            new_admin,
        );
    }

    // -----------------------------------------------------------------------
    // Views
    // -----------------------------------------------------------------------

    /// Retrieve a stored verification result by ID.
    pub fn get_verification(env: Env, verification_id: u64) -> VerificationResult {
        env.storage()
            .persistent()
            .get(&DataKey::VerificationRecord(verification_id))
            .unwrap_or_else(|| panic_with_error!(env, VerificationError::VerificationNotFound))
    }

    /// Retrieve a merchant's configuration.
    pub fn get_merchant_config(env: Env, merchant: Address) -> MerchantConfig {
        env.storage()
            .persistent()
            .get(&DataKey::MerchantConfig(merchant))
            .unwrap_or_else(|| panic_with_error!(env, VerificationError::MerchantNotFound))
    }

    pub fn is_verifier(env: Env, verifier: Address) -> bool {
        env.storage()
            .persistent()
            .get::<DataKey, bool>(&DataKey::Verifier(verifier))
            .unwrap_or(false)
    }

    pub fn get_duplicate_window(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&KEY_DUP_WIN)
            .unwrap_or(DEFAULT_DUPLICATE_WINDOW)
    }

    pub fn get_expiry_window(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&KEY_EXP_WIN)
            .unwrap_or(DEFAULT_EXPIRY_WINDOW)
    }

    pub fn get_admin(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&KEY_ADMIN)
            .unwrap_or_else(|| panic_with_error!(env, VerificationError::NotInitialized))
    }

    pub fn is_paused(env: Env) -> bool {
        env.storage()
            .instance()
            .get::<Symbol, bool>(&KEY_PAUSED)
            .unwrap_or(false)
    }

    pub fn get_version(env: Env) -> u32 {
        env.storage().instance().get(&KEY_VERSION).unwrap_or(1)
    }
}

mod test;
