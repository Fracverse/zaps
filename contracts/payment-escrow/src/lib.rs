#![no_std]

//! # Payment Escrow Automation Contract
//!
//! Automates conditional payment release based on on-chain conditions.
//!
//! ## Overview
//!
//! This contract extends the basic escrow pattern with **automation triggers**:
//! conditions that, when met, allow anyone to release or refund the escrow
//! without requiring the buyer or seller to act.  This enables trustless
//! payment flows such as:
//!
//! * **Deadline-based release** – funds are automatically released to the
//!   seller after a configurable deadline if the buyer has not raised a
//!   dispute.
//! * **Milestone confirmation** – an authorised oracle/confirmer marks a
//!   milestone as complete, which triggers automatic release.
//! * **Dispute resolution** – either party may open a dispute; an admin
//!   resolver then decides the outcome.
//!
//! ## Lifecycle
//!
//! ```text
//! lock_funds()
//!     │
//!     ▼
//!  Locked ──────────────────────────────────────────────────────────────────┐
//!     │                                                                     │
//!     ├─ confirm_milestone() by confirmer ──► auto_release() by anyone ──► Released
//!     │
//!     ├─ auto_release() after release_deadline ──────────────────────────► Released
//!     │
//!     ├─ buyer calls refund_request() ──────────────────────────────────► Refunded
//!     │
//!     └─ buyer or seller calls open_dispute() ──► Disputed
//!                                                     │
//!                                                     └─ admin calls resolve_dispute()
//!                                                             ├─ release ──► Released
//!                                                             └─ refund  ──► Refunded
//! ```
//!
//! ## Security
//!
//! * Reentrancy guard on all state-changing functions.
//! * Checks-Effects-Interactions ordering throughout.
//! * Admin-managed confirmer whitelist.
//! * Circuit breaker (pause/unpause).

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, symbol_short,
    token::Client as TokenClient, Address, Bytes, Env, Symbol,
};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

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
const KEY_LOCKED: Symbol = symbol_short!("re_lock");
const KEY_COUNTER: Symbol = symbol_short!("counter");
const KEY_VERSION: Symbol = symbol_short!("version");

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    /// Authorised milestone confirmer.
    Confirmer(Address),
    /// Escrow record by ID.
    Escrow(u64),
}

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Escrow state machine.
#[contracttype]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum EscrowState {
    /// Funds are locked; awaiting release condition.
    Locked = 1,
    /// Funds released to the seller.
    Released = 2,
    /// Funds refunded to the buyer.
    Refunded = 3,
    /// Dispute opened; awaiting admin resolution.
    Disputed = 4,
}

/// An automated payment escrow.
#[contracttype]
#[derive(Clone)]
pub struct PaymentEscrow {
    pub id: u64,
    /// Payer — receives refund if escrow is refunded.
    pub buyer: Address,
    /// Payee — receives funds if escrow is released.
    pub seller: Address,
    /// Token contract address.
    pub token: Address,
    /// Amount locked in this escrow.
    pub amount: i128,
    /// Opaque reference (e.g. invoice ID), 32 bytes.
    pub reference: Bytes,
    /// Ledger at which the escrow was created.
    pub created_ledger: u32,
    /// Ledger after which `auto_release` may be called by anyone.
    /// Set to 0 to disable deadline-based auto-release.
    pub release_deadline: u32,
    /// Whether a milestone confirmer has confirmed delivery.
    pub milestone_confirmed: bool,
    pub state: EscrowState,
    /// Resolver note set during dispute resolution.
    pub resolution_note: Bytes,
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[contracterror]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum EscrowError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    ContractPaused = 4,
    Reentrant = 5,
    EscrowNotFound = 6,
    /// The escrow is not in the `Locked` state.
    NotLocked = 7,
    /// The escrow is not in the `Disputed` state.
    NotDisputed = 8,
    InvalidAmount = 9,
    /// The release deadline has not been reached yet.
    DeadlineNotReached = 10,
    /// No release deadline is configured (deadline == 0).
    NoDeadlineConfigured = 11,
    /// Milestone has not been confirmed by a confirmer.
    MilestoneNotConfirmed = 12,
    /// Milestone was already confirmed.
    AlreadyConfirmed = 13,
    ConfirmerAlreadyAdded = 14,
    ConfirmerNotFound = 15,
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
        .unwrap_or_else(|| panic_with_error!(env, EscrowError::NotInitialized));
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
        panic_with_error!(env, EscrowError::ContractPaused);
    }
}

fn reentrancy_enter(env: &Env) {
    if env
        .storage()
        .instance()
        .get::<Symbol, bool>(&KEY_LOCKED)
        .unwrap_or(false)
    {
        panic_with_error!(env, EscrowError::Reentrant);
    }
    env.storage().instance().set(&KEY_LOCKED, &true);
}

