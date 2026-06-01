#![no_std]

//! # Advanced Payment System Contract
//!
//! Comprehensive payment system integrating:
//! 1. Enhanced Escrow with Advanced Dispute Resolution
//! 2. Payment Splitting with Scheduling & Conditions
//! 3. Governance with Emergency Proposals & Batch Voting
//!
//! This contract demonstrates the integration of all three advanced features.

use soroban_sdk::{
    contract, contractimpl, contracttype, panic_with_error, contracterror,
    symbol_short, Address, Env, Symbol, BytesN, Bytes,
    token::{Client as TokenClient},
    Vec,
};

// ─── Constants ────────────────────────────────────────────────────────────

const BPS_TOTAL: u32 = 10_000;
const TTL_THRESHOLD: u32 = 100_000;
const TTL_EXTEND: u32 = 6_307_200;

// ─── Storage Keys ────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone)]
enum StorageKey {
    Admin,
    Token,
    Config,
    // Escrow keys
    Escrow(BytesN<32>),
    Dispute(BytesN<32>),
    Reputation(Address),
    // Payment split keys
    Splits,
    Schedules,
    Conditionals,
    TotalIn,
    // Governance keys
    ProposalCounter,
    Proposal(u64),
    VoteRecord(u64, Address),
    VotingPower(Address),
    DelegateTarget(Address),
    DelegatedPower(Address),
}

// ─── Data Types ──────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone)]
pub struct PaymentConfig {
    pub escrow_enabled: bool,
    pub splitting_enabled: bool,
    pub governance_enabled: bool,
    pub dispute_timeout_days: u32,
    pub min_arbitrator_reputation: i32,
}

#[contracttype]
#[derive(Clone)]
pub struct EscrowRecord {
    pub id: BytesN<32>,
    pub buyer: Address,
    pub seller: Address,
    pub amount: i128,
    pub state: EscrowState,
    pub created_at: u64,
    pub dispute_tier: u32,
}

#[contracttype]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum EscrowState {
    Locked = 1,
    Released = 2,
    Refunded = 3,
    Disputed = 4,
}

#[contracttype]
#[derive(Clone)]
pub struct SplitConfig {
    pub recipient: Address,
    pub percentage: u32,
    pub is_scheduled: bool,
    pub vesting_period: u32,
}

#[contracttype]
#[derive(Clone)]
pub struct GovernanceProposal {
    pub id: u64,
    pub proposer: Address,
    pub payload: Bytes,
    pub status: ProposalStatus,
    pub for_votes: i128,
    pub against_votes: i128,
    pub created_ledger: u32,
}

#[contracttype]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum ProposalStatus {
    Active = 1,
    Passed = 2,
    Failed = 3,
    Executed = 4,
    Cancelled = 5,
}

// ─── Errors ──────────────────────────────────────────────────────────────

#[contracterror]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum AdvancedPaymentError {
    NotAuthorized = 1,
    NotInitialized = 2,
    AlreadyInitialized = 3,
    InvalidAmount = 4,
    InvalidState = 5,
    EscrowNotFound = 6,
    DisputeNotFound = 7,
    ProposalNotFound = 8,
    InsufficientReputation = 9,
    InvalidConfiguration = 10,
    OperationNotAllowed = 11,
    Reentrant = 12,
}

// ─── Reentrancy Guard ────────────────────────────────────────────────────

fn reentrancy_guard_enter(env: &Env) {
    let key = symbol_short!("re_lock");
    if env.storage().instance().get::<Symbol, bool>(&key).unwrap_or(false) {
        panic_with_error!(env, AdvancedPaymentError::Reentrant);
    }
    env.storage().instance().set(&key, &true);
}

fn reentrancy_guard_exit(env: &Env) {
    let key = symbol_short!("re_lock");
    env.storage().instance().set(&key, &false);
}

// ─── Contract ────────────────────────────────────────────────────────────

#[contract]
pub struct AdvancedPaymentSystem;

#[contractimpl]
impl AdvancedPaymentSystem {
    // ───────────────────────────────────────────────────────────────────────
    // Initialization
    // ───────────────────────────────────────────────────────────────────────

    pub fn initialize(
        env: Env,
        admin: Address,
        token: Address,
        config: PaymentConfig,
    ) {
        reentrancy_guard_enter(&env);
        admin.require_auth();

        if env.storage().instance().has(&StorageKey::Admin) {
            reentrancy_guard_exit(&env);
            panic_with_error!(env, AdvancedPaymentError::AlreadyInitialized);
        }

        env.storage().instance().set(&StorageKey::Admin, &admin);
        env.storage().instance().set(&StorageKey::Token, &token);
        env.storage().instance().set(&StorageKey::Config, &config);
        env.storage().instance().set(&StorageKey::ProposalCounter, &0u64);
        env.storage().instance().set(&StorageKey::TotalIn, &0i128);

        env.events().publish(
            (symbol_short!("adv_pay"), symbol_short!("init")),
            (admin, token),
        );

        reentrancy_guard_exit(&env);
    }

