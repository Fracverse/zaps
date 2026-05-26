#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype,
    symbol_short, vec, Address, Env, Symbol, Vec,
};

// ---------------------------------------------------------------------------
// Storage keys
// ---------------------------------------------------------------------------

#[contracttype]
pub enum DataKey {
    Balance(Address),   // merchant_id → i128
    PaymentRouter,      // authorized payment router
    PayoutContract,     // authorized payout contract
    Admin,              // contract administrator
    // Multi-sig
    Signers,            // Vec<Address> — the signer set
    Threshold,          // u32 — approvals required
    NextProposalId,     // u32 — monotonic counter
    Proposal(u32),      // proposal_id → Proposal
}

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

#[contracttype]
pub struct BalanceCreditedEvent {
    pub merchant_id: Address,
    pub amount: i128,
    pub resulting_balance: i128,
}

#[contracttype]
pub struct BalanceDebitedEvent {
    pub merchant_id: Address,
    pub amount: i128,
    pub resulting_balance: i128,
}

// ---------------------------------------------------------------------------
// Multi-sig types
// ---------------------------------------------------------------------------

/// A pending withdrawal proposal.
#[contracttype]
#[derive(Clone)]
pub struct Proposal {
    pub merchant_id: Address,
    pub amount: i128,
    pub proposer: Address,
    /// Addresses that have approved so far.
    pub approvals: Vec<Address>,
    /// Ledger sequence number when the proposal was created.
    pub created_ledger: u32,
    /// Ledger count after which the proposal expires.
    pub expiry_ledgers: u32,
    pub executed: bool,
    pub cancelled: bool,
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    NegativeAmount = 1,
    InsufficientBalance = 2,
    UnauthorizedCaller = 3,
    MerchantNotInitialized = 4,
    AlreadyInitialized = 5,
    NotInitialized = 6,
    // Multi-sig errors
    NotASigner = 7,
    ProposalNotFound = 8,
    ProposalExpired = 9,
    ProposalAlreadyExecuted = 10,
    ProposalAlreadyCancelled = 11,
    AlreadyApproved = 12,
    InvalidThreshold = 13,
    EmptySigners = 14,
}

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

#[contract]
pub struct MerchantVault;

#[contractimpl]
impl MerchantVault {

    // -----------------------------------------------------------------------
    // Initialisation
    // -----------------------------------------------------------------------

