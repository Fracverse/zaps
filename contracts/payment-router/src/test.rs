#![cfg(test)]

use super::*;
use soroban_sdk::{
    contract as scontract, contractimpl as scontractimpl,
    testutils::Address as _,
    token::{Client as TokenClient, StellarAssetClient},
    Address, Bytes, Env, Error as SdkError,
};

fn sdk_err(e: PaymentError) -> SdkError {
    SdkError::from_contract_error(e as u32)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn create_token(env: &Env, admin: &Address) -> Address {
    env.register_stellar_asset_contract_v2(admin.clone())
        .address()
}

fn mint(env: &Env, token: &Address, to: &Address, amount: i128) {
    StellarAssetClient::new(env, token).mint(to, &amount);
}

// ---------------------------------------------------------------------------
// Stub registry
// ---------------------------------------------------------------------------

#[scontract]
pub struct StubRegistry;

#[scontractimpl]
impl StubRegistry {
    pub fn set_merchant(env: Env, merchant_id: Bytes, meta: MerchantMetadata) {
        env.storage().persistent().set(&merchant_id, &meta);
    }
    pub fn get_merchant(env: Env, merchant_id: Bytes) -> MerchantMetadata {
        env.storage().persistent().get(&merchant_id).unwrap()
    }
}

// ---------------------------------------------------------------------------
// Stub vault — matches MerchantVault::credit(merchant_id: Address, amount: i128) -> i128
// ---------------------------------------------------------------------------

#[scontract]
pub struct StubVault;

#[scontractimpl]
impl StubVault {
    pub fn credit(env: Env, merchant_id: Address, amount: i128) -> i128 {
        let key = merchant_id.clone();
        let bal: i128 = env.storage().persistent().get(&key).unwrap_or(0);
        let new_bal = bal + amount;
        env.storage().persistent().set(&key, &new_bal);
        new_bal
    }
    pub fn balance_of(env: Env, merchant_id: Address) -> i128 {
        env.storage().persistent().get(&merchant_id).unwrap_or(0)
    }
}

// ---------------------------------------------------------------------------
// Stub FX router — swaps send_asset for dest_asset at 1:1 ratio
// ---------------------------------------------------------------------------

#[scontract]
pub struct StubFxRouter;

#[scontractimpl]
impl StubFxRouter {
    /// Mints dest_asset to recipient at 1:1 and returns the amount.
    /// In tests we pre-mint dest_asset to the router so it can transfer.
    pub fn swap(
        env: Env,
        recipient: Address,
        _send_asset: Address,
        send_amount: i128,
        dest_asset: Address,
        _min_receive: i128,
    ) -> i128 {
        TokenClient::new(&env, &dest_asset).transfer(
            &env.current_contract_address(),
            &recipient,
            &send_amount,
        );
        send_amount
    }
}

// ---------------------------------------------------------------------------
// Setup helper
// ---------------------------------------------------------------------------

struct Setup {
    env: Env,
    client: PaymentRouterClient<'static>,
    #[allow(dead_code)]
    admin: Address,
    payer: Address,
    merchant_id: Bytes,
    usdc: Address,
    vault_id: Address,
}

impl Setup {
    fn new() -> Self {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let payer = Address::generate(&env);

        let usdc = create_token(&env, &admin);
        mint(&env, &usdc, &payer, 1_000_000);

        let vault_id = env.register_contract(None, StubVault);
        let merchant_id = Bytes::from_slice(&env, b"merchant_001");

        let registry = env.register_contract(None, StubRegistry);
        StubRegistryClient::new(&env, &registry).set_merchant(
            &merchant_id,
            &MerchantMetadata {
                settlement_asset: usdc.clone(),
                vault: vault_id.clone(),
                active: true,
                fx_router: None,
            },
        );

        let router_id = env.register_contract(None, PaymentRouter);
        let client = PaymentRouterClient::new(&env, &router_id);
        client.initialize(&admin, &registry, &0u32, &None);

        let client: PaymentRouterClient<'static> = unsafe { core::mem::transmute(client) };

        Setup {
            env,
            client,
            admin,
            payer,
            merchant_id,
            usdc,
            vault_id,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn test_initialize_once() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let registry = Address::generate(&env);
    let router_id = env.register_contract(None, PaymentRouter);
    let client = PaymentRouterClient::new(&env, &router_id);
    client.initialize(&admin, &registry, &0u32, &None);
    assert_eq!(
        client.try_initialize(&admin, &registry, &0u32, &None),
        Err(Ok(sdk_err(PaymentError::AlreadyInitialized)))
    );
}

#[test]
fn test_initialize_invalid_fee() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let registry = Address::generate(&env);
    let router_id = env.register_contract(None, PaymentRouter);
    let client = PaymentRouterClient::new(&env, &router_id);
    assert_eq!(
        client.try_initialize(&admin, &registry, &1001u32, &None),
        Err(Ok(sdk_err(PaymentError::InvalidFeeBps)))
    );
}

#[test]
fn test_pause_unpause() {
    let s = Setup::new();
    assert!(!s.client.is_paused());
    s.client.pause();
    assert!(s.client.is_paused());
    s.client.unpause();
    assert!(!s.client.is_paused());
}

#[test]
fn test_pay_rejected_when_paused() {
    let s = Setup::new();
    s.client.pause();
    assert_eq!(
        s.client
            .try_pay(&s.payer, &s.merchant_id, &s.usdc, &100i128, &100i128),
        Err(Ok(sdk_err(PaymentError::ContractPaused)))
    );
}

#[test]
fn test_pay_invalid_amounts() {
    let s = Setup::new();
    assert_eq!(
        s.client
            .try_pay(&s.payer, &s.merchant_id, &s.usdc, &0i128, &1i128),
        Err(Ok(sdk_err(PaymentError::InvalidSendAmount)))
    );
    assert_eq!(
        s.client
            .try_pay(&s.payer, &s.merchant_id, &s.usdc, &1i128, &0i128),
        Err(Ok(sdk_err(PaymentError::InvalidMinReceive)))
    );
}

#[test]
fn test_direct_payment_no_fee() {
    let s = Setup::new();
    let net = s
        .client
        .pay(&s.payer, &s.merchant_id, &s.usdc, &1000i128, &1000i128);
    assert_eq!(net, 1000);
    // Tokens land at vault address
    assert_eq!(TokenClient::new(&s.env, &s.usdc).balance(&s.vault_id), 1000);
    // Vault ledger also updated
    assert_eq!(
        StubVaultClient::new(&s.env, &s.vault_id).balance_of(&s.payer),
        1000
    );
}

#[test]
fn test_direct_payment_with_fee() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let payer = Address::generate(&env);
    let fee_dest = Address::generate(&env);
    let usdc = create_token(&env, &admin);
    mint(&env, &usdc, &payer, 1_000_000);

    let vault_id = env.register_contract(None, StubVault);
    let merchant_id = Bytes::from_slice(&env, b"merch2");
    let registry = env.register_contract(None, StubRegistry);
    StubRegistryClient::new(&env, &registry).set_merchant(
        &merchant_id,
        &MerchantMetadata {
            settlement_asset: usdc.clone(),
            vault: vault_id.clone(),
            active: true,
            fx_router: None,
        },
    );

    let router_id = env.register_contract(None, PaymentRouter);
    let client = PaymentRouterClient::new(&env, &router_id);
    // 100 bps = 1%
    client.initialize(&admin, &registry, &100u32, &Some(fee_dest.clone()));

    let net = client.pay(&payer, &merchant_id, &usdc, &1000i128, &990i128);
    assert_eq!(net, 990);
    assert_eq!(TokenClient::new(&env, &usdc).balance(&vault_id), 990);
    assert_eq!(TokenClient::new(&env, &usdc).balance(&fee_dest), 10);
}

#[test]
fn test_inactive_merchant_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let payer = Address::generate(&env);
    let usdc = create_token(&env, &admin);
    mint(&env, &usdc, &payer, 1_000_000);

    let vault_id = env.register_contract(None, StubVault);
    let merchant_id = Bytes::from_slice(&env, b"inactive");
    let registry = env.register_contract(None, StubRegistry);
    StubRegistryClient::new(&env, &registry).set_merchant(
        &merchant_id,
        &MerchantMetadata {
            settlement_asset: usdc.clone(),
            vault: vault_id.clone(),
            active: false,
            fx_router: None,
        },
    );

    let router_id = env.register_contract(None, PaymentRouter);
    let client = PaymentRouterClient::new(&env, &router_id);
    client.initialize(&admin, &registry, &0u32, &None);

    assert_eq!(
        client.try_pay(&payer, &merchant_id, &usdc, &1000i128, &1000i128),
        Err(Ok(sdk_err(PaymentError::MerchantInactive)))
    );
}

