#![no_std]

use soroban_sdk::{
    contract, contractclient, contracterror, contractimpl, contracttype, panic_with_error,
    symbol_short, Address, Bytes, Env, Symbol,
};

// ---------------------------------------------------------------------------
// Storage keys
// ---------------------------------------------------------------------------

const KEY_ADMIN: Symbol = symbol_short!("admin");
const KEY_REGISTRY: Symbol = symbol_short!("registry");
const KEY_PAUSED: Symbol = symbol_short!("paused");
const KEY_FEE_BPS: Symbol = symbol_short!("fee_bps");
const KEY_FEE_DEST: Symbol = symbol_short!("fee_dest");
const KEY_LOCKED: Symbol = symbol_short!("locked");
const KEY_VERSION: Symbol = symbol_short!("version");

/// Ledger TTL extension: ~1 year at ~5s/ledger.
const INSTANCE_TTL_EXTEND: u32 = 6_307_200;
const INSTANCE_TTL_THRESHOLD: u32 = 100_000;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[contracterror]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum PaymentError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    ContractPaused = 4,
    Reentrancy = 5,
    MerchantInactive = 6,
    FxRouterMissing = 7,
    InvalidSendAmount = 8,
    InvalidMinReceive = 9,
    SettlementBelowMin = 10,
    FxSwapFailed = 11,
    InvalidFeeBps = 12,
}

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Merchant metadata returned by the registry.
#[contracttype]
#[derive(Clone)]
pub struct MerchantMetadata {
    pub settlement_asset: Address,
    pub vault: Address,
    pub active: bool,
    pub fx_router: Option<Address>,
}

/// Emitted for every payment lifecycle event.
#[contracttype]
pub struct PaymentEvent {
    pub payer: Address,
    pub merchant_id: Bytes,
    pub send_asset: Address,
    pub send_amount: i128,
    pub settlement_asset: Address,
    pub settled_amount: i128,
    pub fee_amount: i128,
}

// ---------------------------------------------------------------------------
// Cross-contract client traits
// ---------------------------------------------------------------------------

#[contractclient(name = "RegistryClient")]
pub trait BLINKSRegistry {
    fn get_merchant(env: Env, merchant_id: Bytes) -> MerchantMetadata;
}

/// Matches MerchantVault::credit(merchant_id: Address, amount: i128) -> Result<i128, Error>
/// The vault tracks an internal ledger; tokens are transferred separately by the router.
#[contractclient(name = "MerchantVaultClient")]
pub trait MerchantVault {
    fn credit(env: Env, merchant_id: Address, amount: i128) -> i128;
}

#[contractclient(name = "FxRouterClient")]
pub trait FxRouter {
    fn swap(
        env: Env,
        recipient: Address,
        send_asset: Address,
        send_amount: i128,
        dest_asset: Address,
        min_receive: i128,
    ) -> i128;
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn bump_instance(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(INSTANCE_TTL_THRESHOLD, INSTANCE_TTL_EXTEND);
}

fn require_not_paused(env: &Env) {
    let paused: bool = env.storage().instance().get(&KEY_PAUSED).unwrap_or(false);
    if paused {
        panic_with_error!(env, PaymentError::ContractPaused);
    }
}

fn require_admin(env: &Env) -> Address {
    let admin: Address = env
        .storage()
        .instance()
        .get(&KEY_ADMIN)
        .unwrap_or_else(|| panic_with_error!(env, PaymentError::NotInitialized));
    admin.require_auth();
    admin
}

fn registry_address(env: &Env) -> Address {
    env.storage()
        .instance()
        .get(&KEY_REGISTRY)
        .unwrap_or_else(|| panic_with_error!(env, PaymentError::NotInitialized))
}

fn enter(env: &Env) {
    let locked: bool = env.storage().instance().get(&KEY_LOCKED).unwrap_or(false);
    if locked {
        panic_with_error!(env, PaymentError::Reentrancy);
    }
    env.storage().instance().set(&KEY_LOCKED, &true);
}

fn leave(env: &Env) {
    env.storage().instance().set(&KEY_LOCKED, &false);
}

/// Returns `(net_to_merchant, fee)` from a gross amount.
fn split_fee(gross: i128, fee_bps: u32) -> (i128, i128) {
    if fee_bps == 0 {
        return (gross, 0);
    }
    let fee = gross * (fee_bps as i128) / 10_000;
    (gross - fee, fee)
}

/// Direct settlement: same send and settlement asset.
/// Transfers net to vault, fee to fee_dest, then notifies vault ledger.
fn settle_direct(
    env: &Env,
    from: &Address,
    merchant: &MerchantMetadata,
    fee_dest: &Option<Address>,
    asset: &Address,
    send_amount: i128,
    fee_bps: u32,
) -> (i128, i128) {
    use soroban_sdk::token::Client as TokenClient;

    let token = TokenClient::new(env, asset);
    let (net, fee) = split_fee(send_amount, fee_bps);

    // Move tokens: net → vault, fee → fee_dest
    token.transfer(from, &merchant.vault, &net);
    if fee > 0 {
        if let Some(dest) = fee_dest {
            token.transfer(from, dest, &fee);
        }
    }

    // Update vault's internal ledger (vault does not move tokens itself)
    // merchant.vault is an Address used both as token recipient and contract address
    MerchantVaultClient::new(env, &merchant.vault).credit(from, &net);

    (net, fee)
}

struct FxArgs<'a> {
    from: &'a Address,
    merchant: &'a MerchantMetadata,
    merchant_id: &'a Bytes,
    send_asset: &'a Address,
    send_amount: i128,
    settlement_asset: &'a Address,
    min_receive: i128,
    fee_bps: u32,
    fee_dest: &'a Option<Address>,
}

