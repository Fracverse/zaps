#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Bytes, Env, Error as SdkError,
};

fn sdk_err(e: GovError) -> SdkError {
    SdkError::from_contract_error(e as u32)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn default_config(env: &Env) -> GovernanceConfig {
    GovernanceConfig {
        voting_period_ledgers: 100,
        timelock_ledgers: 50,
        quorum_votes: 10,
        proposal_threshold: 5,
    }
}

fn payload(env: &Env) -> Bytes {
    Bytes::from_slice(env, b"set_fee:50")
}

fn description(env: &Env) -> Bytes {
    Bytes::from_slice(env, b"Reduce protocol fee to 50 bps")
}

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

struct Setup {
    env: Env,
    client: GovernanceContractClient<'static>,
    admin: Address,
    /// Voter with power 10 (meets quorum alone).
    voter_a: Address,
    /// Voter with power 5.
    voter_b: Address,
}

impl Setup {
    fn new() -> Self {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let voter_a = Address::generate(&env);
        let voter_b = Address::generate(&env);

        let contract_id = env.register_contract(None, GovernanceContract);
        let client = GovernanceContractClient::new(&env, &contract_id);
        client.initialize(&admin, &default_config(&env));
        client.set_voting_power(&voter_a, &10i128);
        client.set_voting_power(&voter_b, &5i128);

        let client: GovernanceContractClient<'static> =
            unsafe { core::mem::transmute(client) };

        Setup {
            env,
            client,
            admin,
            voter_a,
            voter_b,
        }
    }

    /// Create a proposal from voter_a and return its ID.
    fn create_proposal(&self) -> u64 {
        self.client
            .propose(&self.voter_a, &payload(&self.env), &description(&self.env))
    }
}

// ---------------------------------------------------------------------------
// Initialisation
// ---------------------------------------------------------------------------

#[test]
fn test_double_init_rejected() {
    let s = Setup::new();
    assert_eq!(
        s.client.try_initialize(&s.admin, &default_config(&s.env)),
        Err(Ok(sdk_err(GovError::AlreadyInitialized)))
    );
}

#[test]
fn test_invalid_config_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, GovernanceContract);
    let client = GovernanceContractClient::new(&env, &contract_id);

    let bad_config = GovernanceConfig {
        voting_period_ledgers: 0, // invalid
        timelock_ledgers: 50,
        quorum_votes: 10,
        proposal_threshold: 5,
    };
    assert_eq!(
        client.try_initialize(&admin, &bad_config),
        Err(Ok(sdk_err(GovError::InvalidConfig)))
    );
}

// ---------------------------------------------------------------------------
// Voting power
// ---------------------------------------------------------------------------

#[test]
fn test_set_and_get_voting_power() {
    let s = Setup::new();
    assert_eq!(s.client.get_voting_power(&s.voter_a), 10);
    assert_eq!(s.client.get_voting_power(&s.voter_b), 5);
}

#[test]
fn test_address_with_no_power_has_zero() {
    let s = Setup::new();
    let unknown = Address::generate(&s.env);
    assert_eq!(s.client.get_voting_power(&unknown), 0);
}

// ---------------------------------------------------------------------------
// Delegation
// ---------------------------------------------------------------------------

#[test]
fn test_delegate_increases_delegatee_power() {
    let s = Setup::new();
    // voter_b (5) delegates to voter_a (10) → voter_a effective = 15.
    s.client.delegate(&s.voter_b, &s.voter_a);
    assert_eq!(s.client.get_voting_power(&s.voter_a), 15);
    // voter_b's own power is unchanged.
    assert_eq!(s.client.get_voting_power(&s.voter_b), 5);
}

#[test]
fn test_delegate_to_self_rejected() {
    let s = Setup::new();
    assert_eq!(
        s.client.try_delegate(&s.voter_a, &s.voter_a),
        Err(Ok(sdk_err(GovError::CannotDelegateToSelf)))
    );
}

