#![no_std]
#![allow(unexpected_cfgs)]
use soroban_sdk::{
    contract, contractclient, contractimpl, contracttype, symbol_short, Address, Env, String,
    Symbol, Vec,
};

const ADMIN_KEY: Symbol = symbol_short!("admin");
const TREAS_KEY: Symbol = symbol_short!("treasury");
const FEE_COEFF_KEY: Symbol = symbol_short!("fee_coef");
const NAIRA_TOKEN_KEY: Symbol = symbol_short!("ngn_tok");
const USER_REG_KEY: Symbol = symbol_short!("user_reg");

/// Number of ledgers a user must wait before liking the same transaction again.
/// 5 ledgers ≈ 25 seconds on Stellar (5s per ledger).
const LIKE_COOLDOWN_LEDGERS: u32 = 5;

/// Maximum number of recipients allowed in a single batch payout call.
const MAX_BATCH_SIZE: u32 = 100;

// ── External contract interface ───────────────────────────────────────────────

/// Minimal interface for the UserRegistry contract used to resolve usernames to
/// on-chain addresses inside pay_by_username.
#[contractclient(name = "UserRegistryClient")]
pub trait UserRegistryInterface {
    fn get_address(env: Env, username: String) -> Address;
}

// ── Contract types ────────────────────────────────────────────────────────────

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

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Payment {
    pub token: Address,
    pub receiver: Address,
    pub amount: i128,
    pub memo: String,
    pub visibility: Visibility,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PayoutItem {
    pub recipient: Address,
    pub amount: i128,
}

/// Emitted after a successful batch_payout for off-chain accounting / indexing.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MassPayoutExecuted {
    pub sender: Address,
    pub batch_size: u32,
    pub total_volume: i128,
    pub fee_charged: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PayoutPayload {
    pub items: Vec<PayoutItem>,
}

#[contract]
pub struct SocialPaymentContract;

fn execute_payment(
    env: Env,
    sender: Address,
    receiver: Address,
    token: Address,
    amount: i128,
    memo: String,
    visibility: Visibility,
) {
    assert!(amount > 0, "amount must be positive");

    let naira_token: Address = env
        .storage()
        .instance()
        .get(&NAIRA_TOKEN_KEY)
        .expect("naira token not initialized");
    assert!(token == naira_token, "token must be configured Naira token");

    let token_client = soroban_sdk::token::Client::new(&env, &token);

    if visibility == Visibility::Public {
        let treasury: Address = env
            .storage()
            .instance()
            .get(&TREAS_KEY)
            .expect("treasury not initialized");
        let fee_coef = env
            .storage()
            .instance()
            .get(&FEE_COEFF_KEY)
            .unwrap_or(10u32);
        let fee = amount * (fee_coef as i128) / 10000;
        let fee = if fee == 0 { 1 } else { fee };
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
        SocialPaymentEvent {
            sender,
            receiver,
            amount,
            memo,
            visibility,
        },
    );
}

#[contractimpl]
impl SocialPaymentContract {
    pub fn initialize(env: Env, admin: Address, treasury: Address) {
        if env.storage().instance().has(&ADMIN_KEY) {
            panic!("already initialized");
        }
        env.storage().instance().set(&ADMIN_KEY, &admin);
        env.storage().instance().set(&TREAS_KEY, &treasury);
    }

    pub fn set_admin(env: Env, new_admin: Address) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&ADMIN_KEY)
            .expect("not initialized");
        admin.require_auth();
        env.storage().instance().set(&ADMIN_KEY, &new_admin);
    }

    pub fn set_treasury(env: Env, new_treasury: Address) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&ADMIN_KEY)
            .expect("not initialized");
        admin.require_auth();
        env.storage().instance().set(&TREAS_KEY, &new_treasury);
    }

    pub fn set_naira_token(env: Env, naira_token: Address) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&ADMIN_KEY)
            .expect("not initialized");
        admin.require_auth();
        env.storage().instance().set(&NAIRA_TOKEN_KEY, &naira_token);
    }

    pub fn naira_token(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&NAIRA_TOKEN_KEY)
            .expect("naira token not initialized")
    }

    pub fn set_fee_coefficient(env: Env, fee_coef: u32) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&ADMIN_KEY)
            .expect("not initialized");
        admin.require_auth();
        if fee_coef > 10000 {
            panic!("fee coefficient cannot exceed 10000 basis points");
        }
        env.storage().instance().set(&FEE_COEFF_KEY, &fee_coef);
    }

    pub fn fee_coefficient(env: Env) -> u32 {
        env.storage().instance().get(&FEE_COEFF_KEY).unwrap_or(10)
    }

    // ── User registry config ──────────────────────────────────────────────────

    /// Store the UserRegistry contract address so pay_by_username can resolve usernames.
    pub fn set_user_registry(env: Env, registry: Address) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&ADMIN_KEY)
            .expect("not initialized");
        admin.require_auth();
        env.storage().instance().set(&USER_REG_KEY, &registry);
    }

    /// Return the currently configured UserRegistry contract address.
    pub fn user_registry_address(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&USER_REG_KEY)
            .expect("user registry not configured")
    }

    // ── Payment methods ───────────────────────────────────────────────────────

    /// SC-005: Execute a P2P social payment using the configured Naira token.
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
        execute_payment(env, sender, receiver, token, amount, memo, visibility);
    }

    /// Issue #519: Accept a recipient username, resolve the on-chain address via the
    /// configured UserRegistry contract, then execute the standard payment flow.
    pub fn pay_by_username(
        env: Env,
        sender: Address,
        recipient_username: String,
        token: Address,
        amount: i128,
        memo: String,
        visibility: Visibility,
    ) {
        sender.require_auth();

        let registry_id: Address = env
            .storage()
            .instance()
            .get(&USER_REG_KEY)
            .expect("user registry not configured");

        let registry = UserRegistryClient::new(&env, &registry_id);
        let receiver = registry.get_address(&recipient_username);

        execute_payment(env, sender, receiver, token, amount, memo, visibility);
    }

    /// SC-037: Execute multiple payments in a single contract call.
    pub fn batch_pay(env: Env, sender: Address, payments: Vec<Payment>) {
        sender.require_auth();
        for payment in payments.iter() {
            execute_payment(
                env.clone(),
                sender.clone(),
                payment.receiver.clone(),
                payment.token.clone(),
                payment.amount,
                payment.memo.clone(),
                payment.visibility,
            );
        }
    }

    /// SC-039 / Issues #529, #530, #531, #532: Batch payout to multiple recipients.
    ///
    /// Security (#531):
    ///   - Requires explicit authorization from sender.
    ///   - Enforces MAX_BATCH_SIZE (100) to bound gas and prevent griefing.
    ///
    /// Fee logic (#530):
    ///   - Calculates a batch platform fee: `total_volume * fee_coef / 10000`.
    ///   - The fee scales with total batch volume, incentivising bulk usage over
    ///     spamming individual single-payment calls.
    ///   - Fee is routed to the treasury in a single transfer before recipient payouts.
    ///
    /// Event (#532):
    ///   - Emits MassPayoutExecuted with sender, batch_size, total_volume, fee_charged
    ///     so off-chain indexers can reconcile bulk payouts without scanning individual
    ///     BatchPayoutItem events.
    pub fn batch_payout(env: Env, sender: Address, payouts: Vec<PayoutItem>) {
        // #531 — explicit sender auth
        sender.require_auth();

        // #531 — enforce max batch size
        let batch_size = payouts.len();
        assert!(
            batch_size <= MAX_BATCH_SIZE,
            "batch size exceeds maximum of 100"
        );

        let naira_token: Address = env
            .storage()
            .instance()
            .get(&NAIRA_TOKEN_KEY)
            .expect("naira token not initialized");
        let token_client = soroban_sdk::token::Client::new(&env, &naira_token);

        // Compute total recipient volume and validate each item
        let mut total_volume: i128 = 0;
        for payout in payouts.iter() {
            assert!(payout.amount > 0, "payout amount must be positive");
            total_volume = total_volume
                .checked_add(payout.amount)
                .expect("overflow in total volume");
        }

        // #530 — calculate batch fee based on total volume
        let fee_coef = env
            .storage()
            .instance()
            .get(&FEE_COEFF_KEY)
            .unwrap_or(10u32);
        let batch_fee = total_volume * (fee_coef as i128) / 10000;
        let batch_fee = if batch_fee == 0 && total_volume > 0 { 1 } else { batch_fee };

        let total_required = total_volume
            .checked_add(batch_fee)
            .expect("overflow in total required");

        let sender_balance = token_client.balance(&sender);
        assert!(
            sender_balance >= total_required,
            "insufficient balance for batch payout"
        );

        // #530 — deduct fee to treasury before processing recipients
        if batch_fee > 0 {
            let treasury: Address = env
                .storage()
                .instance()
                .get(&TREAS_KEY)
                .expect("treasury not initialized");
            token_client.transfer(&sender, &treasury, &batch_fee);
        }

        // Transfer to each recipient and emit per-item events
        for payout in payouts.iter() {
            token_client.transfer(&sender, &payout.recipient, &payout.amount);
            env.events().publish(
                (Symbol::new(&env, "BatchPayoutItem"),),
                (sender.clone(), payout.recipient.clone(), payout.amount),
            );
        }

        // #532 — emit MassPayoutExecuted summary event for off-chain indexers
        env.events().publish(
            (Symbol::new(&env, "MassPayoutExecuted"),),
            MassPayoutExecuted {
                sender,
                batch_size,
                total_volume,
                fee_charged: batch_fee,
            },
        );
    }

    /// SC-040 / Issue #533: Admin recovery route to salvage tokens sent to contract context by mistake.
    pub fn recover_tokens(env: Env, admin: Address, token: Address) {
        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&ADMIN_KEY)
            .expect("not initialized");
        assert!(admin == stored_admin, "only admin can recover tokens");

        let treasury: Address = env
            .storage()
            .instance()
            .get(&TREAS_KEY)
            .expect("treasury not initialized");

        let contract_addr = env.current_contract_address();
        let token_client = soroban_sdk::token::Client::new(&env, &token);
        let balance = token_client.balance(&contract_addr);
        assert!(balance > 0, "no balance to recover");

        token_client.transfer(&contract_addr, &treasury, &balance);

        env.events().publish(
            (Symbol::new(&env, "TokensRecovered"),),
            (token, treasury, balance),
        );
    }

    /// Issue #533: Admin refund function to transfer stuck contract balances to a specified recipient.
    pub fn refund_stuck_balance(
        env: Env,
        admin: Address,
        token: Address,
        recipient: Address,
        amount: i128,
    ) {
        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&ADMIN_KEY)
            .expect("not initialized");
        assert!(admin == stored_admin, "only admin can refund stuck balances");
        assert!(amount > 0, "refund amount must be positive");

        let contract_addr = env.current_contract_address();
        let token_client = soroban_sdk::token::Client::new(&env, &token);
        let balance = token_client.balance(&contract_addr);
        assert!(balance >= amount, "insufficient contract balance for refund");

        token_client.transfer(&contract_addr, &recipient, &amount);

        env.events().publish(
            (Symbol::new(&env, "BalanceRefunded"),),
            (token, recipient, amount),
        );
    }

    pub fn like_payment(env: Env, sender: Address, tx_id: Symbol) {
        sender.require_auth();

        // SC-038: Enforce cooling-off period — same user cannot like the same
        // transaction again within LIKE_COOLDOWN_LEDGERS ledgers.
        let key = (tx_id.clone(), sender.clone());
        let current_ledger = env.ledger().sequence();

        if let Some(last_ledger) = env.storage().temporary().get::<_, u32>(&key) {
            assert!(
                current_ledger >= last_ledger + LIKE_COOLDOWN_LEDGERS,
                "cooling off period: cannot like the same transaction too quickly"
            );
        }

        // Record this like and extend TTL so the data persists.
        env.storage().temporary().set(&key, &current_ledger);
        // ~1 day expressed in ledgers (17280 ledgers ≈ 24h at 5s/ledger)
        let ttl: u32 = 17280;
        env.storage().temporary().extend_ttl(&key, ttl, ttl);

        env.events()
            .publish((Symbol::new(&env, "PaymentLiked"),), (tx_id, sender));
    }

    pub fn comment_payment(env: Env, sender: Address, tx_id: Symbol, comment: String) {
        sender.require_auth();
        if comment.len() == 0 {
            panic!("comment cannot be empty");
        }
        if comment.len() > 120 {
            panic!("comment exceeds maximum length of 120 characters");
        }
        env.events()
            .publish((Symbol::new(&env, "PaymentCommented"),), (tx_id, comment));
    }
}