#[test]
fn test_fx_payment() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let payer = Address::generate(&env);

    // send_asset = XLM-like token, settlement_asset = USDC
    let xlm = create_token(&env, &admin);
    let usdc = create_token(&env, &admin);
    mint(&env, &xlm, &payer, 1_000_000);

    // Pre-fund the FX router with USDC so it can pay out
    let fx_router_id = env.register_contract(None, StubFxRouter);
    mint(&env, &usdc, &fx_router_id, 1_000_000);

    let vault_id = env.register_contract(None, StubVault);
    let merchant_id = Bytes::from_slice(&env, b"fx_merch");
    let registry = env.register_contract(None, StubRegistry);
    StubRegistryClient::new(&env, &registry).set_merchant(
        &merchant_id,
        &MerchantMetadata {
            settlement_asset: usdc.clone(),
            vault: vault_id.clone(),
            active: true,
            fx_router: Some(fx_router_id.clone()),
        },
    );

    let router_id = env.register_contract(None, PaymentRouter);
    let client = PaymentRouterClient::new(&env, &router_id);
    client.initialize(&admin, &registry, &0u32, &None);

    // Pay 1000 XLM, expect at least 1000 USDC (1:1 stub)
    let net = client.pay(&payer, &merchant_id, &xlm, &1000i128, &1000i128);
    assert_eq!(net, 1000);
    assert_eq!(TokenClient::new(&env, &usdc).balance(&vault_id), 1000);
}