#[test]
fn test_double_delegation_rejected() {
    let s = Setup::new();
    s.client.delegate(&s.voter_b, &s.voter_a);
    assert_eq!(
        s.client.try_delegate(&s.voter_b, &s.voter_a),
        Err(Ok(sdk_err(GovError::AlreadyDelegated)))
    );
}

#[test]
fn test_undelegate_restores_power() {
    let s = Setup::new();
    s.client.delegate(&s.voter_b, &s.voter_a);
    assert_eq!(s.client.get_voting_power(&s.voter_a), 15);

    s.client.undelegate(&s.voter_b);
    assert_eq!(s.client.get_voting_power(&s.voter_a), 10);
}

#[test]
fn test_get_delegate_returns_delegatee() {
    let s = Setup::new();
    assert_eq!(s.client.get_delegate(&s.voter_b), None);
    s.client.delegate(&s.voter_b, &s.voter_a);
    assert_eq!(s.client.get_delegate(&s.voter_b), Some(s.voter_a.clone()));
}

// ---------------------------------------------------------------------------
// Proposal creation
// ---------------------------------------------------------------------------

#[test]
fn test_propose_creates_proposal() {
    let s = Setup::new();
    let id = s.create_proposal();
    assert_eq!(id, 1);
    let p = s.client.get_proposal(&id);
    assert_eq!(p.proposer, s.voter_a);
    assert_eq!(p.status, ProposalStatus::Active);
    assert_eq!(p.for_votes, 0);
}

#[test]
fn test_propose_insufficient_power_rejected() {
    let s = Setup::new();
    let weak = Address::generate(&s.env);
    s.client.set_voting_power(&weak, &1i128); // below threshold of 5
    assert_eq!(
        s.client
            .try_propose(&weak, &payload(&s.env), &description(&s.env)),
        Err(Ok(sdk_err(GovError::InsufficientVotingPower)))
    );
}

#[test]
fn test_proposal_counter_increments() {
    let s = Setup::new();
    assert_eq!(s.client.proposal_count(), 0);
    s.create_proposal();
    assert_eq!(s.client.proposal_count(), 1);
    s.create_proposal();
    assert_eq!(s.client.proposal_count(), 2);
}

// ---------------------------------------------------------------------------
// Voting
// ---------------------------------------------------------------------------

#[test]
fn test_vote_for_recorded() {
    let s = Setup::new();
    let id = s.create_proposal();
    s.client.vote(&s.voter_a, &id, &VoteChoice::For);
    let p = s.client.get_proposal(&id);
    assert_eq!(p.for_votes, 10);
    assert_eq!(s.client.get_vote(&id, &s.voter_a), Some(VoteChoice::For));
}

#[test]
fn test_vote_against_recorded() {
    let s = Setup::new();
    let id = s.create_proposal();
    s.client.vote(&s.voter_b, &id, &VoteChoice::Against);
    let p = s.client.get_proposal(&id);
    assert_eq!(p.against_votes, 5);
}

#[test]
fn test_vote_abstain_recorded() {
    let s = Setup::new();
    let id = s.create_proposal();
    s.client.vote(&s.voter_a, &id, &VoteChoice::Abstain);
    let p = s.client.get_proposal(&id);
    assert_eq!(p.abstain_votes, 10);
}

#[test]
fn test_double_vote_rejected() {
    let s = Setup::new();
    let id = s.create_proposal();
    s.client.vote(&s.voter_a, &id, &VoteChoice::For);
    assert_eq!(
        s.client.try_vote(&s.voter_a, &id, &VoteChoice::Against),
        Err(Ok(sdk_err(GovError::AlreadyVoted)))
    );
}

#[test]
fn test_vote_after_period_rejected() {
    let s = Setup::new();
    let id = s.create_proposal();
    // Advance past voting period (100 ledgers).
    s.env.ledger().with_mut(|l| l.sequence_number += 101);
    assert_eq!(
        s.client.try_vote(&s.voter_a, &id, &VoteChoice::For),
        Err(Ok(sdk_err(GovError::VotingPeriodEnded)))
    );
}