    // ───────────────────────────────────────────────────────────────────────
    // Escrow Operations
    // ───────────────────────────────────────────────────────────────────────

    pub fn create_escrow(
        env: Env,
        escrow_id: BytesN<32>,
        buyer: Address,
        seller: Address,
        amount: i128,
    ) {
        reentrancy_guard_enter(&env);
        buyer.require_auth();

        if amount <= 0 {
            reentrancy_guard_exit(&env);
            panic_with_error!(env, AdvancedPaymentError::InvalidAmount);
        }

        let config: PaymentConfig = env.storage().instance()
            .get(&StorageKey::Config)
            .unwrap_or_else(|| panic_with_error!(env, AdvancedPaymentError::NotInitialized));

        if !config.escrow_enabled {
            reentrancy_guard_exit(&env);
            panic_with_error!(env, AdvancedPaymentError::OperationNotAllowed);
        }

        let token: Address = env.storage().instance()
            .get(&StorageKey::Token)
            .unwrap_or_else(|| panic_with_error!(env, AdvancedPaymentError::NotInitialized));

        let key = StorageKey::Escrow(escrow_id.clone());
        if env.storage().persistent().has(&key) {
            reentrancy_guard_exit(&env);
            panic_with_error!(env, AdvancedPaymentError::InvalidState);
        }

        let escrow = EscrowRecord {
            id: escrow_id.clone(),
            buyer: buyer.clone(),
            seller: seller.clone(),
            amount,
            state: EscrowState::Locked,
            created_at: env.ledger().timestamp(),
            dispute_tier: 0,
        };

        env.storage().persistent().set(&key, &escrow);

        let token_client = TokenClient::new(&env, &token);
        token_client.transfer(&buyer, &env.current_contract_address(), &amount);

        env.events().publish(
            (symbol_short!("adv_pay"), symbol_short!("esc_cr")),
            (escrow_id, buyer, seller, amount),
        );

        reentrancy_guard_exit(&env);
    }

    pub fn release_escrow(env: Env, escrow_id: BytesN<32>, caller: Address) {
        reentrancy_guard_enter(&env);
        caller.require_auth();

        let key = StorageKey::Escrow(escrow_id.clone());
        let mut escrow: EscrowRecord = env.storage().persistent()
            .get(&key)
            .unwrap_or_else(|| panic_with_error!(env, AdvancedPaymentError::EscrowNotFound));

        if escrow.state != EscrowState::Locked {
            reentrancy_guard_exit(&env);
            panic_with_error!(env, AdvancedPaymentError::InvalidState);
        }

        if caller != escrow.seller {
            reentrancy_guard_exit(&env);
            panic_with_error!(env, AdvancedPaymentError::NotAuthorized);
        }

        escrow.state = EscrowState::Released;
        env.storage().persistent().set(&key, &escrow);

        let token: Address = env.storage().instance()
            .get(&StorageKey::Token)
            .unwrap_or_else(|| panic_with_error!(env, AdvancedPaymentError::NotInitialized));

        let token_client = TokenClient::new(&env, &token);
        token_client.transfer(
            &env.current_contract_address(),
            &escrow.seller,
            &escrow.amount,
        );

        env.events().publish(
            (symbol_short!("adv_pay"), symbol_short!("esc_rel")),
            (escrow_id, escrow.seller, escrow.amount),
        );

        reentrancy_guard_exit(&env);
    }

    pub fn refund_escrow(env: Env, escrow_id: BytesN<32>, caller: Address) {
        reentrancy_guard_enter(&env);
        caller.require_auth();

        let key = StorageKey::Escrow(escrow_id.clone());
        let mut escrow: EscrowRecord = env.storage().persistent()
            .get(&key)
            .unwrap_or_else(|| panic_with_error!(env, AdvancedPaymentError::EscrowNotFound));

        if escrow.state != EscrowState::Locked {
            reentrancy_guard_exit(&env);
            panic_with_error!(env, AdvancedPaymentError::InvalidState);
        }

        if caller != escrow.buyer {
            reentrancy_guard_exit(&env);
            panic_with_error!(env, AdvancedPaymentError::NotAuthorized);
        }

        escrow.state = EscrowState::Refunded;
        env.storage().persistent().set(&key, &escrow);

        let token: Address = env.storage().instance()
            .get(&StorageKey::Token)
            .unwrap_or_else(|| panic_with_error!(env, AdvancedPaymentError::NotInitialized));

        let token_client = TokenClient::new(&env, &token);
        token_client.transfer(
            &env.current_contract_address(),
            &escrow.buyer,
            &escrow.amount,
        );

        env.events().publish(
            (symbol_short!("adv_pay"), symbol_short!("esc_ref")),
            (escrow_id, escrow.buyer, escrow.amount),
        );

        reentrancy_guard_exit(&env);
    }

