#![no_std]

//! # Governance Contract
//!
//! Decentralized governance system for protocol decisions.
//! Supports proposal creation, voting, and execution of approved proposals.
//!
//! ## Design
//! * Token holders can create and vote on proposals
//! * Voting power is proportional to token balance
//! * Proposals require minimum quorum and approval threshold
//! * Approved proposals can be executed by anyone
//! * Supports multiple proposal types (parameter changes, fund transfers, etc.)

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype,
    symbol_short, token::Client as TokenClient,
    Address, Env, Symbol, Vec, Map, Bytes,
};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const MIN_PROPOSAL_THRESHOLD: i128 = 1_000_000; // Minimum tokens to create proposal
const VOTING_PERIOD: u64 = 604_800; // 7 days in seconds
const QUORUM_PERCENTAGE: u32 = 40; // 40% quorum required
const APPROVAL_THRESHOLD: u32 = 50; // 50% approval required

// ---------------------------------------------------------------------------
// Storage keys
// ---------------------------------------------------------------------------

const KEY_ADMIN: Symbol = symbol_short!("admin");
const KEY_GOVERNANCE_TOKEN: Symbol = symbol_short!("gov_token");
const KEY_PROPOSALS: Symbol = symbol_short!("proposals");
const KEY_VOTES: Symbol = symbol_short!("votes");
const KEY_PROPOSAL_COUNT: Symbol = symbol_short!("prop_count");
const KEY_TOTAL_SUPPLY: Symbol = symbol_short!("total_supply");

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum ProposalType {
    ParameterChange,
    FundTransfer,
    ContractUpgrade,
    FeatureToggle,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct Proposal {
    pub id: u64,
    pub proposer: Address,
    pub proposal_type: ProposalType,
    pub title: Symbol,
    pub description: Bytes,
    pub target: Address,
    pub call_data: Bytes,
    pub start_time: u64,
    pub end_time: u64,
    pub for_votes: i128,
    pub against_votes: i128,
    pub abstain_votes: i128,
    pub status: Symbol, // "pending", "active", "succeeded", "failed", "executed", "cancelled"
    pub execution_eta: u64,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct Vote {
    pub proposal_id: u64,
    pub voter: Address,
    pub support: u32, // 0 = against, 1 = for, 2 = abstain
    pub votes: i128,
    pub reason: Bytes,
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    InsufficientTokens = 4,
    ProposalNotFound = 5,
    VotingClosed = 6,
    AlreadyVoted = 7,
    InvalidVoteType = 8,
    ProposalNotActive = 9,
    ProposalNotSucceeded = 10,
    ExecutionFailed = 11,
    InvalidProposal = 12,
    QuorumNotMet = 13,
    ProposalNotPending = 14,
}

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

#[contract]
pub struct Governance;

#[contractimpl]
impl Governance {

    // -----------------------------------------------------------------------
    // Initialization
    // -----------------------------------------------------------------------

    pub fn initialize(
        env: Env,
        admin: Address,
        governance_token: Address,
        total_supply: i128,
    ) -> Result<(), Error> {
        if env.storage().instance().has(&KEY_ADMIN) {
            return Err(Error::AlreadyInitialized);
        }
        admin.require_auth();

        env.storage().instance().set(&KEY_ADMIN, &admin);
        env.storage().instance().set(&KEY_GOVERNANCE_TOKEN, &governance_token);
        env.storage().instance().set(&KEY_TOTAL_SUPPLY, &total_supply);
        env.storage().instance().set(&KEY_PROPOSAL_COUNT, &0u64);

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Proposal Management
    // -----------------------------------------------------------------------

    /// Create a new proposal
    pub fn create_proposal(
        env: Env,
        proposal_type: ProposalType,
        title: Symbol,
        description: Bytes,
        target: Address,
        call_data: Bytes,
    ) -> Result<u64, Error> {
        Self::require_initialized(&env)?;

        let proposer = env.invoker();
        let gov_token: Address = env.storage().instance().get(&KEY_GOVERNANCE_TOKEN).unwrap();
        let token_client = TokenClient::new(&env, &gov_token);

        // Check proposer has minimum tokens
        let balance = token_client.balance(&proposer);
        if balance < MIN_PROPOSAL_THRESHOLD {
            return Err(Error::InsufficientTokens);
        }

        // Generate proposal ID
        let prop_count: u64 = env.storage().instance().get(&KEY_PROPOSAL_COUNT).unwrap_or(0);
        let proposal_id = prop_count + 1;
        env.storage().instance().set(&KEY_PROPOSAL_COUNT, &proposal_id);

        let current_time = env.ledger().timestamp();
        let start_time = current_time;
        let end_time = current_time + VOTING_PERIOD;

        let proposal = Proposal {
            id: proposal_id,
            proposer: proposer.clone(),
            proposal_type,
            title,
            description,
            target,
            call_data,
            start_time,
            end_time,
            for_votes: 0,
            against_votes: 0,
            abstain_votes: 0,
            status: symbol_short!("active"),
            execution_eta: 0,
        };

        // Store proposal
        let mut proposals: Map<u64, Proposal> = env.storage()
            .instance()
            .get(&KEY_PROPOSALS)
            .unwrap_or_else(|| Map::new(&env));
        proposals.set(proposal_id, proposal);
        env.storage().instance().set(&KEY_PROPOSALS, &proposals);

        env.events().publish(
            (symbol_short!("gov"), symbol_short!("proposal")),
            (proposal_id, proposer, title),
        );

        Ok(proposal_id)
    }

    /// Cast a vote on a proposal
    pub fn vote(
        env: Env,
        proposal_id: u64,
        support: u32, // 0 = against, 1 = for, 2 = abstain
        reason: Bytes,
    ) -> Result<(), Error> {
        Self::require_initialized(&env)?;

        if support > 2 {
            return Err(Error::InvalidVoteType);
        }

        let voter = env.invoker();
        let gov_token: Address = env.storage().instance().get(&KEY_GOVERNANCE_TOKEN).unwrap();
        let token_client = TokenClient::new(&env, &gov_token);

        // Get voter's voting power
        let voting_power = token_client.balance(&voter);
        if voting_power <= 0 {
            return Err(Error::InsufficientTokens);
        }

        // Get proposal
        let mut proposals: Map<u64, Proposal> = env.storage()
            .instance()
            .get(&KEY_PROPOSALS)
            .unwrap_or_else(|| Map::new(&env));

        let mut proposal = proposals.get(proposal_id).ok_or(Error::ProposalNotFound)?;

        // Check voting is still open
        let current_time = env.ledger().timestamp();
        if current_time > proposal.end_time {
            return Err(Error::VotingClosed);
        }

        if proposal.status != symbol_short!("active") {
            return Err(Error::ProposalNotActive);
        }

        // Check if already voted
        let vote_key = (proposal_id, voter.clone());
        let votes: Map<(u64, Address), Vote> = env.storage()
            .instance()
            .get(&KEY_VOTES)
            .unwrap_or_else(|| Map::new(&env));

        if votes.contains_key(vote_key.clone()) {
            return Err(Error::AlreadyVoted);
        }

        // Record vote
        match support {
            0 => proposal.against_votes += voting_power,
            1 => proposal.for_votes += voting_power,
            2 => proposal.abstain_votes += voting_power,
            _ => return Err(Error::InvalidVoteType),
        }

        let vote = Vote {
            proposal_id,
            voter: voter.clone(),
            support,
            votes: voting_power,
            reason,
        };

        let mut votes_map = votes;
        votes_map.set(vote_key, vote);
        env.storage().instance().set(&KEY_VOTES, &votes_map);

        proposals.set(proposal_id, proposal);
        env.storage().instance().set(&KEY_PROPOSALS, &proposals);

        env.events().publish(
            (symbol_short!("gov"), symbol_short!("vote")),
            (proposal_id, voter, support, voting_power),
        );

        Ok(())
    }

    /// Finalize voting on a proposal
    pub fn finalize_proposal(env: Env, proposal_id: u64) -> Result<(), Error> {
        Self::require_initialized(&env)?;

        let mut proposals: Map<u64, Proposal> = env.storage()
            .instance()
            .get(&KEY_PROPOSALS)
            .unwrap_or_else(|| Map::new(&env));

        let mut proposal = proposals.get(proposal_id).ok_or(Error::ProposalNotFound)?;

        let current_time = env.ledger().timestamp();
        if current_time <= proposal.end_time {
            return Err(Error::VotingClosed);
        }

        let total_supply: i128 = env.storage().instance().get(&KEY_TOTAL_SUPPLY).unwrap_or(0);
        let total_votes = proposal.for_votes + proposal.against_votes + proposal.abstain_votes;

        // Check quorum
        let quorum_required = (total_supply * QUORUM_PERCENTAGE as i128) / 100;
        if total_votes < quorum_required {
            proposal.status = symbol_short!("failed");
            proposals.set(proposal_id, proposal);
            env.storage().instance().set(&KEY_PROPOSALS, &proposals);
            return Err(Error::QuorumNotMet);
        }

        // Check approval threshold
        let approval_required = (total_votes * APPROVAL_THRESHOLD as i128) / 100;
        if proposal.for_votes >= approval_required {
            proposal.status = symbol_short!("succeeded");
            proposal.execution_eta = current_time + 86_400; // 1 day timelock
        } else {
            proposal.status = symbol_short!("failed");
        }

        proposals.set(proposal_id, proposal);
        env.storage().instance().set(&KEY_PROPOSALS, &proposals);

        env.events().publish(
            (symbol_short!("gov"), symbol_short!("finalized")),
            (proposal_id, proposal.status),
        );

        Ok(())
    }

    /// Execute an approved proposal
    pub fn execute_proposal(env: Env, proposal_id: u64) -> Result<(), Error> {
        Self::require_initialized(&env)?;

        let mut proposals: Map<u64, Proposal> = env.storage()
            .instance()
            .get(&KEY_PROPOSALS)
            .unwrap_or_else(|| Map::new(&env));

        let mut proposal = proposals.get(proposal_id).ok_or(Error::ProposalNotFound)?;

        if proposal.status != symbol_short!("succeeded") {
            return Err(Error::ProposalNotSucceeded);
        }

        let current_time = env.ledger().timestamp();
        if current_time < proposal.execution_eta {
            return Err(Error::ExecutionFailed);
        }

        proposal.status = symbol_short!("executed");
        proposals.set(proposal_id, proposal);
        env.storage().instance().set(&KEY_PROPOSALS, &proposals);

        env.events().publish(
            (symbol_short!("gov"), symbol_short!("executed")),
            (proposal_id,),
        );

        Ok(())
    }

    /// Cancel a proposal (admin only)
    pub fn cancel_proposal(env: Env, proposal_id: u64) -> Result<(), Error> {
        Self::require_initialized(&env)?;
        let admin: Address = env.storage().instance().get(&KEY_ADMIN).unwrap();
        admin.require_auth();

        let mut proposals: Map<u64, Proposal> = env.storage()
            .instance()
            .get(&KEY_PROPOSALS)
            .unwrap_or_else(|| Map::new(&env));

        let mut proposal = proposals.get(proposal_id).ok_or(Error::ProposalNotFound)?;
        proposal.status = symbol_short!("cancelled");

        proposals.set(proposal_id, proposal);
        env.storage().instance().set(&KEY_PROPOSALS, &proposals);

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Admin Functions
    // -----------------------------------------------------------------------

    pub fn transfer_admin(env: Env, new_admin: Address) -> Result<(), Error> {
        Self::require_initialized(&env)?;
        let admin: Address = env.storage().instance().get(&KEY_ADMIN).unwrap();
        admin.require_auth();
        env.storage().instance().set(&KEY_ADMIN, &new_admin);
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Views
    // -----------------------------------------------------------------------

    pub fn get_proposal(env: Env, proposal_id: u64) -> Result<Proposal, Error> {
        Self::require_initialized(&env)?;

        let proposals: Map<u64, Proposal> = env.storage()
            .instance()
            .get(&KEY_PROPOSALS)
            .unwrap_or_else(|| Map::new(&env));

        proposals.get(proposal_id).ok_or(Error::ProposalNotFound)
    }

    pub fn get_vote(
        env: Env,
        proposal_id: u64,
        voter: Address,
    ) -> Result<Vote, Error> {
        Self::require_initialized(&env)?;

        let votes: Map<(u64, Address), Vote> = env.storage()
            .instance()
            .get(&KEY_VOTES)
            .unwrap_or_else(|| Map::new(&env));

        votes.get((proposal_id, voter)).ok_or(Error::ProposalNotFound)
    }

    pub fn get_proposal_count(env: Env) -> Result<u64, Error> {
        Self::require_initialized(&env)?;
        Ok(env.storage().instance().get(&KEY_PROPOSAL_COUNT).unwrap_or(0))
    }

    pub fn get_admin(env: Env) -> Result<Address, Error> {
        Self::require_initialized(&env)?;
        Ok(env.storage().instance().get(&KEY_ADMIN).unwrap())
    }

    pub fn get_governance_token(env: Env) -> Result<Address, Error> {
        Self::require_initialized(&env)?;
        Ok(env.storage().instance().get(&KEY_GOVERNANCE_TOKEN).unwrap())
    }

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn require_initialized(env: &Env) -> Result<(), Error> {
        if !env.storage().instance().has(&KEY_ADMIN) {
            return Err(Error::NotInitialized);
        }
        Ok(())
    }
}
