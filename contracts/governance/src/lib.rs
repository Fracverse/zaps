#![no_std]

//! # Governance Contract
//!
//! Protocol parameter governance with proposal creation, token-weighted voting,
//! delegation, quorum enforcement, and time-locked execution.
//!
//! ## Lifecycle
//!
//! 1. **Propose** – any address with voting power ≥ `proposal_threshold` creates a
//!    proposal.  The proposal enters the `Active` voting period.
//! 2. **Vote / Delegate** – token holders vote For/Against/Abstain, or delegate their
//!    voting power to another address.  Delegated power is additive.
//! 3. **Queue** – once the voting period ends, anyone may call `queue` if the proposal
//!    passed (For > Against AND quorum met).  This starts the time-lock delay.
//! 4. **Execute** – after the time-lock expires, anyone may call `execute`.  The
//!    proposal payload is emitted as an event for off-chain execution.
//! 5. **Cancel** – the proposer (or admin) may cancel before execution.
//!
//! ## Quorum
//! `for_votes + against_votes + abstain_votes >= quorum_votes` must hold.
//!
//! ## Delegation
//! Delegation is single-level: if A delegates to B, B's effective power is
//! `B_own + A_own`.  B cannot re-delegate A's power.

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, symbol_short,
    Address, Bytes, Env,
};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

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
    /// Governance parameters.
    Config,
    /// Monotonic proposal counter.
    ProposalCounter,
    /// Proposal data.
    Proposal(u64),
    /// Voting record: (proposal_id, voter) → VoteChoice.
    VoteRecord(u64, Address),
    /// Voting power assigned to an address (base units, set by admin).
    VotingPower(Address),
    /// Delegation target: delegator → delegatee Address.
    DelegateTarget(Address),
    /// Accumulated delegated power received: delegatee → i128.
    DelegatedPower(Address),
}

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Governance configuration parameters.
#[contracttype]
#[derive(Clone)]
pub struct GovernanceConfig {
    /// Ledgers a proposal stays open for voting.
    pub voting_period_ledgers: u32,
    /// Ledgers between queue and execute (time-lock).
    pub timelock_ledgers: u32,
    /// Minimum total votes (for + against + abstain) for a proposal to be valid.
    pub quorum_votes: i128,
    /// Minimum voting power required to create a proposal.
    pub proposal_threshold: i128,
}

/// Proposal state machine.
#[contracttype]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum ProposalStatus {
    Active = 1,
    Defeated = 2,
    Succeeded = 3,
    Queued = 4,
    Executed = 5,
    Cancelled = 6,
}

/// A governance proposal.
#[contracttype]
#[derive(Clone)]
pub struct Proposal {
    pub id: u64,
    pub proposer: Address,
    /// Arbitrary payload describing the parameter change (interpreted off-chain).
    pub payload: Bytes,
    /// Human-readable description (stored as raw bytes to stay `no_std`).
    pub description: Bytes,
    /// Ledger at which voting ends.
    pub vote_end_ledger: u32,
    /// Ledger after which the proposal may be executed (set when queued).
    pub execute_after_ledger: u32,
    pub for_votes: i128,
    pub against_votes: i128,
    pub abstain_votes: i128,
    pub status: ProposalStatus,
}

/// Vote choice.
#[contracttype]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum VoteChoice {
    For = 1,
    Against = 2,
    Abstain = 3,
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[contracterror]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum GovError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    ProposalNotFound = 4,
    ProposalNotActive = 5,
    VotingPeriodEnded = 6,
    VotingPeriodNotEnded = 7,
    AlreadyVoted = 8,
    QuorumNotMet = 9,
    ProposalDefeated = 10,
    TimelockNotExpired = 11,
    ProposalNotQueued = 12,
    InsufficientVotingPower = 13,
    InvalidConfig = 14,
    CannotDelegateToSelf = 15,
    AlreadyDelegated = 16,
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
        .get(&Key::Admin)
        .unwrap_or_else(|| panic_with_error!(env, GovError::NotInitialized));
    admin.require_auth();
    admin
}

fn load_config(env: &Env) -> GovernanceConfig {
    env.storage()
        .instance()
        .get(&Key::Config)
        .unwrap_or_else(|| panic_with_error!(env, GovError::NotInitialized))
}