/// FX settlement: swap send asset → settlement asset, then credit vault.
fn settle_with_fx(env: &Env, a: FxArgs) -> (i128, i128) {
    let FxArgs {
        from,
        merchant,
        merchant_id,
        send_asset,
        send_amount,
        settlement_asset,
        min_receive,
        fee_bps,
        fee_dest,
    } = a;
    use soroban_sdk::token::Client as TokenClient;

    let fx_router = match merchant.fx_router.clone() {
        Some(addr) => addr,
        None => {
            emit_event(
                env,
                "PaymentFailed",
                from,
                merchant_id,
                send_asset,
                send_amount,
                settlement_asset,
                0,
                0,
            );
            panic_with_error!(env, PaymentError::FxRouterMissing);
        }
    };

    // Transfer send asset to fx router
    let offer_token = TokenClient::new(env, send_asset);
    offer_token.transfer(from, &fx_router, &send_amount);

    // Swap; router sends settlement asset to this contract
    let contract_addr = env.current_contract_address();
    let settlement_token = TokenClient::new(env, settlement_asset);
    let before = settlement_token.balance(&contract_addr);

    let quoted = FxRouterClient::new(env, &fx_router).swap(
        &contract_addr,
        send_asset,
        &send_amount,
        settlement_asset,
        &min_receive,
    );

    if quoted < min_receive {
        emit_event(
            env,
            "PaymentFailed",
            from,
            merchant_id,
            send_asset,
            send_amount,
            settlement_asset,
            quoted,
            0,
        );
        panic_with_error!(env, PaymentError::SettlementBelowMin);
    }

    let received = settlement_token.balance(&contract_addr) - before;
    if received < min_receive {
        emit_event(
            env,
            "PaymentFailed",
            from,
            merchant_id,
            send_asset,
            send_amount,
            settlement_asset,
            received,
            0,
        );
        panic_with_error!(env, PaymentError::FxSwapFailed);
    }

    let (net, fee) = split_fee(received, fee_bps);

    // Forward net to vault, fee to fee_dest
    settlement_token.transfer(&contract_addr, &merchant.vault, &net);
    if fee > 0 {
        if let Some(dest) = fee_dest {
            settlement_token.transfer(&contract_addr, dest, &fee);
        }
    }

    // Update vault ledger
    MerchantVaultClient::new(env, &merchant.vault).credit(from, &net);

    (net, fee)
}

