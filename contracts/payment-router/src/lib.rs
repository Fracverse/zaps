#![no_std]

use core::option::Option;
use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::{
    contract, contractclient, contracterror, contractimpl, contracttype, panic_with_error,
    symbol_short, Address, Bytes, Env, Symbol,
};

const REGISTRY_KEY: Symbol = symbol_short!("registry");
const REENTRANCY_KEY: Symbol = symbol_short!("locked");

#[contracttype]
#[derive(Clone)]
pub struct MerchantMetadata {
    pub settlement_asset: Address,
    pub vault: Address,
    pub active: bool,
    pub fx_router: Option<Address>,
}

pub struct FxSettlementCtx<'a> {
    pub env: &'a Env,
    pub from: &'a Address,
    pub merchant: &'a MerchantMetadata,
    pub merchant_id: &'a Bytes,
    pub send_asset: &'a Address,
    pub send_amount: i128,
    pub settlement_asset: &'a Address,
    pub min_receive: i128,
}

#[contracttype]
pub struct PaymentEvent {
    pub payer: Address,
    pub merchant_id: Bytes,
    pub send_asset: Address,
    pub send_amount: i128,
    pub settlement_asset: Address,
    pub settled_amount: i128,
}

#[contracterror]
#[derive(Copy, Clone, PartialEq, Eq)]
pub enum PaymentError {
    AlreadyInitialized = 1,
    RegistryNotSet = 2,
    InvalidSendAmount = 3,
    InvalidMinReceive = 4,
    Reentrancy = 5,
    MerchantInactive = 6,
    FxRouterMissing = 7,
    SettlementBelowMin = 8,
    FxSwapFailed = 9,
}

#[contractclient(name = "BLINKSRegistryClient")]
pub trait BLINKSRegistry {
    fn get_merchant(env: Env, merchant_id: Bytes) -> MerchantMetadata;
}

#[contractclient(name = "MerchantVaultClient")]
pub trait MerchantVault {
    fn credit(env: Env, merchant_id: Bytes, asset: Address, amount: i128, payer: Address);
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

#[contract]
pub struct PaymentRouter;

#[contractimpl]
impl PaymentRouter {
    pub fn init(env: Env, registry: Address) {
        if env.storage().instance().has(&REGISTRY_KEY) {
            panic_with_error!(env, PaymentError::AlreadyInitialized);
        }

        env.storage().instance().set(&REGISTRY_KEY, &registry);
    }

    pub fn pay(
        env: Env,
        from: Address,
        merchant_id: Bytes,
        send_asset: Address,
        send_amount: i128,
        min_receive: i128,
    ) -> i128 {
        if send_amount <= 0 {
            panic_with_error!(env, PaymentError::InvalidSendAmount);
        }
        if min_receive <= 0 {
            panic_with_error!(env, PaymentError::InvalidMinReceive);
        }

        from.require_auth();
        enter(&env);

        let registry = registry_address(&env);
        let registry_client = BLINKSRegistryClient::new(&env, &registry);
        let merchant = registry_client.get_merchant(&merchant_id);

        if !merchant.active {
            emit_failed(
                &env,
                &from,
                &merchant_id,
                &send_asset,
                send_amount,
                &merchant.settlement_asset,
                0,
            );
            leave(&env);
            panic_with_error!(env, PaymentError::MerchantInactive);
        }

        let settlement_asset = merchant.settlement_asset.clone();
        emit_initiated(
            &env,
            &from,
            &merchant_id,
            &send_asset,
            send_amount,
            &settlement_asset,
        );

        let settled_amount = if send_asset == settlement_asset {
            settle_direct(&env, &from, &merchant.vault, &settlement_asset, send_amount)
        } else {
            let data = FxSettlementCtx {
                env: &env,
                from: &from,
                merchant: &merchant,
                merchant_id: &merchant_id,
                min_receive,
                send_amount: min_receive,
                send_asset: &send_asset,
                settlement_asset: &settlement_asset,
            };
            settle_with_fx(data)
        };

        if settled_amount < min_receive {
            emit_failed(
                &env,
                &from,
                &merchant_id,
                &send_asset,
                send_amount,
                &settlement_asset,
                settled_amount,
            );
            leave(&env);
            panic_with_error!(env, PaymentError::SettlementBelowMin);
        }

        MerchantVaultClient::new(&env, &merchant.vault).credit(
            &merchant_id,
            &settlement_asset,
            &settled_amount,
            &from,
        );

        emit_settled(
            &env,
            &from,
            &merchant_id,
            &send_asset,
            send_amount,
            &settlement_asset,
            settled_amount,
        );

        leave(&env);
        settled_amount
    }