fn load_proposal(env: &Env, proposal_id: u64) -> Proposal {
    env.storage()
        .persistent()
        .get(&Key::Proposal(proposal_id))
        .unwrap_or_else(|| panic_with_error!(env, GovError::ProposalNotFound))
}

fn save_proposal(env: &Env, proposal: &Proposal) {
    env.storage()
        .persistent()
        .set(&Key::Proposal(proposal.id), proposal);
    bump_persistent(env, &Key::Proposal(proposal.id));
}

/// Returns the effective voting power of `addr`, including any power delegated to it.
/// Delegation is single-level: we only count direct delegations to `addr`.
fn effective_voting_power(env: &Env, addr: &Address) -> i128 {
    // Own base power.
    let own: i128 = env
        .storage()
        .persistent()
        .get(&Key::VotingPower(addr.clone()))
        .unwrap_or(0);

    // Accumulated power delegated to this address by others.
    let delegated: i128 = env
        .storage()
        .persistent()
        .get(&Key::DelegatedPower(addr.clone()))
        .unwrap_or(0);

    own + delegated
}

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

#[contract]
pub struct GovernanceContract;

#[contractimpl]
impl GovernanceContract {
    // -----------------------------------------------------------------------
    // Initialisation
    // -----------------------------------------------------------------------

    /// Initialise the governance contract.
    ///
    /// * `admin`  – address that manages voting power and config
    /// * `config` – initial governance parameters
    pub fn initialize(env: Env, admin: Address, config: GovernanceConfig) {
        if env.storage().instance().has(&Key::Admin) {
            panic_with_error!(env, GovError::AlreadyInitialized);
        }
        admin.require_auth();

        if config.voting_period_ledgers == 0
            || config.timelock_ledgers == 0
            || config.quorum_votes <= 0
            || config.proposal_threshold <= 0
        {
            panic_with_error!(env, GovError::InvalidConfig);
        }

        env.storage().instance().set(&Key::Admin, &admin);
        env.storage().instance().set(&Key::Config, &config);
        env.storage().instance().set(&Key::ProposalCounter, &0u64);
        bump_instance(&env);
    }

    // -----------------------------------------------------------------------
    // Admin: voting power management
    // -----------------------------------------------------------------------

    /// Assign (or update) the base voting power for an address.
    /// In a token-based system this would be derived from token balances;
    /// here the admin sets it explicitly to keep the contract self-contained.
    pub fn set_voting_power(env: Env, account: Address, power: i128) {
        require_admin(&env);
        bump_instance(&env);

        env.storage()
            .persistent()
            .set(&Key::VotingPower(account.clone()), &power);
        bump_persistent(&env, &Key::VotingPower(account.clone()));

        env.events().publish(
            (symbol_short!("gov"), symbol_short!("vp_set")),
            (account, power),
        );
    }

    /// Update governance configuration parameters (admin only).
    pub fn update_config(env: Env, config: GovernanceConfig) {
        require_admin(&env);
        bump_instance(&env);

        if config.voting_period_ledgers == 0
            || config.timelock_ledgers == 0
            || config.quorum_votes <= 0
            || config.proposal_threshold <= 0
        {
            panic_with_error!(env, GovError::InvalidConfig);
        }

        env.storage().instance().set(&Key::Config, &config);
        env.events()
            .publish((symbol_short!("gov"), symbol_short!("cfg_upd")), ());
    }

    // -----------------------------------------------------------------------
    // Delegation
    // -----------------------------------------------------------------------

    /// Delegate your voting power to `delegatee`.
    ///
    /// The delegator's own power is added to the delegatee's accumulated
    /// delegated total.  A delegator may only have one active delegation at a
    /// time; call `undelegate` first to change.
    pub fn delegate(env: Env, delegator: Address, delegatee: Address) {
        delegator.require_auth();
        bump_instance(&env);

        if delegator == delegatee {
            panic_with_error!(env, GovError::CannotDelegateToSelf);
        }

        // Prevent double-delegation without undelegating first.
        if env
            .storage()
            .persistent()
            .has(&Key::DelegateTarget(delegator.clone()))
        {
            panic_with_error!(env, GovError::AlreadyDelegated);
        }

        let delegator_power: i128 = env
            .storage()
            .persistent()
            .get(&Key::VotingPower(delegator.clone()))
            .unwrap_or(0);

        // Record who the delegator is delegating to.
        env.storage()
            .persistent()
            .set(&Key::DelegateTarget(delegator.clone()), &delegatee);
        bump_persistent(&env, &Key::DelegateTarget(delegator.clone()));

        // Accumulate delegated power on the delegatee.
        let current_delegated: i128 = env
            .storage()
            .persistent()
            .get(&Key::DelegatedPower(delegatee.clone()))
            .unwrap_or(0i128);
        let new_delegated = current_delegated + delegator_power;
        env.storage()
            .persistent()
            .set(&Key::DelegatedPower(delegatee.clone()), &new_delegated);
        bump_persistent(&env, &Key::DelegatedPower(delegatee.clone()));

        env.events().publish(
            (symbol_short!("gov"), symbol_short!("delegated")),
            (delegator, delegatee, delegator_power),
        );
    }

