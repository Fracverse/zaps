#![no_std]
#![allow(unexpected_cfgs)]
use soroban_sdk::{contract, contractimpl, contracttype, symbol_short, Address, Env, String, Symbol};

const ADMIN_KEY: Symbol = symbol_short!("admin");
const TREAS_KEY: Symbol = symbol_short!("treasury");

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Visibility {
    Public = 0,
    Friends = 1,
    Private = 2,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SocialPaymentEvent {
    pub sender: Address,
    pub receiver: Address,
    pub amount: i128,
    pub memo: String,
    pub visibility: Visibility,
}

#[contract]
pub struct SocialPaymentContract;

#[contractimpl]
impl SocialPaymentContract {
    pub fn initialize(env: Env, admin: Address, treasury: Address) {
        if env.storage().instance().has(&ADMIN_KEY) {
            panic!("already initialized");
        }
        env.storage().instance().set(&ADMIN_KEY, &admin);
        env.storage().instance().set(&TREAS_KEY, &treasury);
    }

    pub fn set_treasury(env: Env, new_treasury: Address) {
        let admin: Address = env.storage().instance().get(&ADMIN_KEY).expect("not initialized");
        admin.require_auth();
        env.storage().instance().set(&TREAS_KEY, &new_treasury);
    }

    /// SC-005: Execute a P2P social payment using the Naira token (or any SEP-41 token).
    /// For Public payments a 0.1% platform fee is routed to the treasury.
    /// For Friends/Private payments the full amount goes to the receiver.
    pub fn pay(
        env: Env,
        sender: Address,
        receiver: Address,
        token: Address,
        amount: i128,
        memo: String,
        visibility: Visibility,
    ) {
        sender.require_auth();
        assert!(amount > 0, "amount must be positive");

        let token_client = soroban_sdk::token::Client::new(&env, &token);

        if visibility == Visibility::Public {
            let treasury: Address = env.storage().instance().get(&TREAS_KEY).expect("treasury not initialized");
            let fee = amount / 1000; // 0.1%
            let receiver_amount = amount - fee;
            token_client.transfer(&sender, &receiver, &receiver_amount);
            if fee > 0 {
                token_client.transfer(&sender, &treasury, &fee);
            }
        } else {
            token_client.transfer(&sender, &receiver, &amount);
        }

        env.events().publish(
            (Symbol::new(&env, "SocialPaymentEvent"),),
            SocialPaymentEvent { sender, receiver, amount, memo, visibility },
        );
    }

    pub fn like_payment(env: Env, sender: Address, tx_id: Symbol) {
        sender.require_auth();
        env.events().publish((Symbol::new(&env, "PaymentLiked"),), (tx_id, sender));
    }

    pub fn comment_payment(env: Env, sender: Address, tx_id: Symbol, comment: String) {
        sender.require_auth();
        if comment.len() > 120 {
            panic!("comment exceeds maximum length of 120 characters");
        }
        env.events().publish((Symbol::new(&env, "PaymentCommented"),), (tx_id, comment));
    }
}

// ─── SC-015: Comprehensive unit test suite ───────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{
        testutils::{Address as _, Events},
        Address, Env, IntoVal, String, Symbol, TryIntoVal, Val,
    };

    fn setup() -> (Env, SocialPaymentContractClient<'static>, Address, Address, Address, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, SocialPaymentContract);
        let client = SocialPaymentContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);
        let sender = Address::generate(&env);
        let receiver = Address::generate(&env);
        client.initialize(&admin, &treasury);
        (env, client, admin, treasury, sender, receiver)
    }

    fn mint_token(env: &Env, admin: &Address, sender: &Address, amount: i128) -> Address {
        let token_address = env.register_stellar_asset_contract(admin.clone());
        let token_admin = soroban_sdk::token::StellarAssetClient::new(env, &token_address);
        token_admin.mint(sender, &amount);
        token_address
    }

    // ── Public payment: fee deducted, event emitted ──────────────────────────
    #[test]
    fn test_social_payment_public_visibility_deducts_fee() {
        let (env, client, admin, treasury, sender, receiver) = setup();
        let token = mint_token(&env, &admin, &sender, 10_000);
        let token_client = soroban_sdk::token::Client::new(&env, &token);

        client.pay(&sender, &receiver, &token, &1000, &String::from_str(&env, "Public payment"), &Visibility::Public);

        assert_eq!(token_client.balance(&receiver), 999);
        assert_eq!(token_client.balance(&treasury), 1);
        assert_eq!(token_client.balance(&sender), 9_000);

        let events = env.events().all();
        let topic: Val = Symbol::new(&env, "SocialPaymentEvent").into_val(&env);
        let mut found = false;
        for item in events.iter() {
            if item.1.contains(topic.clone()) {
                let ev: SocialPaymentEvent = item.2.try_into_val(&env).unwrap();
                assert_eq!(ev.sender, sender);
                assert_eq!(ev.receiver, receiver);
                assert_eq!(ev.amount, 1000);
                assert_eq!(ev.visibility, Visibility::Public);
                found = true;
            }
        }
        assert!(found, "SocialPaymentEvent not emitted");
    }

    // ── Private payment: no fee, full amount ─────────────────────────────────
    #[test]
    fn test_social_payment_private_visibility_no_fee() {
        let (env, client, admin, _treasury, sender, receiver) = setup();
        let token = mint_token(&env, &admin, &sender, 10_000);
        let token_client = soroban_sdk::token::Client::new(&env, &token);

        client.pay(&sender, &receiver, &token, &1000, &String::from_str(&env, "Private"), &Visibility::Private);

        assert_eq!(token_client.balance(&receiver), 1000);
        assert_eq!(token_client.balance(&sender), 9_000);
    }

    // ── Friends-only payment: no fee ─────────────────────────────────────────
    #[test]
    fn test_social_payment_friends_visibility_no_fee() {
        let (env, client, admin, treasury, sender, receiver) = setup();
        let token = mint_token(&env, &admin, &sender, 5_000);
        let token_client = soroban_sdk::token::Client::new(&env, &token);

        client.pay(&sender, &receiver, &token, &500, &String::from_str(&env, "Friends"), &Visibility::Friends);

        assert_eq!(token_client.balance(&receiver), 500);
        assert_eq!(token_client.balance(&treasury), 0);
        assert_eq!(token_client.balance(&sender), 4_500);
    }

    // ── Pay panics on zero amount ─────────────────────────────────────────────
    #[test]
    #[should_panic(expected = "amount must be positive")]
    fn test_pay_rejects_zero_amount() {
        let (env, client, admin, _treasury, sender, receiver) = setup();
        let token = mint_token(&env, &admin, &sender, 1_000);
        client.pay(&sender, &receiver, &token, &0, &String::from_str(&env, "bad"), &Visibility::Private);
    }

    // ── Like event ───────────────────────────────────────────────────────────
    #[test]
    fn test_like_payment_emits_event() {
        let (env, client, _admin, _treasury, sender, _receiver) = setup();
        let tx_id = Symbol::new(&env, "tx123");
        client.like_payment(&sender, &tx_id);

        let events = env.events().all();
        let topic: Val = Symbol::new(&env, "PaymentLiked").into_val(&env);
        let mut found = false;
        for item in events.iter() {
            if item.1.contains(topic.clone()) {
                let (eid, eaddr): (Symbol, Address) = item.2.try_into_val(&env).unwrap();
                assert_eq!(eid, tx_id);
                assert_eq!(eaddr, sender);
                found = true;
            }
        }
        assert!(found, "PaymentLiked event not emitted");
    }

    // ── Comment: valid ───────────────────────────────────────────────────────
    #[test]
    fn comment_payment_accepts_valid_comment() {
        let (env, client, _admin, _treasury, sender, _receiver) = setup();
        let tx_id = Symbol::new(&env, "tx456");
        client.comment_payment(&sender, &tx_id, &String::from_str(&env, "Nice one!"));
    }

    // ── Comment: too long ────────────────────────────────────────────────────
    #[test]
    #[should_panic(expected = "comment exceeds maximum length")]
    fn comment_payment_rejects_overlong_comment() {
        let (env, client, _admin, _treasury, sender, _receiver) = setup();
        let tx_id = Symbol::new(&env, "tx789");
        let long = "x".repeat(121);
        client.comment_payment(&sender, &tx_id, &String::from_str(&env, &long));
    }

    // ── Double-initialize panics ─────────────────────────────────────────────
    #[test]
    #[should_panic(expected = "already initialized")]
    fn test_initialize_twice_panics() {
        let (env, client, admin, treasury, _sender, _receiver) = setup();
        client.initialize(&admin, &treasury);
    }
}