#[test]
fn test_delegated_power_counts_in_vote() {
    let s = Setup::new();
    // voter_b delegates to voter_a → voter_a effective = 15.
    s.client.delegate(&s.voter_b, &s.voter_a);
    let id = s.create_proposal();
    s.client.vote(&s.voter_a, &id, &VoteChoice::For);
    let p = s.client.get_proposal(&id);
    assert_eq!(p.for_votes, 15);
}

// ---------------------------------------------------------------------------
// Queue
// ---------------------------------------------------------------------------

#[test]
fn test_queue_before_voting_ends_rejected() {
    let s = Setup::new();
    let id = s.create_proposal();
    s.client.vote(&s.voter_a, &id, &VoteChoice::For);
    assert_eq!(
        s.client.try_queue(&id),
        Err(Ok(sdk_err(GovError::VotingPeriodNotEnded)))
    );
}

#[test]
fn test_queue_quorum_not_met_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let voter = Address::generate(&env);

    let contract_id = env.register_contract(None, GovernanceContract);
    let client = GovernanceContractClient::new(&env, &contract_id);
    // quorum = 100, voter only has 5.
    client.initialize(
        &admin,
        &GovernanceConfig {
            voting_period_ledgers: 100,
            timelock_ledgers: 50,
            quorum_votes: 100,
            proposal_threshold: 5,
        },
    );
    client.set_voting_power(&voter, &5i128);

    let id = client.propose(&voter, &payload(&env), &description(&env));
    client.vote(&voter, &id, &VoteChoice::For);

    env.ledger().with_mut(|l| l.sequence_number += 101);
    assert_eq!(
        client.try_queue(&id),
        Err(Ok(sdk_err(GovError::QuorumNotMet)))
    );
}
#[test]
fn test_queue_defeated_when_against_wins() {
    let s = Setup::new();
    let id = s.create_proposal();
    // voter_a (10) votes against, voter_b (5) votes for → against wins.
    s.client.vote(&s.voter_a, &id, &VoteChoice::Against);
    s.client.vote(&s.voter_b, &id, &VoteChoice::For);

    s.env.ledger().with_mut(|l| l.sequence_number += 101);
    // queue returns ProposalDefeated error; status stays Active due to tx rollback.
    assert_eq!(
        s.client.try_queue(&id),
        Err(Ok(sdk_err(GovError::ProposalDefeated)))
    );
}

#[test]
fn test_queue_succeeded_proposal() {
    let s = Setup::new();
    let id = s.create_proposal();
    s.client.vote(&s.voter_a, &id, &VoteChoice::For);

    s.env.ledger().with_mut(|l| l.sequence_number += 101);
    s.client.queue(&id);

    let p = s.client.get_proposal(&id);
    assert_eq!(p.status, ProposalStatus::Queued);
    // execute_after_ledger = current (101) + timelock (50) = 151.
    assert_eq!(p.execute_after_ledger, 151);
}

// ---------------------------------------------------------------------------
// Execute
// ---------------------------------------------------------------------------

#[test]
fn test_execute_before_timelock_rejected() {
    let s = Setup::new();
    let id = s.create_proposal();
    s.client.vote(&s.voter_a, &id, &VoteChoice::For);
    s.env.ledger().with_mut(|l| l.sequence_number += 101);
    s.client.queue(&id);

    // Timelock is 50 ledgers; we're at 101, need 151.
    assert_eq!(
        s.client.try_execute(&id),
        Err(Ok(sdk_err(GovError::TimelockNotExpired)))
    );
}

#[test]
fn test_execute_after_timelock_succeeds() {
    let s = Setup::new();
    let id = s.create_proposal();
    s.client.vote(&s.voter_a, &id, &VoteChoice::For);
    s.env.ledger().with_mut(|l| l.sequence_number += 101);
    s.client.queue(&id);

    // Advance past timelock.
    s.env.ledger().with_mut(|l| l.sequence_number += 50);
    s.client.execute(&id);

    assert_eq!(
        s.client.get_proposal(&id).status,
        ProposalStatus::Executed
    );
}