    /// Remove an active delegation, returning the delegator's power to themselves.
    pub fn undelegate(env: Env, delegator: Address) {
        delegator.require_auth();
        bump_instance(&env);

        let delegatee: Address = env
            .storage()
            .persistent()
            .get(&Key::DelegateTarget(delegator.clone()))
            .unwrap_or_else(|| panic_with_error!(env, GovError::ProposalNotFound));

        let delegator_power: i128 = env
            .storage()
            .persistent()
            .get(&Key::VotingPower(delegator.clone()))
            .unwrap_or(0);

        // Remove the delegation record.
        env.storage()
            .persistent()
            .remove(&Key::DelegateTarget(delegator.clone()));

        // Subtract from delegatee's accumulated power.
        let current_delegated: i128 = env
            .storage()
            .persistent()
            .get(&Key::DelegatedPower(delegatee.clone()))
            .unwrap_or(0i128);
        let new_delegated = (current_delegated - delegator_power).max(0);
        env.storage()
            .persistent()
            .set(&Key::DelegatedPower(delegatee.clone()), &new_delegated);
        bump_persistent(&env, &Key::DelegatedPower(delegatee.clone()));

        env.events().publish(
            (symbol_short!("gov"), symbol_short!("undelegat")),
            (delegator, delegatee),
        );
    }

    // -----------------------------------------------------------------------
    // Proposal lifecycle
    // -----------------------------------------------------------------------

    /// Create a new governance proposal.
    ///
    /// * `proposer`     – must have voting power ≥ `proposal_threshold`
    /// * `payload`      – encoded parameter change (interpreted off-chain)
    /// * `description`  – human-readable description bytes
    ///
    /// Returns the new proposal ID.
    pub fn propose(
        env: Env,
        proposer: Address,
        payload: Bytes,
        description: Bytes,
    ) -> u64 {
        proposer.require_auth();
        bump_instance(&env);

        let config = load_config(&env);
        let power = effective_voting_power(&env, &proposer);

        if power < config.proposal_threshold {
            panic_with_error!(env, GovError::InsufficientVotingPower);
        }

        let proposal_id: u64 = env
            .storage()
            .instance()
            .get(&Key::ProposalCounter)
            .unwrap_or(0);
        let next_id = proposal_id + 1;

        let current_ledger = env.ledger().sequence();
        let proposal = Proposal {
            id: next_id,
            proposer: proposer.clone(),
            payload,
            description,
            vote_end_ledger: current_ledger + config.voting_period_ledgers,
            execute_after_ledger: 0,
            for_votes: 0,
            against_votes: 0,
            abstain_votes: 0,
            status: ProposalStatus::Active,
        };

        save_proposal(&env, &proposal);
        env.storage().instance().set(&Key::ProposalCounter, &next_id);

        env.events().publish(
            (symbol_short!("gov"), symbol_short!("proposed")),
            (next_id, proposer, current_ledger + config.voting_period_ledgers),
        );

        next_id
    }