#[allow(clippy::too_many_arguments)]
fn emit_event(
    env: &Env,
    kind: &str,
    payer: &Address,
    merchant_id: &Bytes,
    send_asset: &Address,
    send_amount: i128,
    settlement_asset: &Address,
    settled_amount: i128,
    fee_amount: i128,
) {
    env.events().publish(
        (symbol_short!("payment"), Symbol::new(env, kind)),
        PaymentEvent {
            payer: payer.clone(),
            merchant_id: merchant_id.clone(),
            send_asset: send_asset.clone(),
            send_amount,
            settlement_asset: settlement_asset.clone(),
            settled_amount,
            fee_amount,
        },
    );
}

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

#[contract]
pub struct PaymentRouter;

#[contractimpl]
impl PaymentRouter {
    // -----------------------------------------------------------------------
    // Initialisation
    // -----------------------------------------------------------------------

    /// Initialise the router. Can only be called once.
    ///
    /// * `admin`    – address that controls admin functions
    /// * `registry` – BLINKS registry contract address
    /// * `fee_bps`  – protocol fee in basis points (0–1000, i.e. 0–10%)
    /// * `fee_dest` – optional address that receives collected fees
    pub fn initialize(
        env: Env,
        admin: Address,
        registry: Address,
        fee_bps: u32,
        fee_dest: Option<Address>,
    ) {
        if env.storage().instance().has(&KEY_ADMIN) {
            panic_with_error!(env, PaymentError::AlreadyInitialized);
        }
        if fee_bps > 1000 {
            panic_with_error!(env, PaymentError::InvalidFeeBps);
        }

        admin.require_auth();

        env.storage().instance().set(&KEY_ADMIN, &admin);
        env.storage().instance().set(&KEY_REGISTRY, &registry);
        env.storage().instance().set(&KEY_FEE_BPS, &fee_bps);
        env.storage().instance().set(&KEY_FEE_DEST, &fee_dest);
        env.storage().instance().set(&KEY_PAUSED, &false);
        env.storage().instance().set(&KEY_LOCKED, &false);
        env.storage().instance().set(&KEY_VERSION, &1u32);

        bump_instance(&env);
    }

    // -----------------------------------------------------------------------
    // Core payment
    // -----------------------------------------------------------------------

    /// Route a payment to a merchant.
    ///
    /// * `from`        – payer (must sign)
    /// * `merchant_id` – opaque merchant identifier in the registry
    /// * `send_asset`  – token the payer is sending (XLM or stablecoin)
    /// * `send_amount` – amount of `send_asset` to send (stroops / base units)
    /// * `min_receive` – minimum settlement amount accepted (slippage guard)
    ///
    /// Returns the net amount credited to the merchant vault.
    pub fn pay(
        env: Env,
        from: Address,
        merchant_id: Bytes,
        send_asset: Address,
        send_amount: i128,
        min_receive: i128,
    ) -> i128 {
        require_not_paused(&env);
        bump_instance(&env);

        if send_amount <= 0 {
            panic_with_error!(env, PaymentError::InvalidSendAmount);
        }
        if min_receive <= 0 {
            panic_with_error!(env, PaymentError::InvalidMinReceive);
        }

        from.require_auth();
        enter(&env);

        let registry = registry_address(&env);
        let merchant = RegistryClient::new(&env, &registry).get_merchant(&merchant_id);

        if !merchant.active {
            emit_event(
                &env,
                "PaymentFailed",
                &from,
                &merchant_id,
                &send_asset,
                send_amount,
                &merchant.settlement_asset,
                0,
                0,
            );
            leave(&env);
            panic_with_error!(env, PaymentError::MerchantInactive);
        }

        let settlement_asset = merchant.settlement_asset.clone();
        let fee_bps: u32 = env.storage().instance().get(&KEY_FEE_BPS).unwrap_or(0);
        let fee_dest: Option<Address> = env.storage().instance().get(&KEY_FEE_DEST).unwrap_or(None);

        emit_event(
            &env,
            "PaymentInitiated",
            &from,
            &merchant_id,
            &send_asset,
            send_amount,
            &settlement_asset,
            0,
            0,
        );

        let (net, fee) = if send_asset == settlement_asset {
            settle_direct(
                &env,
                &from,
                &merchant,
                &fee_dest,
                &settlement_asset,
                send_amount,
                fee_bps,
            )
        } else {
            settle_with_fx(
                &env,
                FxArgs {
                    from: &from,
                    merchant: &merchant,
                    merchant_id: &merchant_id,
                    send_asset: &send_asset,
                    send_amount,
                    settlement_asset: &settlement_asset,
                    min_receive,
                    fee_bps,
                    fee_dest: &fee_dest,
                },
            )
        };

        if net < min_receive {
            emit_event(
                &env,
                "PaymentFailed",
                &from,
                &merchant_id,
                &send_asset,
                send_amount,
                &settlement_asset,
                net,
                fee,
            );
            leave(&env);
            panic_with_error!(env, PaymentError::SettlementBelowMin);
        }

        emit_event(
            &env,
            "PaymentSettled",
            &from,
            &merchant_id,
            &send_asset,
            send_amount,
            &settlement_asset,
            net,
            fee,
        );

        leave(&env);
        net
    }