#[test]
fn test_execute_non_queued_rejected() {
    let s = Setup::new();
    let id = s.create_proposal();
    assert_eq!(
        s.client.try_execute(&id),
        Err(Ok(sdk_err(GovError::ProposalNotQueued)))
    );
}

// ---------------------------------------------------------------------------
// Cancellation
// ---------------------------------------------------------------------------

#[test]
fn test_proposer_can_cancel() {
    let s = Setup::new();
    let id = s.create_proposal();
    s.client.cancel(&s.voter_a, &id);
    assert_eq!(
        s.client.get_proposal(&id).status,
        ProposalStatus::Cancelled
    );
}

#[test]
fn test_admin_can_cancel() {
    let s = Setup::new();
    let id = s.create_proposal();
    s.client.cancel(&s.admin, &id);
    assert_eq!(
        s.client.get_proposal(&id).status,
        ProposalStatus::Cancelled
    );
}

#[test]
fn test_unauthorized_cancel_rejected() {
    let s = Setup::new();
    let id = s.create_proposal();
    let other = Address::generate(&s.env);
    assert_eq!(
        s.client.try_cancel(&other, &id),
        Err(Ok(sdk_err(GovError::Unauthorized)))
    );
}

#[test]
fn test_cancel_executed_proposal_rejected() {
    let s = Setup::new();
    let id = s.create_proposal();
    s.client.vote(&s.voter_a, &id, &VoteChoice::For);
    s.env.ledger().with_mut(|l| l.sequence_number += 101);
    s.client.queue(&id);
    s.env.ledger().with_mut(|l| l.sequence_number += 50);
    s.client.execute(&id);

    assert_eq!(
        s.client.try_cancel(&s.voter_a, &id),
        Err(Ok(sdk_err(GovError::ProposalNotActive)))
    );
}

// ---------------------------------------------------------------------------
// Admin transfer & config update
// ---------------------------------------------------------------------------

#[test]
fn test_transfer_admin() {
    let s = Setup::new();
    let new_admin = Address::generate(&s.env);
    s.client.transfer_admin(&new_admin);
    assert_eq!(s.client.get_admin(), new_admin);
}

#[test]
fn test_update_config() {
    let s = Setup::new();
    let new_config = GovernanceConfig {
        voting_period_ledgers: 200,
        timelock_ledgers: 100,
        quorum_votes: 20,
        proposal_threshold: 10,
    };
    s.client.update_config(&new_config);
    let cfg = s.client.get_config();
    assert_eq!(cfg.voting_period_ledgers, 200);
    assert_eq!(cfg.timelock_ledgers, 100);
}

#[test]
fn test_update_config_invalid_rejected() {
    let s = Setup::new();
    let bad = GovernanceConfig {
        voting_period_ledgers: 100,
        timelock_ledgers: 0, // invalid
        quorum_votes: 10,
        proposal_threshold: 5,
    };
    assert_eq!(
        s.client.try_update_config(&bad),
        Err(Ok(sdk_err(GovError::InvalidConfig)))
    );
}

// ---------------------------------------------------------------------------
// Full happy-path end-to-end
// ---------------------------------------------------------------------------

#[test]
fn test_full_proposal_lifecycle() {
    let s = Setup::new();

    // 1. Propose.
    let id = s.create_proposal();
    assert_eq!(s.client.get_proposal(&id).status, ProposalStatus::Active);

    // 2. Vote (voter_a for, voter_b abstain — quorum met, for > against).
    s.client.vote(&s.voter_a, &id, &VoteChoice::For);
    s.client.vote(&s.voter_b, &id, &VoteChoice::Abstain);

    // 3. End voting period.
    s.env.ledger().with_mut(|l| l.sequence_number += 101);

    // 4. Queue.
    s.client.queue(&id);
    assert_eq!(s.client.get_proposal(&id).status, ProposalStatus::Queued);

    // 5. Wait for time-lock.
    s.env.ledger().with_mut(|l| l.sequence_number += 50);

    // 6. Execute.
    s.client.execute(&id);
    assert_eq!(
        s.client.get_proposal(&id).status,
        ProposalStatus::Executed
    );
}