    /// Cast a vote on an active proposal.
    ///
    /// * `voter`       – must sign; uses their effective voting power
    /// * `proposal_id` – target proposal
    /// * `choice`      – For / Against / Abstain
    pub fn vote(env: Env, voter: Address, proposal_id: u64, choice: VoteChoice) {
        voter.require_auth();
        bump_instance(&env);

        let mut proposal = load_proposal(&env, proposal_id);

        if proposal.status != ProposalStatus::Active {
            panic_with_error!(env, GovError::ProposalNotActive);
        }

        let current_ledger = env.ledger().sequence();
        if current_ledger > proposal.vote_end_ledger {
            panic_with_error!(env, GovError::VotingPeriodEnded);
        }

        // Prevent double voting.
        let vote_key = Key::VoteRecord(proposal_id, voter.clone());
        if env.storage().persistent().has(&vote_key) {
            panic_with_error!(env, GovError::AlreadyVoted);
        }

        let power = effective_voting_power(&env, &voter);

        match choice {
            VoteChoice::For => proposal.for_votes += power,
            VoteChoice::Against => proposal.against_votes += power,
            VoteChoice::Abstain => proposal.abstain_votes += power,
        }

        // Record the vote to prevent double-voting.
        env.storage().persistent().set(&vote_key, &choice);
        bump_persistent(&env, &vote_key);

        save_proposal(&env, &proposal);

        env.events().publish(
            (symbol_short!("gov"), symbol_short!("voted")),
            (proposal_id, voter, choice as u32, power),
        );
    }

    /// Queue a succeeded proposal for time-locked execution.
    ///
    /// Can be called by anyone once the voting period has ended and the
    /// proposal has passed (For > Against, quorum met).
    pub fn queue(env: Env, proposal_id: u64) {
        bump_instance(&env);

        let mut proposal = load_proposal(&env, proposal_id);
        let config = load_config(&env);
        let current_ledger = env.ledger().sequence();

        if proposal.status != ProposalStatus::Active {
            panic_with_error!(env, GovError::ProposalNotActive);
        }
        if current_ledger <= proposal.vote_end_ledger {
            panic_with_error!(env, GovError::VotingPeriodNotEnded);
        }

        // Check quorum.
        let total_votes =
            proposal.for_votes + proposal.against_votes + proposal.abstain_votes;
        if total_votes < config.quorum_votes {
            panic_with_error!(env, GovError::QuorumNotMet);
        }

        // Check majority.
        if proposal.for_votes <= proposal.against_votes {
            panic_with_error!(env, GovError::ProposalDefeated);
        }
        proposal.status = ProposalStatus::Queued;
        proposal.execute_after_ledger = current_ledger + config.timelock_ledgers;
        save_proposal(&env, &proposal);

        env.events().publish(
            (symbol_short!("gov"), symbol_short!("queued")),
            (proposal_id, proposal.execute_after_ledger),
        );
    }

    /// Execute a queued proposal after the time-lock has expired.
    ///
    /// Emits the proposal payload as an event for off-chain execution.
    /// Can be called by anyone.
    pub fn execute(env: Env, proposal_id: u64) {
        bump_instance(&env);

        let mut proposal = load_proposal(&env, proposal_id);
        let current_ledger = env.ledger().sequence();

        if proposal.status != ProposalStatus::Queued {
            panic_with_error!(env, GovError::ProposalNotQueued);
        }
        if current_ledger < proposal.execute_after_ledger {
            panic_with_error!(env, GovError::TimelockNotExpired);
        }

        proposal.status = ProposalStatus::Executed;
        save_proposal(&env, &proposal);

        env.events().publish(
            (symbol_short!("gov"), symbol_short!("executed")),
            (proposal_id, proposal.payload.clone()),
        );
    }

    /// Cancel a proposal. Only the proposer or admin may cancel.
    pub fn cancel(env: Env, caller: Address, proposal_id: u64) {
        caller.require_auth();
        bump_instance(&env);

        let mut proposal = load_proposal(&env, proposal_id);

        // Only proposer or admin may cancel.
        let admin: Address = env
            .storage()
            .instance()
            .get(&Key::Admin)
            .unwrap_or_else(|| panic_with_error!(env, GovError::NotInitialized));

        if caller != proposal.proposer && caller != admin {
            panic_with_error!(env, GovError::Unauthorized);
        }

        if proposal.status == ProposalStatus::Executed
            || proposal.status == ProposalStatus::Cancelled
        {
            panic_with_error!(env, GovError::ProposalNotActive);
        }

        proposal.status = ProposalStatus::Cancelled;
        save_proposal(&env, &proposal);

        env.events().publish(
            (symbol_short!("gov"), symbol_short!("cancelled")),
            (proposal_id, caller),
        );
    }

