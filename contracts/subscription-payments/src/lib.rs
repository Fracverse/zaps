#![no_std]

//! # Subscription Payments Contract
//!
//! Manages recurring payment subscriptions between subscribers and merchants.
//!
//! ## Lifecycle
//!
//! 1. A merchant (or admin) creates a **plan** defining the token, amount, and interval.
//! 2. A subscriber calls `subscribe`, which locks the first payment immediately and
//!    records the next due ledger.
//! 3. Anyone may call `execute_payment` once the due ledger is reached; the contract
//!    pulls the agreed amount from the subscriber and forwards it to the merchant.
//! 4. On failure (insufficient balance / auth) the subscription is marked
//!    `PaymentFailed` and a retry window opens.  After `MAX_RETRIES` failures the
//!    subscription is automatically cancelled.
//! 5. The subscriber may call `cancel` at any time to stop future charges.
//!
//! ## Access Control
//! - Admin: initialise, create/deactivate plans, pause/unpause, upgrade.
//! - Subscriber: subscribe, cancel their own subscription.
//! - Anyone: execute a due payment (permissionless keeper).

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, symbol_short,
    token::Client as TokenClient, Address, Env,
};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Maximum consecutive payment failures before auto-cancellation.
const MAX_RETRIES: u32 = 3;
/// Instance storage TTL (~1 year at 5 s/ledger).
const TTL_THRESHOLD: u32 = 100_000;
const TTL_EXTEND: u32 = 6_307_200;
/// Persistent storage TTL (~6 months).
const PERSISTENT_TTL_THRESHOLD: u32 = 50_000;
const PERSISTENT_TTL_EXTEND: u32 = 3_153_600;

// ---------------------------------------------------------------------------
// Storage keys
// ---------------------------------------------------------------------------

#[contracttype]
#[derive(Clone)]
enum Key {
    Admin,
    Paused,
    PlanCounter,
    Plan(u64),
    Subscription(u64),
    SubCounter,
}

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// A recurring payment plan created by a merchant.
#[contracttype]
#[derive(Clone)]
pub struct Plan {
    /// Merchant / recipient of payments.
    pub merchant: Address,
    /// Token used for payments.
    pub token: Address,
    /// Amount charged per interval (in token base units).
    pub amount: i128,
    /// Payment interval in ledgers.
    pub interval_ledgers: u32,
    /// Whether new subscriptions can be created against this plan.
    pub active: bool,
}

/// Subscription state machine.
#[contracttype]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum SubscriptionStatus {
    Active = 1,
    Cancelled = 2,
    PaymentFailed = 3,
}

/// A subscriber's subscription to a plan.
#[contracttype]
#[derive(Clone)]
pub struct Subscription {
    pub plan_id: u64,
    pub subscriber: Address,
    /// Ledger at which the next payment is due.
    pub next_payment_ledger: u32,
    pub status: SubscriptionStatus,
    /// Consecutive payment failures.
    pub failure_count: u32,
    /// Total successful payments executed.
    pub payments_made: u32,
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[contracterror]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum SubError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    ContractPaused = 4,
    PlanNotFound = 5,
    PlanInactive = 6,
    SubscriptionNotFound = 7,
    SubscriptionNotActive = 8,
    PaymentNotDue = 9,
    InvalidAmount = 10,
    InvalidInterval = 11,
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

fn require_not_paused(env: &Env) {
    let paused: bool = env.storage().instance().get(&Key::Paused).unwrap_or(false);
    if paused {
        panic_with_error!(env, SubError::ContractPaused);
    }
}

fn require_admin(env: &Env) -> Address {
    let admin: Address = env
        .storage()
        .instance()
        .get(&Key::Admin)
        .unwrap_or_else(|| panic_with_error!(env, SubError::NotInitialized));
    admin.require_auth();
    admin
}

fn load_plan(env: &Env, plan_id: u64) -> Plan {
    env.storage()
        .persistent()
        .get(&Key::Plan(plan_id))
        .unwrap_or_else(|| panic_with_error!(env, SubError::PlanNotFound))
}

fn load_subscription(env: &Env, sub_id: u64) -> Subscription {
    env.storage()
        .persistent()
        .get(&Key::Subscription(sub_id))
        .unwrap_or_else(|| panic_with_error!(env, SubError::SubscriptionNotFound))
}

fn save_subscription(env: &Env, sub_id: u64, sub: &Subscription) {
    env.storage()
        .persistent()
        .set(&Key::Subscription(sub_id), sub);
    bump_persistent(env, &Key::Subscription(sub_id));
}

/// Attempt to pull `amount` of `token` from `from` to `to` using transfer_from.
/// Returns true on success, false if the transfer fails (insufficient balance/allowance).
fn try_pull(env: &Env, token: &Address, from: &Address, to: &Address, amount: i128) -> bool {
    let token_client = TokenClient::new(env, token);
    let spender = env.current_contract_address();
    token_client
        .try_transfer_from(&spender, from, to, &amount)
        .is_ok()
}

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

#[contract]
pub struct SubscriptionPayments;

#[contractimpl]
impl SubscriptionPayments {
    // -----------------------------------------------------------------------
    // Initialisation
    // -----------------------------------------------------------------------