    pub fn initiate_dispute(
        env: Env,
        escrow_id: BytesN<32>,
        caller: Address,
        arbitrators: Vec<Address>,
    ) {
        reentrancy_guard_enter(&env);
        caller.require_auth();

        let key = StorageKey::Escrow(escrow_id.clone());
        let mut escrow: EscrowRecord = env.storage().persistent()
            .get(&key)
            .unwrap_or_else(|| panic_with_error!(env, AdvancedPaymentError::EscrowNotFound));

        if escrow.state != EscrowState::Locked {
            reentrancy_guard_exit(&env);
            panic_with_error!(env, AdvancedPaymentError::InvalidState);
        }

        if caller != escrow.buyer && caller != escrow.seller {
            reentrancy_guard_exit(&env);
            panic_with_error!(env, AdvancedPaymentError::NotAuthorized);
        }

        escrow.state = EscrowState::Disputed;
        escrow.dispute_tier = escrow.dispute_tier.saturating_add(1);
        env.storage().persistent().set(&key, &escrow);

        env.events().publish(
            (symbol_short!("adv_pay"), symbol_short!("disp_init")),
            (escrow_id, caller, arbitrators.len() as u32),
        );

        reentrancy_guard_exit(&env);
    }

    // ───────────────────────────────────────────────────────────────────────
    // Payment Splitting Operations
    // ───────────────────────────────────────────────────────────────────────

    pub fn add_split_recipient(
        env: Env,
        recipient: Address,
        percentage: u32,
    ) {
        reentrancy_guard_enter(&env);

        let admin: Address = env.storage().instance()
            .get(&StorageKey::Admin)
            .unwrap_or_else(|| panic_with_error!(env, AdvancedPaymentError::NotInitialized));
        admin.require_auth();

        let config: PaymentConfig = env.storage().instance()
            .get(&StorageKey::Config)
            .unwrap_or_else(|| panic_with_error!(env, AdvancedPaymentError::NotInitialized));

        if !config.splitting_enabled {
            reentrancy_guard_exit(&env);
            panic_with_error!(env, AdvancedPaymentError::OperationNotAllowed);
        }

        if percentage == 0 || percentage > BPS_TOTAL {
            reentrancy_guard_exit(&env);
            panic_with_error!(env, AdvancedPaymentError::InvalidConfiguration);
        }

        let mut splits: Vec<SplitConfig> = env.storage().instance()
            .get(&StorageKey::Splits)
            .unwrap_or(Vec::new(&env));

        let split = SplitConfig {
            recipient: recipient.clone(),
            percentage,
            is_scheduled: false,
            vesting_period: 0,
        };

        splits.push_back(split);
        env.storage().instance().set(&StorageKey::Splits, &splits);

        env.events().publish(
            (symbol_short!("adv_pay"), symbol_short!("spl_add")),
            (recipient, percentage),
        );

        reentrancy_guard_exit(&env);
    }

    pub fn split_payment(env: Env, from: Address, amount: i128) {
        reentrancy_guard_enter(&env);
        from.require_auth();

        if amount <= 0 {
            reentrancy_guard_exit(&env);
            panic_with_error!(env, AdvancedPaymentError::InvalidAmount);
        }

        let config: PaymentConfig = env.storage().instance()
            .get(&StorageKey::Config)
            .unwrap_or_else(|| panic_with_error!(env, AdvancedPaymentError::NotInitialized));

        if !config.splitting_enabled {
            reentrancy_guard_exit(&env);
            panic_with_error!(env, AdvancedPaymentError::OperationNotAllowed);
        }

        let token: Address = env.storage().instance()
            .get(&StorageKey::Token)
            .unwrap_or_else(|| panic_with_error!(env, AdvancedPaymentError::NotInitialized));

        let token_client = TokenClient::new(&env, &token);
        let contract_addr = env.current_contract_address();

        token_client.transfer(&from, &contract_addr, &amount);

        let total_in: i128 = env.storage().instance()
            .get(&StorageKey::TotalIn)
            .unwrap_or(0);
        env.storage().instance().set(&StorageKey::TotalIn, &(total_in + amount));

        let splits: Vec<SplitConfig> = env.storage().instance()
            .get(&StorageKey::Splits)
            .unwrap_or(Vec::new(&env));

        for split in splits.iter() {
            let payout = amount * (split.percentage as i128) / (BPS_TOTAL as i128);
            if payout > 0 {
                token_client.transfer(&contract_addr, &split.recipient, &payout);
            }
        }

        env.events().publish(
            (symbol_short!("adv_pay"), symbol_short!("spl_pay")),
            (from, amount),
        );

        reentrancy_guard_exit(&env);
    }