#[test]
fn test_fx_missing_router_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let payer = Address::generate(&env);
    let xlm = create_token(&env, &admin);
    let usdc = create_token(&env, &admin);
    mint(&env, &xlm, &payer, 1_000_000);

    let vault_id = env.register_contract(None, StubVault);
    let merchant_id = Bytes::from_slice(&env, b"no_fx");
    let registry = env.register_contract(None, StubRegistry);
    StubRegistryClient::new(&env, &registry).set_merchant(
        &merchant_id,
        &MerchantMetadata {
            settlement_asset: usdc.clone(),
            vault: vault_id.clone(),
            active: true,
            fx_router: None, // no FX router configured
        },
    );

    let router_id = env.register_contract(None, PaymentRouter);
    let client = PaymentRouterClient::new(&env, &router_id);
    client.initialize(&admin, &registry, &0u32, &None);

    assert_eq!(
        client.try_pay(&payer, &merchant_id, &xlm, &1000i128, &1000i128),
        Err(Ok(sdk_err(PaymentError::FxRouterMissing)))
    );
}

#[test]
fn test_set_fee_and_get_fee_dest() {
    let s = Setup::new();
    let fee_dest = Address::generate(&s.env);
    s.client.set_fee(&50u32, &Some(fee_dest.clone()));
    assert_eq!(s.client.get_fee_bps(), 50);
    assert_eq!(s.client.get_fee_dest(), Some(fee_dest));

    assert_eq!(
        s.client.try_set_fee(&1001u32, &None),
        Err(Ok(sdk_err(PaymentError::InvalidFeeBps)))
    );
}

#[test]
fn test_transfer_admin() {
    let s = Setup::new();
    let new_admin = Address::generate(&s.env);
    s.client.transfer_admin(&new_admin);
    assert_eq!(s.client.get_admin(), new_admin);
}

#[test]
fn test_version_starts_at_one() {
    let s = Setup::new();
    assert_eq!(s.client.get_version(), 1);
}