    /// Initialise the contract.
    ///
    /// * `signers`        – ordered list of addresses that form the multi-sig
    ///                      signer set (must be non-empty)
    /// * `threshold`      – number of approvals required to execute a
    ///                      withdrawal (1 ≤ threshold ≤ signers.len())
    /// * `expiry_ledgers` – how many ledgers a proposal stays open before it
    ///                      expires (e.g. 17_280 ≈ 1 day at 5 s/ledger)
    pub fn initialize(
        env: Env,
        admin: Address,
        payment_router: Address,
        payout_contract: Address,
        signers: Vec<Address>,
        threshold: u32,
        expiry_ledgers: u32,
    ) -> Result<(), Error> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }

        admin.require_auth();

        if signers.is_empty() {
            return Err(Error::EmptySigners);
        }
        if threshold == 0 || threshold > signers.len() as u32 {
            return Err(Error::InvalidThreshold);
        }

        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::PaymentRouter, &payment_router);
        env.storage().instance().set(&DataKey::PayoutContract, &payout_contract);
        env.storage().instance().set(&DataKey::Signers, &signers);
        env.storage().instance().set(&DataKey::Threshold, &threshold);
        // expiry_ledgers stored per-proposal at proposal time; keep a default
        // in instance storage so callers don't have to pass it every time.
        env.storage().instance().set(&symbol_short!("exp_ldgrs"), &expiry_ledgers);
        env.storage().instance().set(&DataKey::NextProposalId, &0u32);

        Ok(())
    }

    /// Initialise a merchant account with zero balance.
    pub fn init_merchant(env: Env, merchant_id: Address) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;

        admin.require_auth();

        if env.storage().persistent().has(&DataKey::Balance(merchant_id.clone())) {
            return Err(Error::AlreadyInitialized);
        }

        env.storage().persistent().set(&DataKey::Balance(merchant_id), &0i128);
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Core ledger operations (existing paths)
    // -----------------------------------------------------------------------

    /// Credit merchant balance — callable only by the payment router.
    pub fn credit(env: Env, merchant_id: Address, amount: i128) -> Result<i128, Error> {
        let payment_router: Address = env
            .storage()
            .instance()
            .get(&DataKey::PaymentRouter)
            .ok_or(Error::NotInitialized)?;

        payment_router.require_auth();

        if amount < 0 {
            return Err(Error::NegativeAmount);
        }

        if !env.storage().persistent().has(&DataKey::Balance(merchant_id.clone())) {
            return Err(Error::MerchantNotInitialized);
        }

        let current: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::Balance(merchant_id.clone()))
            .unwrap_or(0);

        let new_balance = current.checked_add(amount).expect("Balance overflow");

        env.storage().persistent().set(&DataKey::Balance(merchant_id.clone()), &new_balance);

        env.events().publish(
            (Symbol::new(&env, "balance_credited"), merchant_id.clone()),
            BalanceCreditedEvent { merchant_id, amount, resulting_balance: new_balance },
        );

        Ok(new_balance)
    }

    /// Debit merchant balance — callable only by the payout contract.
    ///
    /// This is the direct (single-authority) debit path.  For multi-sig
    /// withdrawals use `propose_withdrawal` / `approve_withdrawal`.
    pub fn debit(env: Env, merchant_id: Address, amount: i128) -> Result<i128, Error> {
        let payout_contract: Address = env
            .storage()
            .instance()
            .get(&DataKey::PayoutContract)
            .ok_or(Error::NotInitialized)?;

        payout_contract.require_auth();

        if amount < 0 {
            return Err(Error::NegativeAmount);
        }

        if !env.storage().persistent().has(&DataKey::Balance(merchant_id.clone())) {
            return Err(Error::MerchantNotInitialized);
        }

        let current: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::Balance(merchant_id.clone()))
            .unwrap_or(0);

        if current < amount {
            return Err(Error::InsufficientBalance);
        }

        let new_balance = current - amount;
        env.storage().persistent().set(&DataKey::Balance(merchant_id.clone()), &new_balance);

        env.events().publish(
            (Symbol::new(&env, "balance_debited"), merchant_id.clone()),
            BalanceDebitedEvent { merchant_id, amount, resulting_balance: new_balance },
        );

        Ok(new_balance)
    }

    // -----------------------------------------------------------------------
    // Multi-sig withdrawal flow
    // -----------------------------------------------------------------------

    /// Propose a withdrawal from a merchant's balance.
    ///
    /// The caller must be one of the configured signers.  The proposal is
    /// created with the proposer's approval already recorded.
    ///
    /// Returns the new `proposal_id`.
    pub fn propose_withdrawal(
        env: Env,
        merchant_id: Address,
        amount: i128,
        proposer: Address,
    ) -> Result<u32, Error> {
        proposer.require_auth();

        Self::assert_signer(&env, &proposer)?;

        if amount <= 0 {
            return Err(Error::NegativeAmount);
        }

        if !env.storage().persistent().has(&DataKey::Balance(merchant_id.clone())) {
            return Err(Error::MerchantNotInitialized);
        }

        let expiry_ledgers: u32 = env
            .storage()
            .instance()
            .get(&symbol_short!("exp_ldgrs"))
            .unwrap_or(17_280);

        let proposal_id: u32 = env
            .storage()
            .instance()
            .get(&DataKey::NextProposalId)
            .unwrap_or(0);

        // Proposer's approval is counted immediately.
        let mut initial_approvals: Vec<Address> = vec![&env];
        initial_approvals.push_back(proposer.clone());

        let proposal = Proposal {
            merchant_id: merchant_id.clone(),
            amount,
            proposer: proposer.clone(),
            approvals: initial_approvals,
            created_ledger: env.ledger().sequence(),
            expiry_ledgers,
            executed: false,
            cancelled: false,
        };

        env.storage().persistent().set(&DataKey::Proposal(proposal_id), &proposal);
        env.storage().instance().set(&DataKey::NextProposalId, &(proposal_id + 1));

        env.events().publish(
            (symbol_short!("multisig"), symbol_short!("proposed")),
            (proposal_id, merchant_id, amount, proposer),
        );

        Ok(proposal_id)
    }

    /// Approve a pending withdrawal proposal.
    ///
    /// When the number of approvals reaches the threshold the withdrawal is
    /// executed automatically (balance is debited).
    pub fn approve_withdrawal(
        env: Env,
        proposal_id: u32,
        approver: Address,
    ) -> Result<bool, Error> {
        approver.require_auth();

        Self::assert_signer(&env, &approver)?;

        let mut proposal: Proposal = env
            .storage()
            .persistent()
            .get(&DataKey::Proposal(proposal_id))
            .ok_or(Error::ProposalNotFound)?;

        if proposal.cancelled {
            return Err(Error::ProposalAlreadyCancelled);
        }
        if proposal.executed {
            return Err(Error::ProposalAlreadyExecuted);
        }

        let current_ledger = env.ledger().sequence();
        if current_ledger > proposal.created_ledger + proposal.expiry_ledgers {
            return Err(Error::ProposalExpired);
        }

        // Reject duplicate approvals.
        for existing in proposal.approvals.iter() {
            if existing == approver {
                return Err(Error::AlreadyApproved);
            }
        }

        proposal.approvals.push_back(approver.clone());

        let threshold: u32 = env
            .storage()
            .instance()
            .get(&DataKey::Threshold)
            .unwrap_or(1);

        let executed = proposal.approvals.len() as u32 >= threshold;

        if executed {
            // Checks-Effects-Interactions: update state before any balance change.
            proposal.executed = true;
            env.storage().persistent().set(&DataKey::Proposal(proposal_id), &proposal);

            // Debit the balance.
            let current: i128 = env
                .storage()
                .persistent()
                .get(&DataKey::Balance(proposal.merchant_id.clone()))
                .unwrap_or(0);

            if current < proposal.amount {
                return Err(Error::InsufficientBalance);
            }

            let new_balance = current - proposal.amount;
            env.storage()
                .persistent()
                .set(&DataKey::Balance(proposal.merchant_id.clone()), &new_balance);

            env.events().publish(
                (symbol_short!("multisig"), symbol_short!("executed")),
                (proposal_id, proposal.merchant_id.clone(), proposal.amount, new_balance),
            );
        } else {
            env.storage().persistent().set(&DataKey::Proposal(proposal_id), &proposal);

            env.events().publish(
                (symbol_short!("multisig"), symbol_short!("approved")),
                (proposal_id, approver, proposal.approvals.len() as u32, threshold),
            );
        }

        Ok(executed)
    }

    /// Cancel a pending proposal.
    ///
    /// Only the original proposer or the admin may cancel.
    pub fn cancel_proposal(
        env: Env,
        proposal_id: u32,
        caller: Address,
    ) -> Result<(), Error> {
        caller.require_auth();

        let mut proposal: Proposal = env
            .storage()
            .persistent()
            .get(&DataKey::Proposal(proposal_id))
            .ok_or(Error::ProposalNotFound)?;

        if proposal.executed {
            return Err(Error::ProposalAlreadyExecuted);
        }
        if proposal.cancelled {
            return Err(Error::ProposalAlreadyCancelled);
        }

        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;

        if caller != proposal.proposer && caller != admin {
            return Err(Error::UnauthorizedCaller);
        }

        proposal.cancelled = true;
        env.storage().persistent().set(&DataKey::Proposal(proposal_id), &proposal);

        env.events().publish(
            (symbol_short!("multisig"), symbol_short!("cancelled")),
            (proposal_id, caller),
        );

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Views
    // -----------------------------------------------------------------------

    pub fn balance_of(env: Env, merchant_id: Address) -> Result<i128, Error> {
        if !env.storage().persistent().has(&DataKey::Balance(merchant_id.clone())) {
            return Err(Error::MerchantNotInitialized);
        }
        Ok(env
            .storage()
            .persistent()
            .get(&DataKey::Balance(merchant_id))
            .unwrap_or(0))
    }

    pub fn get_proposal(env: Env, proposal_id: u32) -> Result<Proposal, Error> {
        env.storage()
            .persistent()
            .get(&DataKey::Proposal(proposal_id))
            .ok_or(Error::ProposalNotFound)
    }

    pub fn get_threshold(env: Env) -> u32 {
        env.storage().instance().get(&DataKey::Threshold).unwrap_or(0)
    }

    pub fn get_signers(env: Env) -> Vec<Address> {
        env.storage()
            .instance()
            .get(&DataKey::Signers)
            .unwrap_or_else(|| vec![&env])
    }

    pub fn is_signer(env: Env, address: Address) -> bool {
        let signers: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::Signers)
            .unwrap_or_else(|| vec![&env]);
        signers.contains(&address)
    }

    // -----------------------------------------------------------------------
    // Admin helpers
    // -----------------------------------------------------------------------

    pub fn update_payment_router(env: Env, new_router: Address) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();
        env.storage().instance().set(&DataKey::PaymentRouter, &new_router);
        Ok(())
    }

    pub fn update_payout_contract(env: Env, new_payout: Address) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();
        env.storage().instance().set(&DataKey::PayoutContract, &new_payout);
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    fn assert_signer(env: &Env, address: &Address) -> Result<(), Error> {
        let signers: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::Signers)
            .ok_or(Error::NotInitialized)?;
        if !signers.contains(address) {
            return Err(Error::NotASigner);
        }
        Ok(())
    }
}

mod test;