fn reentrancy_exit(env: &Env) {
    env.storage().instance().set(&KEY_LOCKED, &false);
}

fn next_id(env: &Env) -> u64 {
    let id: u64 = env.storage().instance().get(&KEY_COUNTER).unwrap_or(0);
    env.storage().instance().set(&KEY_COUNTER, &(id + 1));
    id
}

fn load_escrow(env: &Env, escrow_id: u64) -> PaymentEscrow {
    env.storage()
        .persistent()
        .get(&DataKey::Escrow(escrow_id))
        .unwrap_or_else(|| panic_with_error!(env, EscrowError::EscrowNotFound))
}

fn save_escrow(env: &Env, escrow: &PaymentEscrow) {
    env.storage()
        .persistent()
        .set(&DataKey::Escrow(escrow.id), escrow);
    bump_persistent(env, &DataKey::Escrow(escrow.id));
}

fn is_confirmer(env: &Env, address: &Address) -> bool {
    env.storage()
        .persistent()
        .get::<DataKey, bool>(&DataKey::Confirmer(address.clone()))
        .unwrap_or(false)
}

/// Transfer `amount` of `token` from `from` to `to`.
fn transfer(env: &Env, token: &Address, from: &Address, to: &Address, amount: i128) {
    TokenClient::new(env, token).transfer(from, to, &amount);
}

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

#[contract]
pub struct PaymentEscrowAutomation;

#[contractimpl]
impl PaymentEscrowAutomation {
    // -----------------------------------------------------------------------
    // Initialisation
    // -----------------------------------------------------------------------