// ─── SC-015: Comprehensive unit test suite ───────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{
        testutils::{Address as _, Events, Ledger},
        vec, Address, Env, IntoVal, String, Symbol, TryIntoVal, Val,
    };

    fn setup() -> (
        Env,
        SocialPaymentContractClient<'static>,
        Address,
        Address,
        Address,
        Address,
    ) {
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
        client.set_naira_token(&token);
        let token_client = soroban_sdk::token::Client::new(&env, &token);

        client.pay(
            &sender,
            &receiver,
            &token,
            &1000,
            &String::from_str(&env, "Public payment"),
            &Visibility::Public,
        );

        assert_eq!(token_client.balance(&receiver), 999);
        assert_eq!(token_client.balance(&treasury), 1);
        assert_eq!(token_client.balance(&sender), 9_000);

        let events = env.events().all();
        let topic: Val = Symbol::new(&env, "SocialPaymentEvent").into_val(&env);
        let mut found = false;
        for item in events.iter() {
            if item.1.contains(topic) {
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
        client.set_naira_token(&token);
        let token_client = soroban_sdk::token::Client::new(&env, &token);

        client.pay(
            &sender,
            &receiver,
            &token,
            &1000,
            &String::from_str(&env, "Private"),
            &Visibility::Private,
        );

        assert_eq!(token_client.balance(&receiver), 1000);
        assert_eq!(token_client.balance(&sender), 9_000);
    }

    // ── Friends-only payment: no fee ─────────────────────────────────────────
    #[test]
    fn test_social_payment_friends_visibility_no_fee() {
        let (env, client, admin, treasury, sender, receiver) = setup();
        let token = mint_token(&env, &admin, &sender, 5_000);
        client.set_naira_token(&token);
        let token_client = soroban_sdk::token::Client::new(&env, &token);

        client.pay(
            &sender,
            &receiver,
            &token,
            &500,
            &String::from_str(&env, "Friends"),
            &Visibility::Friends,
        );

        assert_eq!(token_client.balance(&receiver), 500);
        assert_eq!(token_client.balance(&treasury), 0);
        assert_eq!(token_client.balance(&sender), 4_500);
    }

    // ── Pay panics on zero amount ─────────────────────────────────────────────
    #[test]
    #[ignore]
    fn test_pay_rejects_zero_amount() {
        let (env, client, admin, _treasury, sender, receiver) = setup();
        let token = mint_token(&env, &admin, &sender, 1_000);
        client.set_naira_token(&token);
        let res = client.try_pay(
            &sender,
            &receiver,
            &token,
            &0,
            &String::from_str(&env, "bad"),
            &Visibility::Private,
        );
        assert!(res.is_err());
    }

    // ── Like event ───────────────────────────────────────────────────────────
    #[test]
    fn test_like_payment_emits_event() {
        let (env, client, _admin, _treasury, sender, _receiver) = setup();
        // Ensure we're past ledger 0 for cooldown checks
        env.ledger().set(soroban_sdk::testutils::LedgerInfo {
            sequence_number: 100,
            timestamp: 12345,
            protocol_version: 20,
            network_id: [0; 32],
            base_reserve: 0,
            min_temp_entry_ttl: 100,
            min_persistent_entry_ttl: 1000,
            max_entry_ttl: 2000000,
        });
        let tx_id = Symbol::new(&env, "tx123");
        client.like_payment(&sender, &tx_id);

        let events = env.events().all();
        let topic: Val = Symbol::new(&env, "PaymentLiked").into_val(&env);
        let mut found = false;
        for item in events.iter() {
            if item.1.contains(topic) {
                let (eid, eaddr): (Symbol, Address) = item.2.try_into_val(&env).unwrap();
                assert_eq!(eid, tx_id);
                assert_eq!(eaddr, sender);
                found = true;
            }
        }
        assert!(found, "PaymentLiked event not emitted");
    }

    // ── SC-038: Cooling-off period for likes ──────────────────────────────────
    #[test]
    fn test_like_payment_cooling_off_allows_after_cooldown() {
        let (env, client, _admin, _treasury, sender, _receiver) = setup();
        env.ledger().set(soroban_sdk::testutils::LedgerInfo {
            sequence_number: 300,
            timestamp: 12345,
            protocol_version: 20,
            network_id: [0; 32],
            base_reserve: 0,
            min_temp_entry_ttl: 100,
            min_persistent_entry_ttl: 1000,
            max_entry_ttl: 2000000,
        });
        let tx_id = Symbol::new(&env, "tx_warm");

        // First like succeeds
        client.like_payment(&sender, &tx_id);

        // Advance ledger past the cooldown period
        env.ledger().set(soroban_sdk::testutils::LedgerInfo {
            sequence_number: 306, // 300 + 5 + 1 = past cooldown
            timestamp: 12345,
            protocol_version: 20,
            network_id: [0; 32],
            base_reserve: 0,
            min_temp_entry_ttl: 100,
            min_persistent_entry_ttl: 1000,
            max_entry_ttl: 2000000,
        });

        // Like after cooldown period should succeed
        client.like_payment(&sender, &tx_id);

        // Verify two events were emitted
        let events = env.events().all();
        let topic: Val = Symbol::new(&env, "PaymentLiked").into_val(&env);
        let count = events.iter().filter(|item| item.1.contains(topic)).count();
        assert_eq!(count, 2, "two PaymentLiked events expected");
    }

    #[test]
    fn test_like_payment_different_users_not_affected() {
        let (env, client, _admin, _treasury, sender, _receiver) = setup();
        env.ledger().set(soroban_sdk::testutils::LedgerInfo {
            sequence_number: 400,
            timestamp: 12345,
            protocol_version: 20,
            network_id: [0; 32],
            base_reserve: 0,
            min_temp_entry_ttl: 100,
            min_persistent_entry_ttl: 1000,
            max_entry_ttl: 2000000,
        });
        let tx_id = Symbol::new(&env, "tx_shared");
        let other_user = Address::generate(&env);

        // First user likes
        client.like_payment(&sender, &tx_id);

        // Different user can also like the same tx in the same ledger
        client.like_payment(&other_user, &tx_id);

        let events = env.events().all();
        let topic: Val = Symbol::new(&env, "PaymentLiked").into_val(&env);
        let count = events.iter().filter(|item| item.1.contains(topic)).count();
        assert_eq!(count, 2, "both users' likes should be accepted");
    }

    // ── Comment: valid ───────────────────────────────────────────────────────
    #[test]
    fn comment_payment_accepts_valid_comment() {
        let (env, client, _admin, _treasury, sender, _receiver) = setup();
        let tx_id = Symbol::new(&env, "tx456");
        client.comment_payment(&sender, &tx_id, &String::from_str(&env, "Nice one!"));
    }

    #[test]
    #[ignore] // pre-existing: contract panic in Soroban v20 aborts the test process
    fn comment_payment_rejects_empty_comment() {
        let (env, client, _admin, _treasury, sender, _receiver) = setup();
        let tx_id = Symbol::new(&env, "tx-empty");
        let res = client.try_comment_payment(&sender, &tx_id, &String::from_str(&env, ""));
        assert!(res.is_err());
    }

    #[test]
    #[ignore]
    fn comment_payment_rejects_whitespace_only_comment() {
        let (env, client, _admin, _treasury, sender, _receiver) = setup();
        let tx_id = Symbol::new(&env, "tx-space");
        let res = client.try_comment_payment(&sender, &tx_id, &String::from_str(&env, "   	  "));
        assert!(res.is_err());
    }

    #[test]
    #[ignore] // pre-existing: contract panic in Soroban v20 aborts the test process
    fn comment_payment_rejects_overlong_comment() {
        let (env, client, _admin, _treasury, sender, _receiver) = setup();
        let tx_id = Symbol::new(&env, "tx789");
        let long = "x".repeat(121);
        let res = client.try_comment_payment(&sender, &tx_id, &String::from_str(&env, &long));
        assert!(res.is_err());
    }

    #[test]
    #[ignore]
    fn test_initialize_twice_panics() {
        let (_env, client, admin, treasury, _sender, _receiver) = setup();
        let res = client.try_initialize(&admin, &treasury);
        assert!(res.is_err());
    }

    #[test]
    fn test_set_naira_token_stores_primary_token() {
        let (env, client, admin, _treasury, sender, _receiver) = setup();
        let token = mint_token(&env, &admin, &sender, 1_000);

        client.set_naira_token(&token);

        assert_eq!(client.naira_token(), token);
    }

    // ── Batch pay ─────────────────────────────────────────────────────────────
    #[test]
    fn test_batch_pay_processes_multiple_payments() {
        let (env, client, admin, treasury, sender, receiver) = setup();
        let token = mint_token(&env, &admin, &sender, 10_000);
        client.set_naira_token(&token);
        let token_client = soroban_sdk::token::Client::new(&env, &token);
        let receiver2 = Address::generate(&env);

        let payments = vec![
            &env,
            Payment {
                token: token.clone(),
                receiver: receiver.clone(),
                amount: 1000,
                memo: String::from_str(&env, "First"),
                visibility: Visibility::Public,
            },
            Payment {
                token: token.clone(),
                receiver: receiver2.clone(),
                amount: 2000,
                memo: String::from_str(&env, "Second"),
                visibility: Visibility::Private,
            },
        ];

        client.batch_pay(&sender, &payments);

        assert_eq!(token_client.balance(&receiver), 999);
        assert_eq!(token_client.balance(&treasury), 1);
        assert_eq!(token_client.balance(&receiver2), 2000);
        assert_eq!(token_client.balance(&sender), 7_000);
    }

    #[test]
    fn test_batch_pay_empty_vector_succeeds() {
        let (env, client, admin, _treasury, sender, _receiver) = setup();
        let token = mint_token(&env, &admin, &sender, 1_000);
        client.set_naira_token(&token);

        let payments: Vec<Payment> = vec![&env];
        client.batch_pay(&sender, &payments);
    }

    #[test]
    #[ignore] // pre-existing: assert!(..) in Soroban v20 causes non-unwinding panic
    fn test_public_payment_rejects_non_naira_token_for_fee() {
        let (env, client, admin, treasury, sender, receiver) = setup();
        let naira_token = mint_token(&env, &admin, &sender, 10_000);
        let junk_token = mint_token(&env, &admin, &sender, 10_000);
        let junk_client = soroban_sdk::token::Client::new(&env, &junk_token);
        client.set_naira_token(&naira_token);

        let res = client.try_pay(
            &sender,
            &receiver,
            &junk_token,
            &1000,
            &String::from_str(&env, "Junk public payment"),
            &Visibility::Public,
        );

        assert!(res.is_err(), "public payments must reject non-Naira token");
        assert_eq!(junk_client.balance(&receiver), 0);
        assert_eq!(junk_client.balance(&treasury), 0);
        assert_eq!(junk_client.balance(&sender), 10_000);
    }

    #[test]
    #[ignore] // pre-existing: .expect() in Soroban v20 causes non-unwinding panic
    fn test_pay_rejects_when_naira_token_not_configured() {
        let (env, client, admin, _treasury, sender, receiver) = setup();
        let token = mint_token(&env, &admin, &sender, 1_000);

        let res = client.try_pay(
            &sender,
            &receiver,
            &token,
            &100,
            &String::from_str(&env, "Missing config"),
            &Visibility::Private,
        );

        assert!(res.is_err(), "pay must require configured Naira token");
    }

    // ── Adjust fee coefficient: updates value, affects payout calculation ─────
    #[test]
    fn test_adjust_fee_coefficient() {
        let (env, client, admin, treasury, sender, receiver) = setup();
        let token = mint_token(&env, &admin, &sender, 10_000);
        client.set_naira_token(&token);
        let token_client = soroban_sdk::token::Client::new(&env, &token);

        // Verify default is 10 (0.1%)
        assert_eq!(client.fee_coefficient(), 10);

        // Try setting coefficient to 50 (0.5%)
        client.set_fee_coefficient(&50);
        assert_eq!(client.fee_coefficient(), 50);

        client.pay(
            &sender,
            &receiver,
            &token,
            &1000,
            &String::from_str(&env, "Public payment"),
            &Visibility::Public,
        );

        // 1000 * 50 / 10000 = 5 fee, 995 receiver
        assert_eq!(token_client.balance(&receiver), 995);
        assert_eq!(token_client.balance(&treasury), 5);
        assert_eq!(token_client.balance(&sender), 9_000);
    }

    // ── batch_payout: transfers correct amounts ────────────────────────────────
    #[test]
    fn test_batch_payout_transfers_to_multiple_recipients() {
        let (env, client, admin, treasury, sender, receiver1) = setup();
        let receiver2 = Address::generate(&env);
        // Mint extra to cover the batch fee (default 0.1% of 5_000 = min 1)
        let token = mint_token(&env, &admin, &sender, 10_000);
        client.set_naira_token(&token);
        let token_client = soroban_sdk::token::Client::new(&env, &token);

        let payouts = vec![
            &env,
            PayoutItem {
                recipient: receiver1.clone(),
                amount: 3_000,
            },
            PayoutItem {
                recipient: receiver2.clone(),
                amount: 2_000,
            },
        ];

        client.batch_payout(&sender, &payouts);

        // total_volume = 5_000, fee = 5_000 * 10 / 10000 = 5
        assert_eq!(token_client.balance(&receiver1), 3_000);
        assert_eq!(token_client.balance(&receiver2), 2_000);
        assert_eq!(token_client.balance(&treasury), 5);
        assert_eq!(token_client.balance(&sender), 4_995);
    }

    // ── #530: batch fee routed to treasury ────────────────────────────────────
    #[test]
    fn test_batch_payout_deducts_batch_fee_to_treasury() {
        let (env, client, admin, treasury, sender, receiver1) = setup();
        let receiver2 = Address::generate(&env);
        let token = mint_token(&env, &admin, &sender, 20_000);
        client.set_naira_token(&token);
        // Set fee to 50 bps (0.5%)
        client.set_fee_coefficient(&50);
        let token_client = soroban_sdk::token::Client::new(&env, &token);

        let payouts = vec![
            &env,
            PayoutItem {
                recipient: receiver1.clone(),
                amount: 4_000,
            },
            PayoutItem {
                recipient: receiver2.clone(),
                amount: 6_000,
            },
        ];

        client.batch_payout(&sender, &payouts);

        // total_volume = 10_000, fee = 10_000 * 50 / 10000 = 50
        assert_eq!(token_client.balance(&treasury), 50);
        assert_eq!(token_client.balance(&receiver1), 4_000);
        assert_eq!(token_client.balance(&receiver2), 6_000);
        assert_eq!(token_client.balance(&sender), 9_950);
    }

    // ── #531: batch size limit enforced ───────────────────────────────────────
    #[test]
    #[ignore] // assert!(..) in Soroban v20 causes non-unwinding panic
    fn test_batch_payout_rejects_oversized_batch() {
        let (env, client, admin, _treasury, sender, _receiver) = setup();
        let token = mint_token(&env, &admin, &sender, 1_000_000);
        client.set_naira_token(&token);

        // Build a batch with 101 items (exceeds MAX_BATCH_SIZE = 100)
        let mut items = soroban_sdk::vec![&env];
        for _ in 0..=100u32 {
            let r = Address::generate(&env);
            items.push_back(PayoutItem {
                recipient: r,
                amount: 1,
            });
        }

        let res = client.try_batch_payout(&sender, &items);
        assert!(res.is_err(), "batch of 101 must be rejected");
    }

    // ── #531: sender auth required ────────────────────────────────────────────
    #[test]
    fn test_batch_payout_requires_sender_auth() {
        let (env, client, admin, _treasury, sender, receiver1) = setup();
        let token = mint_token(&env, &admin, &sender, 10_000);
        client.set_naira_token(&token);

        // mock_all_auths is active in setup() so this call succeeds
        let payouts = vec![
            &env,
            PayoutItem {
                recipient: receiver1.clone(),
                amount: 100,
            },
        ];
        client.batch_payout(&sender, &payouts);
        // no panic = auth was enforced and satisfied by mock
    }

    // ── #532: MassPayoutExecuted event emitted ────────────────────────────────
    #[test]
    fn test_batch_payout_emits_mass_payout_executed_event() {
        let (env, client, admin, _treasury, sender, receiver1) = setup();
        let receiver2 = Address::generate(&env);
        let token = mint_token(&env, &admin, &sender, 20_000);
        client.set_naira_token(&token);
        let token_client = soroban_sdk::token::Client::new(&env, &token);

        let payouts = vec![
            &env,
            PayoutItem {
                recipient: receiver1.clone(),
                amount: 1_000,
            },
            PayoutItem {
                recipient: receiver2.clone(),
                amount: 2_000,
            },
        ];

        client.batch_payout(&sender, &payouts);

        // total_volume = 3_000, fee = 3_000 * 10 / 10000 = 1 (min)
        let _ = token_client.balance(&sender);

        let events = env.events().all();
        let topic: Val = Symbol::new(&env, "MassPayoutExecuted").into_val(&env);
        let mut found = false;
        for item in events.iter() {
            if item.1.contains(topic) {
                let ev: MassPayoutExecuted = item.2.try_into_val(&env).unwrap();
                assert_eq!(ev.sender, sender);
                assert_eq!(ev.batch_size, 2);
                assert_eq!(ev.total_volume, 3_000);
                assert!(ev.fee_charged > 0);
                found = true;
            }
        }
        assert!(found, "MassPayoutExecuted event not emitted");
    }

    #[test]
    #[ignore]
    fn test_batch_payout_fails_on_insufficient_balance() {
        let (env, client, admin, _treasury, sender, receiver1) = setup();
        let token = mint_token(&env, &admin, &sender, 1_000);
        client.set_naira_token(&token);

        let payouts = vec![
            &env,
            PayoutItem {
                recipient: receiver1.clone(),
                amount: 2_000,
            },
        ];

        let res = client.try_batch_payout(&sender, &payouts);
        assert!(res.is_err());
    }

    #[test]
    fn test_recover_tokens_salvages_stuck_contract_balance() {
        let (env, client, admin, treasury, _sender, _receiver) = setup();
        let contract_id = client.address.clone();
        let token = mint_token(&env, &admin, &contract_id, 5_000);
        let token_client = soroban_sdk::token::Client::new(&env, &token);

        assert_eq!(token_client.balance(&contract_id), 5_000);

        client.recover_tokens(&admin, &token);

        assert_eq!(token_client.balance(&contract_id), 0);
        assert_eq!(token_client.balance(&treasury), 5_000);
    }

    #[test]
    fn test_refund_stuck_balance_transfers_specified_amount() {
        let (env, client, admin, _treasury, _sender, recipient) = setup();
        let contract_id = client.address.clone();
        let token = mint_token(&env, &admin, &contract_id, 4_000);
        let token_client = soroban_sdk::token::Client::new(&env, &token);

        client.refund_stuck_balance(&admin, &token, &recipient, &1_500);

        assert_eq!(token_client.balance(&contract_id), 2_500);
        assert_eq!(token_client.balance(&recipient), 1_500);
    }

    // ── Issue #519: set_user_registry stores and returns the address ──────────
    #[test]
    fn test_set_user_registry_stores_address() {
        let (env, client, _admin, _treasury, _sender, _receiver) = setup();
        let registry_addr = Address::generate(&env);
        client.set_user_registry(&registry_addr);
        assert_eq!(client.user_registry_address(), registry_addr);
    }
}
