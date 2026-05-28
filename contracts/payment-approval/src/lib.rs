#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, symbol_short, vec,
    Address, Bytes, Env, Symbol, Vec,
};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const KEY_ADMIN: Symbol = symbol_short!("admin");
const KEY_CONFIG: Symbol = symbol_short!("config");
const INSTANCE_TTL_EXTEND: u32 = 6_307_200;
const INSTANCE_TTL_THRESHOLD: u32 = 100_000;
/// Default timeout in ledgers (~1 hour at 5 s/ledger).
const DEFAULT_TIMEOUT: u32 = 720;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[contracterror]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum ApprovalError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    RequestNotFound = 4,
    AlreadyApproved = 5,
    AlreadyRejected = 6,
    Expired = 7,
    BelowThreshold = 8,
    NotApprover = 9,
    AlreadyVoted = 10,
    InvalidThreshold = 11,
}

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Approval status of a payment request.
#[contracttype]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum ApprovalStatus {
    Pending = 0,
    Approved = 1,
    Rejected = 2,
    Expired = 3,
}

/// Global configuration for the approval workflow.
#[contracttype]
#[derive(Clone)]
pub struct ApprovalConfig {
    /// Minimum payment amount (in stroops) that requires approval.
    pub threshold: i128,
    /// Number of approvals required to pass.
    pub required_approvals: u32,
    /// Ledgers before a pending request expires.
    pub timeout_ledgers: u32,
    /// Registered approvers.
    pub approvers: Vec<Address>,
}

/// A single approval request.
#[contracttype]
#[derive(Clone)]
pub struct ApprovalRequest {
    pub payment_id: Bytes,
    pub requester: Address,
    pub amount: i128,
    pub status: ApprovalStatus,
    pub created_at: u32,
    pub approvals: Vec<Address>,
    pub rejections: Vec<Address>,
}

// ---------------------------------------------------------------------------
// Storage helpers
// ---------------------------------------------------------------------------

fn request_key(payment_id: &Bytes) -> (Symbol, Bytes) {
    (symbol_short!("req"), payment_id.clone())
}

fn require_admin(env: &Env) {
    let admin: Address = env.storage().instance().get(&KEY_ADMIN).unwrap();
    admin.require_auth();
}

fn is_initialized(env: &Env) -> bool {
    env.storage().instance().has(&KEY_ADMIN)
}

fn bump(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(INSTANCE_TTL_THRESHOLD, INSTANCE_TTL_EXTEND);
}

fn require_initialized(env: &Env) {
    if !is_initialized(env) {
        panic_with_error!(env, ApprovalError::NotInitialized);
    }
}

fn get_config(env: &Env) -> ApprovalConfig {
    env.storage().instance().get(&KEY_CONFIG).unwrap()
}

fn is_approver(config: &ApprovalConfig, addr: &Address) -> bool {
    for a in config.approvers.iter() {
        if a == *addr {
            return true;
        }
    }
    false
}

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

#[contract]
pub struct PaymentApproval;

#[contractimpl]
impl PaymentApproval {
    pub fn initialize(env: Env, admin: Address, config: ApprovalConfig) {
        if is_initialized(&env) {
            panic_with_error!(&env, ApprovalError::AlreadyInitialized);
        }
        admin.require_auth();
        if config.threshold <= 0 || config.required_approvals == 0 {
            panic_with_error!(&env, ApprovalError::InvalidThreshold);
        }
        env.storage().instance().set(&KEY_ADMIN, &admin);
        env.storage().instance().set(&KEY_CONFIG, &config);
        bump(&env);
    }

    // ------------------------------------------------------------------
    // Admin: update config
    // ------------------------------------------------------------------

    pub fn update_config(env: Env, config: ApprovalConfig) {
        require_initialized(&env);
        require_admin(&env);
        if config.threshold <= 0 || config.required_approvals == 0 {
            panic_with_error!(&env, ApprovalError::InvalidThreshold);
        }
        env.storage().instance().set(&KEY_CONFIG, &config);
        bump(&env);
    }

    // ------------------------------------------------------------------
    // Request lifecycle
    // ------------------------------------------------------------------

    /// Submit a payment for approval. Only required when amount >= threshold.
    pub fn submit_request(env: Env, requester: Address, payment_id: Bytes, amount: i128) {
        require_initialized(&env);
        requester.require_auth();
        let config = get_config(&env);
        if amount < config.threshold {
            panic_with_error!(&env, ApprovalError::BelowThreshold);
        }
        let key = request_key(&payment_id);
        // Idempotency: don't overwrite existing request
        if env.storage().persistent().has(&key) {
            panic_with_error!(&env, ApprovalError::AlreadyApproved);
        }
        let request = ApprovalRequest {
            payment_id,
            requester,
            amount,
            status: ApprovalStatus::Pending,
            created_at: env.ledger().sequence(),
            approvals: vec![&env],
            rejections: vec![&env],
        };
        env.storage().persistent().set(&key, &request);
        env.storage()
            .persistent()
            .extend_ttl(&key, INSTANCE_TTL_THRESHOLD, INSTANCE_TTL_EXTEND);
        bump(&env);
    }