    // -----------------------------------------------------------------------
    // Admin: pause / unpause (circuit breaker)
    // -----------------------------------------------------------------------

    pub fn pause(env: Env) {
        require_admin(&env);
        env.storage().instance().set(&KEY_PAUSED, &true);
        env.events()
            .publish((symbol_short!("admin"), symbol_short!("paused")), ());
    }

    pub fn unpause(env: Env) {
        require_admin(&env);
        env.storage().instance().set(&KEY_PAUSED, &false);
        env.events()
            .publish((symbol_short!("admin"), symbol_short!("unpaused")), ());
    }

    // -----------------------------------------------------------------------
    // Admin: fee management
    // -----------------------------------------------------------------------

    /// Update protocol fee. `fee_bps` must be ≤ 1000 (10%).
    pub fn set_fee(env: Env, fee_bps: u32, fee_dest: Option<Address>) {
        require_admin(&env);
        if fee_bps > 1000 {
            panic_with_error!(env, PaymentError::InvalidFeeBps);
        }
        env.storage().instance().set(&KEY_FEE_BPS, &fee_bps);
        env.storage().instance().set(&KEY_FEE_DEST, &fee_dest);
        env.events()
            .publish((symbol_short!("admin"), symbol_short!("fee_set")), fee_bps);
    }

    // -----------------------------------------------------------------------
    // Admin: upgrade mechanism
    // -----------------------------------------------------------------------

    /// Upgrade the contract WASM. Bumps the on-chain version counter.
    pub fn upgrade(env: Env, new_wasm_hash: soroban_sdk::BytesN<32>) {
        require_admin(&env);
        let version: u32 = env.storage().instance().get(&KEY_VERSION).unwrap_or(1);
        env.deployer().update_current_contract_wasm(new_wasm_hash);
        env.storage().instance().set(&KEY_VERSION, &(version + 1));
        env.events().publish(
            (symbol_short!("admin"), symbol_short!("upgraded")),
            version + 1,
        );
    }

    // -----------------------------------------------------------------------
    // Admin: transfer admin
    // -----------------------------------------------------------------------

    pub fn transfer_admin(env: Env, new_admin: Address) {
        require_admin(&env);
        env.storage().instance().set(&KEY_ADMIN, &new_admin);
        env.events()
            .publish((symbol_short!("admin"), symbol_short!("xfer")), new_admin);
    }

    // -----------------------------------------------------------------------
    // Views
    // -----------------------------------------------------------------------

    pub fn get_registry(env: Env) -> Address {
        registry_address(&env)
    }

    pub fn get_admin(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&KEY_ADMIN)
            .unwrap_or_else(|| panic_with_error!(env, PaymentError::NotInitialized))
    }

    pub fn is_paused(env: Env) -> bool {
        env.storage().instance().get(&KEY_PAUSED).unwrap_or(false)
    }

    pub fn get_fee_bps(env: Env) -> u32 {
        env.storage().instance().get(&KEY_FEE_BPS).unwrap_or(0)
    }

    pub fn get_fee_dest(env: Env) -> Option<Address> {
        env.storage().instance().get(&KEY_FEE_DEST).unwrap_or(None)
    }

    pub fn get_version(env: Env) -> u32 {
        env.storage().instance().get(&KEY_VERSION).unwrap_or(1)
    }
}

mod test;