    pub fn get_registry(env: Env) -> Address {
        registry_address(&env)
    }
}

fn enter(env: &Env) {
    let locked: bool = env
        .storage()
        .instance()
        .get(&REENTRANCY_KEY)
        .unwrap_or(false);
    if locked {
        panic_with_error!(env, PaymentError::Reentrancy);
    }
    env.storage().instance().set(&REENTRANCY_KEY, &true);
}

fn leave(env: &Env) {
    env.storage().instance().set(&REENTRANCY_KEY, &false);
}

fn registry_address(env: &Env) -> Address {
    env.storage()
        .instance()
        .get(&REGISTRY_KEY)
        .unwrap_or_else(|| panic_with_error!(env, PaymentError::RegistryNotSet))
}

fn settle_direct(
    env: &Env,
    from: &Address,
    vault: &Address,
    asset: &Address,
    amount: i128,
) -> i128 {
    let token = TokenClient::new(env, asset);
    let before = token.balance(vault);
    token.transfer(from, vault, &amount);
    let after = token.balance(vault);
    let received = after - before;

    if received <= 0 {
        panic_with_error!(env, PaymentError::FxSwapFailed);
    }

    received
}

fn settle_with_fx(ctx: FxSettlementCtx) -> i128 {
    let FxSettlementCtx {
        env,
        from,
        merchant,
        merchant_id,
        send_asset,
        send_amount,
        settlement_asset,
        min_receive,
    } = ctx;

    let fx_router = match merchant.fx_router.clone() {
        Option::Some(addr) => addr,
        Option::None => {
            emit_failed(
                env,
                from,
                merchant_id,
                send_asset,
                send_amount,
                settlement_asset,
                0,
            );
            panic_with_error!(env, PaymentError::FxRouterMissing);
        }
    };

    let offer_token = TokenClient::new(env, send_asset);
    offer_token.transfer(from, &fx_router, &send_amount);

    let settlement_token = TokenClient::new(env, settlement_asset);
    let router_client = FxRouterClient::new(env, &fx_router);
    let contract_addr = env.current_contract_address();
    let before = settlement_token.balance(&contract_addr);

    let quoted = router_client.swap(
        &contract_addr,
        send_asset,
        &send_amount,
        settlement_asset,
        &min_receive,
    );

    if quoted < min_receive {
        emit_failed(
            env,
            from,
            merchant_id,
            send_asset,
            send_amount,
            settlement_asset,
            quoted,
        );
        panic_with_error!(env, PaymentError::SettlementBelowMin);
    }

    let after = settlement_token.balance(&contract_addr);
    let received = after - before;

    if received < min_receive {
        emit_failed(
            env,
            from,
            merchant_id,
            send_asset,
            send_amount,
            settlement_asset,
            received,
        );
        panic_with_error!(env, PaymentError::FxSwapFailed);
    }

    settlement_token.transfer(&contract_addr, &merchant.vault, &received);
    received
}

fn emit_initiated(
    env: &Env,
    payer: &Address,
    merchant_id: &Bytes,
    send_asset: &Address,
    send_amount: i128,
    settlement_asset: &Address,
) {
    emit_payment_event(
        env,
        "PaymentInitiated",
        PaymentEvent {
            payer: payer.clone(),
            merchant_id: merchant_id.clone(),
            send_asset: send_asset.clone(),
            send_amount,
            settlement_asset: settlement_asset.clone(),
            settled_amount: 0,
        },
    );
}

fn emit_settled(
    env: &Env,
    payer: &Address,
    merchant_id: &Bytes,
    send_asset: &Address,
    send_amount: i128,
    settlement_asset: &Address,
    settled_amount: i128,
) {
    emit_payment_event(
        env,
        "PaymentSettled",
        PaymentEvent {
            payer: payer.clone(),
            merchant_id: merchant_id.clone(),
            send_asset: send_asset.clone(),
            send_amount,
            settlement_asset: settlement_asset.clone(),
            settled_amount,
        },
    );
}

fn emit_failed(
    env: &Env,
    payer: &Address,
    merchant_id: &Bytes,
    send_asset: &Address,
    send_amount: i128,
    settlement_asset: &Address,
    settled_amount: i128,
) {
    emit_payment_event(
        env,
        "PaymentFailed",
        PaymentEvent {
            payer: payer.clone(),
            merchant_id: merchant_id.clone(),
            send_asset: send_asset.clone(),
            send_amount,
            settlement_asset: settlement_asset.clone(),
            settled_amount,
        },
    );
}

fn emit_payment_event(env: &Env, kind: &str, event: PaymentEvent) {
    let kind_symbol = Symbol::new(env, kind);
    env.events()
        .publish((symbol_short!("payment"), kind_symbol), event);
}