    /// Initialise the contract. Can only be called once.
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&Key::Admin) {
            panic_with_error!(env, SubError::AlreadyInitialized);
        }
        admin.require_auth();
        env.storage().instance().set(&Key::Admin, &admin);
        env.storage().instance().set(&Key::Paused, &false);
        env.storage().instance().set(&Key::PlanCounter, &0u64);
        env.storage().instance().set(&Key::SubCounter, &0u64);
        bump_instance(&env);
    }

    // -----------------------------------------------------------------------
    // Plan management (admin only)
    // -----------------------------------------------------------------------

    /// Create a new recurring payment plan.
    ///
    /// * `merchant`         – address that receives payments
    /// * `token`            – token contract address
    /// * `amount`           – amount per payment (> 0)
    /// * `interval_ledgers` – ledgers between payments (> 0)
    ///
    /// Returns the new plan ID.
    pub fn create_plan(
        env: Env,
        merchant: Address,
        token: Address,
        amount: i128,
        interval_ledgers: u32,
    ) -> u64 {
        require_admin(&env);
        bump_instance(&env);

        if amount <= 0 {
            panic_with_error!(env, SubError::InvalidAmount);
        }
        if interval_ledgers == 0 {
            panic_with_error!(env, SubError::InvalidInterval);
        }

        let plan_id: u64 = env
            .storage()
            .instance()
            .get(&Key::PlanCounter)
            .unwrap_or(0);
        let next_id = plan_id + 1;

        let plan = Plan {
            merchant: merchant.clone(),
            token,
            amount,
            interval_ledgers,
            active: true,
        };

        env.storage().persistent().set(&Key::Plan(next_id), &plan);
        bump_persistent(&env, &Key::Plan(next_id));
        env.storage().instance().set(&Key::PlanCounter, &next_id);

        env.events().publish(
            (symbol_short!("sub"), symbol_short!("plan_new")),
            (next_id, merchant, amount, interval_ledgers),
        );

        next_id
    }

    /// Deactivate a plan so no new subscriptions can be created.
    /// Existing active subscriptions continue until cancelled.
    pub fn deactivate_plan(env: Env, plan_id: u64) {
        require_admin(&env);
        bump_instance(&env);

        let mut plan = load_plan(&env, plan_id);
        plan.active = false;
        env.storage().persistent().set(&Key::Plan(plan_id), &plan);
        bump_persistent(&env, &Key::Plan(plan_id));

        env.events().publish(
            (symbol_short!("sub"), symbol_short!("plan_off")),
            plan_id,
        );
    }

    // -----------------------------------------------------------------------
    // Subscription management
    // -----------------------------------------------------------------------

    /// Subscribe to a plan. Executes the first payment immediately.
    ///
    /// * `subscriber` – address being charged (must sign)
    /// * `plan_id`    – plan to subscribe to
    ///
    /// Returns the new subscription ID.
    pub fn subscribe(env: Env, subscriber: Address, plan_id: u64) -> u64 {
        require_not_paused(&env);
        bump_instance(&env);

        subscriber.require_auth();

        let plan = load_plan(&env, plan_id);
        if !plan.active {
            panic_with_error!(env, SubError::PlanInactive);
        }

        // Execute first payment immediately.
        let success = try_pull(&env, &plan.token, &subscriber, &plan.merchant, plan.amount);

        let sub_id: u64 = env
            .storage()
            .instance()
            .get(&Key::SubCounter)
            .unwrap_or(0);
        let next_sub_id = sub_id + 1;

        let current_ledger = env.ledger().sequence();
        let (status, failure_count, payments_made) = if success {
            (SubscriptionStatus::Active, 0u32, 1u32)
        } else {
            (SubscriptionStatus::PaymentFailed, 1u32, 0u32)
        };

        let sub = Subscription {
            plan_id,
            subscriber: subscriber.clone(),
            next_payment_ledger: current_ledger + plan.interval_ledgers,
            status,
            failure_count,
            payments_made,
        };

        save_subscription(&env, next_sub_id, &sub);
        env.storage().instance().set(&Key::SubCounter, &next_sub_id);

        env.events().publish(
            (symbol_short!("sub"), symbol_short!("sub_new")),
            (next_sub_id, subscriber, plan_id, success),
        );

        next_sub_id
    }

    /// Cancel a subscription. Only the subscriber may cancel their own subscription.
    pub fn cancel(env: Env, subscriber: Address, sub_id: u64) {
        require_not_paused(&env);
        bump_instance(&env);

        subscriber.require_auth();

        let mut sub = load_subscription(&env, sub_id);

        if sub.subscriber != subscriber {
            panic_with_error!(env, SubError::Unauthorized);
        }
        if sub.status == SubscriptionStatus::Cancelled {
            panic_with_error!(env, SubError::SubscriptionNotActive);
        }

        sub.status = SubscriptionStatus::Cancelled;
        save_subscription(&env, sub_id, &sub);

        env.events().publish(
            (symbol_short!("sub"), symbol_short!("cancelled")),
            (sub_id, subscriber),
        );
    }

    // -----------------------------------------------------------------------
    // Payment execution (permissionless keeper)
    // -----------------------------------------------------------------------

    /// Execute a due recurring payment for a subscription.
    ///
    /// Anyone may call this once `next_payment_ledger` has been reached.
    /// On failure the subscription's `failure_count` is incremented; after
    /// `MAX_RETRIES` failures the subscription is automatically cancelled.
    ///
    /// Returns `true` if the payment succeeded, `false` if it failed.
    pub fn execute_payment(env: Env, sub_id: u64) -> bool {
        require_not_paused(&env);
        bump_instance(&env);

        let mut sub = load_subscription(&env, sub_id);

        if sub.status == SubscriptionStatus::Cancelled {
            panic_with_error!(env, SubError::SubscriptionNotActive);
        }

        let current_ledger = env.ledger().sequence();
        if current_ledger < sub.next_payment_ledger {
            panic_with_error!(env, SubError::PaymentNotDue);
        }

        let plan = load_plan(&env, sub.plan_id);

        let success = try_pull(
            &env,
            &plan.token,
            &sub.subscriber,
            &plan.merchant,
            plan.amount,
        );

        if success {
            sub.payments_made += 1;
            sub.failure_count = 0;
            sub.status = SubscriptionStatus::Active;
            sub.next_payment_ledger = current_ledger + plan.interval_ledgers;

            env.events().publish(
                (symbol_short!("sub"), symbol_short!("paid")),
                (sub_id, sub.subscriber.clone(), plan.amount),
            );
        } else {
            sub.failure_count += 1;

            if sub.failure_count >= MAX_RETRIES {
                sub.status = SubscriptionStatus::Cancelled;
                env.events().publish(
                    (symbol_short!("sub"), symbol_short!("auto_cxl")),
                    (sub_id, sub.subscriber.clone(), sub.failure_count),
                );
            } else {
                sub.status = SubscriptionStatus::PaymentFailed;
                // Advance the due date so the keeper can retry after one interval.
                sub.next_payment_ledger = current_ledger + plan.interval_ledgers;
                env.events().publish(
                    (symbol_short!("sub"), symbol_short!("pay_fail")),
                    (sub_id, sub.subscriber.clone(), sub.failure_count),
                );
            }
        }

        save_subscription(&env, sub_id, &sub);
        success
    }

    // -----------------------------------------------------------------------
    // Admin: pause / unpause
    // -----------------------------------------------------------------------

    pub fn pause(env: Env) {
        require_admin(&env);
        env.storage().instance().set(&Key::Paused, &true);
        env.events()
            .publish((symbol_short!("sub"), symbol_short!("paused")), ());
    }

    pub fn unpause(env: Env) {
        require_admin(&env);
        env.storage().instance().set(&Key::Paused, &false);
        env.events()
            .publish((symbol_short!("sub"), symbol_short!("unpaused")), ());
    }

    // -----------------------------------------------------------------------
    // Admin: upgrade
    // -----------------------------------------------------------------------

    pub fn upgrade(env: Env, new_wasm_hash: soroban_sdk::BytesN<32>) {
        require_admin(&env);
        env.deployer().update_current_contract_wasm(new_wasm_hash);
    }

    // -----------------------------------------------------------------------
    // Admin: transfer admin
    // -----------------------------------------------------------------------

    pub fn transfer_admin(env: Env, new_admin: Address) {
        require_admin(&env);
        env.storage().instance().set(&Key::Admin, &new_admin);
        env.events().publish(
            (symbol_short!("sub"), symbol_short!("adm_xfer")),
            new_admin,
        );
    }

    // -----------------------------------------------------------------------
    // Views
    // -----------------------------------------------------------------------

    pub fn get_plan(env: Env, plan_id: u64) -> Plan {
        load_plan(&env, plan_id)
    }

    pub fn get_subscription(env: Env, sub_id: u64) -> Subscription {
        load_subscription(&env, sub_id)
    }

    pub fn is_paused(env: Env) -> bool {
        env.storage().instance().get(&Key::Paused).unwrap_or(false)
    }

    pub fn get_admin(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&Key::Admin)
            .unwrap_or_else(|| panic_with_error!(env, SubError::NotInitialized))
    }

    pub fn plan_count(env: Env) -> u64 {
        env.storage().instance().get(&Key::PlanCounter).unwrap_or(0)
    }

    pub fn sub_count(env: Env) -> u64 {
        env.storage().instance().get(&Key::SubCounter).unwrap_or(0)
    }
}

mod test;