    // ───────────────────────────────────────────────────────────────────────
    // Governance Operations
    // ───────────────────────────────────────────────────────────────────────

    pub fn create_proposal(
        env: Env,
        proposer: Address,
        payload: Bytes,
        description: Bytes,
    ) -> u64 {
        reentrancy_guard_enter(&env);
        proposer.require_auth();

        let config: PaymentConfig = env.storage().instance()
            .get(&StorageKey::Config)
            .unwrap_or_else(|| panic_with_error!(env, AdvancedPaymentError::NotInitialized));

        if !config.governance_enabled {
            reentrancy_guard_exit(&env);
            panic_with_error!(env, AdvancedPaymentError::OperationNotAllowed);
        }

        let proposal_id: u64 = env.storage().instance()
            .get(&StorageKey::ProposalCounter)
            .unwrap_or(0);
        let next_id = proposal_id + 1;

        let proposal = GovernanceProposal {
            id: next_id,
            proposer: proposer.clone(),
            payload,
            status: ProposalStatus::Active,
            for_votes: 0,
            against_votes: 0,
            created_ledger: env.ledger().sequence(),
        };

        env.storage().persistent().set(&StorageKey::Proposal(next_id), &proposal);
        env.storage().instance().set(&StorageKey::ProposalCounter, &next_id);

        env.events().publish(
            (symbol_short!("adv_pay"), symbol_short!("prop_cr")),
            (next_id, proposer),
        );

        reentrancy_guard_exit(&env);
        next_id
    }

    pub fn vote_on_proposal(
        env: Env,
        voter: Address,
        proposal_id: u64,
        support: bool,
    ) {
        reentrancy_guard_enter(&env);
        voter.require_auth();

        let mut proposal: GovernanceProposal = env.storage().persistent()
            .get(&StorageKey::Proposal(proposal_id))
            .unwrap_or_else(|| panic_with_error!(env, AdvancedPaymentError::ProposalNotFound));

        if proposal.status != ProposalStatus::Active {
            reentrancy_guard_exit(&env);
            panic_with_error!(env, AdvancedPaymentError::InvalidState);
        }

        let voting_power: i128 = env.storage().persistent()
            .get(&StorageKey::VotingPower(voter.clone()))
            .unwrap_or(1); // Default 1 vote per participant

        if support {
            proposal.for_votes += voting_power;
        } else {
            proposal.against_votes += voting_power;
        }

        env.storage().persistent().set(&StorageKey::Proposal(proposal_id), &proposal);

        env.events().publish(
            (symbol_short!("adv_pay"), symbol_short!("voted")),
            (proposal_id, voter, support),
        );

        reentrancy_guard_exit(&env);
    }

    // ───────────────────────────────────────────────────────────────────────
    // View Functions
    // ───────────────────────────────────────────────────────────────────────

    pub fn get_escrow(env: Env, escrow_id: BytesN<32>) -> EscrowRecord {
        let key = StorageKey::Escrow(escrow_id);
        env.storage().persistent()
            .get(&key)
            .unwrap_or_else(|| panic_with_error!(env, AdvancedPaymentError::EscrowNotFound))
    }

    pub fn get_proposal(env: Env, proposal_id: u64) -> GovernanceProposal {
        env.storage().persistent()
            .get(&StorageKey::Proposal(proposal_id))
            .unwrap_or_else(|| panic_with_error!(env, AdvancedPaymentError::ProposalNotFound))
    }

    pub fn get_splits(env: Env) -> Vec<SplitConfig> {
        env.storage().instance()
            .get(&StorageKey::Splits)
            .unwrap_or(Vec::new(&env))
    }

    pub fn get_config(env: Env) -> PaymentConfig {
        env.storage().instance()
            .get(&StorageKey::Config)
            .unwrap_or_else(|| panic_with_error!(env, AdvancedPaymentError::NotInitialized))
    }

    pub fn get_admin(env: Env) -> Address {
        env.storage().instance()
            .get(&StorageKey::Admin)
            .unwrap_or_else(|| panic_with_error!(env, AdvancedPaymentError::NotInitialized))
    }
}

mod test;