    // -----------------------------------------------------------------------
    // Admin: transfer admin / upgrade
    // -----------------------------------------------------------------------

    pub fn transfer_admin(env: Env, new_admin: Address) {
        require_admin(&env);
        env.storage().instance().set(&Key::Admin, &new_admin);
        env.events().publish(
            (symbol_short!("gov"), symbol_short!("adm_xfer")),
            new_admin,
        );
    }

    pub fn upgrade(env: Env, new_wasm_hash: soroban_sdk::BytesN<32>) {
        require_admin(&env);
        env.deployer().update_current_contract_wasm(new_wasm_hash);
    }

    // -----------------------------------------------------------------------
    // Views
    // -----------------------------------------------------------------------

    pub fn get_proposal(env: Env, proposal_id: u64) -> Proposal {
        load_proposal(&env, proposal_id)
    }

    pub fn get_voting_power(env: Env, account: Address) -> i128 {
        effective_voting_power(&env, &account)
    }

    pub fn get_config(env: Env) -> GovernanceConfig {
        load_config(&env)
    }

    pub fn get_admin(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&Key::Admin)
            .unwrap_or_else(|| panic_with_error!(env, GovError::NotInitialized))
    }

    pub fn proposal_count(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&Key::ProposalCounter)
            .unwrap_or(0)
    }

    /// Returns the vote choice cast by `voter` on `proposal_id`, if any.
    pub fn get_vote(env: Env, proposal_id: u64, voter: Address) -> Option<VoteChoice> {
        env.storage()
            .persistent()
            .get(&Key::VoteRecord(proposal_id, voter))
    }

    // -----------------------------------------------------------------------
    // Advanced Governance Features
    // -----------------------------------------------------------------------

    /// Create a proposal with emergency fast-track option.
    /// Emergency proposals have a shorter voting period and higher quorum.
    pub fn propose_emergency(
        env: Env,
        proposer: Address,
        payload: Bytes,
        description: Bytes,
    ) -> u64 {
        proposer.require_auth();
        bump_instance(&env);

        let config = load_config(&env);
        let power = effective_voting_power(&env, &proposer);

        // Emergency proposals require 2x the normal threshold
        if power < config.proposal_threshold * 2 {
            panic_with_error!(env, GovError::InsufficientVotingPower);
        }

        let proposal_id: u64 = env
            .storage()
            .instance()
            .get(&Key::ProposalCounter)
            .unwrap_or(0);
        let next_id = proposal_id + 1;

        let current_ledger = env.ledger().sequence();
        // Emergency proposals have half the voting period
        let emergency_voting_period = config.voting_period_ledgers / 2;

        let proposal = Proposal {
            id: next_id,
            proposer: proposer.clone(),
            payload,
            description,
            vote_end_ledger: current_ledger + emergency_voting_period,
            execute_after_ledger: 0,
            for_votes: 0,
            against_votes: 0,
            abstain_votes: 0,
            status: ProposalStatus::Active,
        };

        save_proposal(&env, &proposal);
        env.storage().instance().set(&Key::ProposalCounter, &next_id);

        env.events().publish(
            (symbol_short!("gov"), symbol_short!("emg_prop")),
            (next_id, proposer, emergency_voting_period),
        );

        next_id
    }

    /// Vote with delegation chain support. Allows voting on behalf of delegated power.
    pub fn vote_delegated(
        env: Env,
        voter: Address,
        proposal_id: u64,
        choice: VoteChoice,
        delegated_voters: soroban_sdk::Vec<Address>,
    ) {
        voter.require_auth();
        bump_instance(&env);

        let mut proposal = load_proposal(&env, proposal_id);

        if proposal.status != ProposalStatus::Active {
            panic_with_error!(env, GovError::ProposalNotActive);
        }

        let current_ledger = env.ledger().sequence();
        if current_ledger > proposal.vote_end_ledger {
            panic_with_error!(env, GovError::VotingPeriodEnded);
        }

        // Prevent double voting.
        let vote_key = Key::VoteRecord(proposal_id, voter.clone());
        if env.storage().persistent().has(&vote_key) {
            panic_with_error!(env, GovError::AlreadyVoted);
        }

        let mut total_power = effective_voting_power(&env, &voter);

        // Add power from delegated voters
        for delegated in delegated_voters.iter() {
            let delegated_power: i128 = env
                .storage()
                .persistent()
                .get(&Key::VotingPower(delegated.clone()))
                .unwrap_or(0);
            total_power += delegated_power;
        }

        match choice {
            VoteChoice::For => proposal.for_votes += total_power,
            VoteChoice::Against => proposal.against_votes += total_power,
            VoteChoice::Abstain => proposal.abstain_votes += total_power,
        }

        env.storage().persistent().set(&vote_key, &choice);
        bump_persistent(&env, &vote_key);

        save_proposal(&env, &proposal);

        env.events().publish(
            (symbol_short!("gov"), symbol_short!("v_deleg")),
            (proposal_id, voter, choice as u32, total_power),
        );
    }

    /// Create a proposal with veto power for admin (emergency override).
    pub fn veto_proposal(env: Env, proposal_id: u64) {
        require_admin(&env);
        bump_instance(&env);

        let mut proposal = load_proposal(&env, proposal_id);

        if proposal.status == ProposalStatus::Executed
            || proposal.status == ProposalStatus::Cancelled
        {
            panic_with_error!(env, GovError::ProposalNotActive);
        }

        proposal.status = ProposalStatus::Cancelled;
        save_proposal(&env, &proposal);

        env.events().publish(
            (symbol_short!("gov"), symbol_short!("vetoed")),
            (proposal_id,),
        );
    }

    /// Batch vote on multiple proposals (gas optimization).
    pub fn batch_vote(
        env: Env,
        voter: Address,
        proposal_ids: soroban_sdk::Vec<u64>,
        choices: soroban_sdk::Vec<VoteChoice>,
    ) {
        voter.require_auth();
        bump_instance(&env);

        if proposal_ids.len() != choices.len() {
            panic_with_error!(env, GovError::InvalidConfig);
        }

        for i in 0..proposal_ids.len() {
            let proposal_id = proposal_ids.get(i).unwrap();
            let choice = choices.get(i).unwrap();

            let mut proposal = load_proposal(&env, proposal_id);

            if proposal.status != ProposalStatus::Active {
                panic_with_error!(env, GovError::ProposalNotActive);
            }

            let current_ledger = env.ledger().sequence();
            if current_ledger > proposal.vote_end_ledger {
                panic_with_error!(env, GovError::VotingPeriodEnded);
            }

            let vote_key = Key::VoteRecord(proposal_id, voter.clone());
            if env.storage().persistent().has(&vote_key) {
                panic_with_error!(env, GovError::AlreadyVoted);
            }

            let power = effective_voting_power(&env, &voter);

            match choice {
                VoteChoice::For => proposal.for_votes += power,
                VoteChoice::Against => proposal.against_votes += power,
                VoteChoice::Abstain => proposal.abstain_votes += power,
            }

            env.storage().persistent().set(&vote_key, &choice);
            bump_persistent(&env, &vote_key);
            save_proposal(&env, &proposal);
        }

        env.events().publish(
            (symbol_short!("gov"), symbol_short!("batch_v")),
            (voter, proposal_ids.len() as u32),
        );
    }

    /// Get proposal voting statistics.
    pub fn get_proposal_stats(env: Env, proposal_id: u64) -> (i128, i128, i128, i128) {
        let proposal = load_proposal(&env, proposal_id);
        let config = load_config(&env);
        let total_votes = proposal.for_votes + proposal.against_votes + proposal.abstain_votes;
        (proposal.for_votes, proposal.against_votes, proposal.abstain_votes, total_votes)
    }

    /// Check if a proposal would pass based on current votes.
    pub fn would_pass(env: Env, proposal_id: u64) -> bool {
        let proposal = load_proposal(&env, proposal_id);
        let config = load_config(&env);
        let total_votes = proposal.for_votes + proposal.against_votes + proposal.abstain_votes;

        total_votes >= config.quorum_votes && proposal.for_votes > proposal.against_votes
    }

    /// Get voting power breakdown for an address.
    pub fn get_power_breakdown(env: Env, account: Address) -> (i128, i128) {
        let own: i128 = env
            .storage()
            .persistent()
            .get(&Key::VotingPower(account.clone()))
            .unwrap_or(0);

        let delegated: i128 = env
            .storage()
            .persistent()
            .get(&Key::DelegatedPower(account.clone()))
            .unwrap_or(0);

        (own, delegated)
    }
}

mod test;