    /// Initialise the contract.
    ///
    /// * `admin` – address that manages confirmers and resolves disputes
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&KEY_ADMIN) {
            panic_with_error!(env, EscrowError::AlreadyInitialized);
        }

        admin.require_auth();

        env.storage().instance().set(&KEY_ADMIN, &admin);
        env.storage().instance().set(&KEY_PAUSED, &false);
        env.storage().instance().set(&KEY_LOCKED, &false);
        env.storage().instance().set(&KEY_COUNTER, &0u64);
        env.storage().instance().set(&KEY_VERSION, &1u32);
        bump_instance(&env);

        env.events().publish(
            (symbol_short!("escrow_a"), symbol_short!("init")),
            admin,
        );
    }

    // -----------------------------------------------------------------------
    // Confirmer management (admin only)
    // -----------------------------------------------------------------------

    /// Authorise an address to confirm milestone completion.
    pub fn add_confirmer(env: Env, confirmer: Address) {
        require_admin(&env);
        bump_instance(&env);

        let key = DataKey::Confirmer(confirmer.clone());
        if env
            .storage()
            .persistent()
            .get::<DataKey, bool>(&key)
            .unwrap_or(false)
        {
            panic_with_error!(env, EscrowError::ConfirmerAlreadyAdded);
        }

        env.storage().persistent().set(&key, &true);
        bump_persistent(&env, &key);

        env.events().publish(
            (symbol_short!("escrow_a"), symbol_short!("cfm_add")),
            confirmer,
        );
    }

    /// Remove a confirmer.
    pub fn remove_confirmer(env: Env, confirmer: Address) {
        require_admin(&env);
        bump_instance(&env);

        let key = DataKey::Confirmer(confirmer.clone());
        if !env
            .storage()
            .persistent()
            .get::<DataKey, bool>(&key)
            .unwrap_or(false)
        {
            panic_with_error!(env, EscrowError::ConfirmerNotFound);
        }

        env.storage().persistent().remove(&key);

        env.events().publish(
            (symbol_short!("escrow_a"), symbol_short!("cfm_rm")),
            confirmer,
        );
    }

    // -----------------------------------------------------------------------
    // Core: lock funds
    // -----------------------------------------------------------------------

    /// Lock funds into an automated escrow.
    ///
    /// * `buyer`            – payer (must sign); receives refund if escrow is refunded
    /// * `seller`           – payee; receives funds if escrow is released
    /// * `token`            – token contract address
    /// * `amount`           – amount to lock (> 0)
    /// * `reference`        – opaque reference bytes (e.g. invoice ID)
    /// * `release_deadline` – ledger after which auto-release is allowed
    ///                        (0 = no deadline-based auto-release)
    ///
    /// Returns the new escrow ID.
    pub fn lock_funds(
        env: Env,
        buyer: Address,
        seller: Address,
        token: Address,
        amount: i128,
        reference: Bytes,
        release_deadline: u32,
    ) -> u64 {
        require_not_paused(&env);
        bump_instance(&env);

        buyer.require_auth();

        if amount <= 0 {
            panic_with_error!(env, EscrowError::InvalidAmount);
        }

        reentrancy_enter(&env);

        let escrow_id = next_id(&env);
        let current_ledger = env.ledger().sequence();

        // Effects: persist escrow record before token transfer.
        let escrow = PaymentEscrow {
            id: escrow_id,
            buyer: buyer.clone(),
            seller: seller.clone(),
            token: token.clone(),
            amount,
            reference: reference.clone(),
            created_ledger: current_ledger,
            release_deadline,
            milestone_confirmed: false,
            state: EscrowState::Locked,
            resolution_note: Bytes::new(&env),
        };
        save_escrow(&env, &escrow);

        // Interaction: pull funds from buyer.
        transfer(&env, &token, &buyer, &env.current_contract_address(), amount);

        reentrancy_exit(&env);

        env.events().publish(
            (symbol_short!("escrow_a"), symbol_short!("locked")),
            (escrow_id, buyer, seller, amount, release_deadline),
        );

        escrow_id
    }

    // -----------------------------------------------------------------------
    // Core: milestone confirmation
    // -----------------------------------------------------------------------

    /// Confirm that the milestone for an escrow has been completed.
    ///
    /// Only an authorised confirmer may call this.  Once confirmed, anyone
    /// may call `auto_release` to release the funds.
    pub fn confirm_milestone(env: Env, confirmer: Address, escrow_id: u64) {
        require_not_paused(&env);
        bump_instance(&env);

        confirmer.require_auth();

        if !is_confirmer(&env, &confirmer) {
            panic_with_error!(env, EscrowError::Unauthorized);
        }

        let mut escrow = load_escrow(&env, escrow_id);

        if escrow.state != EscrowState::Locked {
            panic_with_error!(env, EscrowError::NotLocked);
        }

        if escrow.milestone_confirmed {
            panic_with_error!(env, EscrowError::AlreadyConfirmed);
        }

        escrow.milestone_confirmed = true;
        save_escrow(&env, &escrow);

        env.events().publish(
            (symbol_short!("escrow_a"), symbol_short!("milestone")),
            (escrow_id, confirmer),
        );
    }

    // -----------------------------------------------------------------------
    // Core: automated release (permissionless)
    // -----------------------------------------------------------------------

    /// Automatically release funds to the seller.
    ///
    /// This is permissionless — anyone may call it once either:
    /// - The milestone has been confirmed by an authorised confirmer, OR
    /// - The `release_deadline` ledger has been reached (and deadline ≠ 0).
    pub fn auto_release(env: Env, escrow_id: u64) {
        require_not_paused(&env);
        bump_instance(&env);

        let mut escrow = load_escrow(&env, escrow_id);

        if escrow.state != EscrowState::Locked {
            panic_with_error!(env, EscrowError::NotLocked);
        }

        let current_ledger = env.ledger().sequence();
        let deadline_reached = escrow.release_deadline > 0
            && current_ledger >= escrow.release_deadline;

        if !escrow.milestone_confirmed && !deadline_reached {
            // Neither condition is met — determine which error to surface.
            if escrow.release_deadline == 0 {
                panic_with_error!(env, EscrowError::MilestoneNotConfirmed);
            } else {
                panic_with_error!(env, EscrowError::DeadlineNotReached);
            }
        }

        reentrancy_enter(&env);

        // Effects before interaction.
        escrow.state = EscrowState::Released;
        save_escrow(&env, &escrow);

        // Interaction: release to seller.
        transfer(
            &env,
            &escrow.token,
            &env.current_contract_address(),
            &escrow.seller,
            escrow.amount,
        );

        reentrancy_exit(&env);

        env.events().publish(
            (symbol_short!("escrow_a"), symbol_short!("released")),
            (escrow_id, escrow.seller, escrow.amount),
        );
    }

    // -----------------------------------------------------------------------
    // Core: buyer-initiated refund
    // -----------------------------------------------------------------------

    /// Request a refund.  Only the buyer may call this while the escrow is
    /// in the `Locked` state and no milestone has been confirmed.
    pub fn refund_request(env: Env, buyer: Address, escrow_id: u64) {
        require_not_paused(&env);
        bump_instance(&env);

        buyer.require_auth();

        let mut escrow = load_escrow(&env, escrow_id);

        if escrow.state != EscrowState::Locked {
            panic_with_error!(env, EscrowError::NotLocked);
        }

        if escrow.buyer != buyer {
            panic_with_error!(env, EscrowError::Unauthorized);
        }

        // Cannot refund if milestone already confirmed.
        if escrow.milestone_confirmed {
            panic_with_error!(env, EscrowError::AlreadyConfirmed);
        }

        reentrancy_enter(&env);

        // Effects before interaction.
        escrow.state = EscrowState::Refunded;
        save_escrow(&env, &escrow);

        // Interaction: return funds to buyer.
        transfer(
            &env,
            &escrow.token,
            &env.current_contract_address(),
            &escrow.buyer,
            escrow.amount,
        );

        reentrancy_exit(&env);

        env.events().publish(
            (symbol_short!("escrow_a"), symbol_short!("refunded")),
            (escrow_id, buyer, escrow.amount),
        );
    }

    // -----------------------------------------------------------------------
    // Core: dispute flow
    // -----------------------------------------------------------------------

    /// Open a dispute on a locked escrow.
    ///
    /// Either the buyer or seller may open a dispute.  Once disputed, only
    /// the admin resolver may settle it via `resolve_dispute`.
    pub fn open_dispute(env: Env, caller: Address, escrow_id: u64) {
        require_not_paused(&env);
        bump_instance(&env);

        caller.require_auth();

        let mut escrow = load_escrow(&env, escrow_id);

        if escrow.state != EscrowState::Locked {
            panic_with_error!(env, EscrowError::NotLocked);
        }

        if caller != escrow.buyer && caller != escrow.seller {
            panic_with_error!(env, EscrowError::Unauthorized);
        }

        escrow.state = EscrowState::Disputed;
        save_escrow(&env, &escrow);

        env.events().publish(
            (symbol_short!("escrow_a"), symbol_short!("disputed")),
            (escrow_id, caller),
        );
    }

    /// Resolve a disputed escrow.
    ///
    /// Only the admin may call this.
    ///
    /// * `release_to_seller` – `true` → release to seller; `false` → refund buyer
    /// * `note`              – resolution note stored on the escrow
    pub fn resolve_dispute(
        env: Env,
        escrow_id: u64,
        release_to_seller: bool,
        note: Bytes,
    ) {
        require_not_paused(&env);
        require_admin(&env);
        bump_instance(&env);

        let mut escrow = load_escrow(&env, escrow_id);

        if escrow.state != EscrowState::Disputed {
            panic_with_error!(env, EscrowError::NotDisputed);
        }

        reentrancy_enter(&env);

        let (new_state, dest) = if release_to_seller {
            (EscrowState::Released, escrow.seller.clone())
        } else {
            (EscrowState::Refunded, escrow.buyer.clone())
        };

        // Effects before interaction.
        escrow.state = new_state;
        escrow.resolution_note = note;
        save_escrow(&env, &escrow);

        // Interaction: transfer funds.
        transfer(
            &env,
            &escrow.token,
            &env.current_contract_address(),
            &dest,
            escrow.amount,
        );

        reentrancy_exit(&env);

        env.events().publish(
            (symbol_short!("escrow_a"), symbol_short!("resolved")),
            (escrow_id, release_to_seller, dest, escrow.amount),
        );
    }

    // -----------------------------------------------------------------------
    // Admin: circuit breaker / upgrade / transfer admin
    // -----------------------------------------------------------------------

    pub fn pause(env: Env) {
        require_admin(&env);
        env.storage().instance().set(&KEY_PAUSED, &true);
        env.events()
            .publish((symbol_short!("escrow_a"), symbol_short!("paused")), ());
    }

    pub fn unpause(env: Env) {
        require_admin(&env);
        env.storage().instance().set(&KEY_PAUSED, &false);
        env.events()
            .publish((symbol_short!("escrow_a"), symbol_short!("unpaused")), ());
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
            (symbol_short!("escrow_a"), symbol_short!("adm_xfer")),
            new_admin,
        );
    }

    // -----------------------------------------------------------------------
    // Views
    // -----------------------------------------------------------------------

    pub fn get_escrow(env: Env, escrow_id: u64) -> PaymentEscrow {
        load_escrow(&env, escrow_id)
    }

    pub fn escrow_count(env: Env) -> u64 {
        env.storage().instance().get(&KEY_COUNTER).unwrap_or(0)
    }

    pub fn is_confirmer(env: Env, address: Address) -> bool {
        is_confirmer(&env, &address)
    }

    pub fn is_paused(env: Env) -> bool {
        env.storage()
            .instance()
            .get::<Symbol, bool>(&KEY_PAUSED)
            .unwrap_or(false)
    }

    pub fn get_admin(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&KEY_ADMIN)
            .unwrap_or_else(|| panic_with_error!(env, EscrowError::NotInitialized))
    }

    pub fn get_version(env: Env) -> u32 {
        env.storage().instance().get(&KEY_VERSION).unwrap_or(1)
    }
}