    /// Cast an approval vote. Emits Approved event when threshold is reached.
    pub fn approve(env: Env, approver: Address, payment_id: Bytes) {
        require_initialized(&env);
        approver.require_auth();
        let config = get_config(&env);
        if !is_approver(&config, &approver) {
            panic_with_error!(&env, ApprovalError::NotApprover);
        }
        let key = request_key(&payment_id);
        let mut req: ApprovalRequest = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| panic_with_error!(&env, ApprovalError::RequestNotFound));

        Self::check_not_terminal(&env, &req, &config);

        // Duplicate vote check
        for a in req.approvals.iter() {
            if a == approver {
                panic_with_error!(&env, ApprovalError::AlreadyVoted);
            }
        }

        req.approvals.push_back(approver);

        if req.approvals.len() >= config.required_approvals {
            req.status = ApprovalStatus::Approved;
            env.events().publish(
                (symbol_short!("approved"), req.payment_id.clone()),
                req.amount,
            );
        }

        env.storage().persistent().set(&key, &req);
        bump(&env);
    }

    /// Cast a rejection vote. One rejection immediately rejects the request.
    pub fn reject(env: Env, approver: Address, payment_id: Bytes) {
        require_initialized(&env);
        approver.require_auth();
        let config = get_config(&env);
        if !is_approver(&config, &approver) {
            panic_with_error!(&env, ApprovalError::NotApprover);
        }
        let key = request_key(&payment_id);
        let mut req: ApprovalRequest = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| panic_with_error!(&env, ApprovalError::RequestNotFound));

        Self::check_not_terminal(&env, &req, &config);

        for r in req.rejections.iter() {
            if r == approver {
                panic_with_error!(&env, ApprovalError::AlreadyVoted);
            }
        }

        req.rejections.push_back(approver);
        req.status = ApprovalStatus::Rejected;
        env.events().publish(
            (symbol_short!("rejected"), req.payment_id.clone()),
            req.amount,
        );

        env.storage().persistent().set(&key, &req);
        bump(&env);
    }

    /// Expire a request that has passed its timeout. Anyone can call this.
    pub fn expire_request(env: Env, payment_id: Bytes) {
        require_initialized(&env);
        let config = get_config(&env);
        let key = request_key(&payment_id);
        let mut req: ApprovalRequest = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| panic_with_error!(&env, ApprovalError::RequestNotFound));

        if req.status != ApprovalStatus::Pending {
            panic_with_error!(&env, ApprovalError::AlreadyApproved);
        }
        let timeout = config.timeout_ledgers;
        let age = env.ledger().sequence().saturating_sub(req.created_at);
        if age <= timeout {
            panic_with_error!(&env, ApprovalError::Expired);
        }
        req.status = ApprovalStatus::Expired;
        env.events().publish(
            (symbol_short!("expired"), req.payment_id.clone()),
            req.amount,
        );
        env.storage().persistent().set(&key, &req);
        bump(&env);
    }

    // ------------------------------------------------------------------
    // Queries
    // ------------------------------------------------------------------

    pub fn get_request(env: Env, payment_id: Bytes) -> ApprovalRequest {
        require_initialized(&env);
        let key = request_key(&payment_id);
        env.storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| panic_with_error!(&env, ApprovalError::RequestNotFound))
    }

    pub fn get_config(env: Env) -> ApprovalConfig {
        require_initialized(&env);
        get_config(&env)
    }

    // ------------------------------------------------------------------
    // Internal
    // ------------------------------------------------------------------

    fn check_not_terminal(env: &Env, req: &ApprovalRequest, config: &ApprovalConfig) {
        match req.status {
            ApprovalStatus::Approved => panic_with_error!(env, ApprovalError::AlreadyApproved),
            ApprovalStatus::Rejected => panic_with_error!(env, ApprovalError::AlreadyRejected),
            ApprovalStatus::Expired => panic_with_error!(env, ApprovalError::Expired),
            ApprovalStatus::Pending => {
                let age = env.ledger().sequence().saturating_sub(req.created_at);
                if age > config.timeout_ledgers {
                    panic_with_error!(env, ApprovalError::Expired);
                }
            }
        }
    }
}
