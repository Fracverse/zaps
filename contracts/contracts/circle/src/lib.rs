#![no_std]
#![allow(unexpected_cfgs)]
//! Zaps Circles: pooled contributions with payout limits, multi-member approval
//! governance, asset-specific balance management, and membership exit rules.
use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, token, Address, Env, Vec,
};

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    NotFound = 1,
    Unauthorized = 2,
    NotMember = 3,
    InvalidAmount = 4,
    ExceedsLimit = 5,
    InsufficientBalance = 6,
    InvalidGovernance = 7,
    NotApprover = 8,
    AlreadyApproved = 9,
    NotApproved = 10,
    AlreadyExecuted = 11,
    PendingObligations = 12,
    ExitRestricted = 13,
    InvalidDestination = 14,
    Overflow = 15,
    DeadlineExpired = 16,
    GoalNotFound = 17,
    GoalMismatch = 18,
    GoalAlreadyCompleted = 19,
    InvalidDeadline = 20,
    DuplicateRecord = 21,
}

#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum RequestStatus {
    Pending = 1,
    Approved = 2,
    Executed = 3,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Circle {
    pub admin: Address,
    pub token: Address,
    pub balance: i128,
    /// Amount reserved by pending payout requests.
    pub reserved: i128,
    /// Maximum amount per payout request. 0 means no limit.
    pub max_payout: i128,
    pub threshold: u32,
    pub approvers: Vec<Address>,
    pub next_request: u32,
    pub total_contributed: i128,
    pub total_paid: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Payout {
    pub requester: Address,
    pub to: Address,
    pub amount: i128,
    pub asset: Address,
    pub status: RequestStatus,
    /// Threshold in force when the request was made; later governance changes do not alter it.
    pub threshold: u32,
    pub approvals: Vec<Address>,
    pub executed: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApprovalProgress {
    pub current_approvals: u32,
    pub required_threshold: u32,
    pub is_approved: bool,
    pub is_executed: bool,
    pub approvers: Vec<Address>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CircleBalance {
    pub asset: Address,
    pub total_balance: i128,
    pub reserved_balance: i128,
    pub available_balance: i128,
    pub total_contributed: i128,
    pub total_paid: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssetBalanceInfo {
    pub asset: Address,
    pub total_balance: i128,
    pub reserved_balance: i128,
    pub available_balance: i128,
    pub total_contributed: i128,
    pub total_paid: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReconciliationResult {
    pub on_chain_token_balance: i128,
    pub recorded_balance: i128,
    pub is_reconciled: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Goal {
    pub id: u32,
    pub circle_id: u32,
    pub creator: Address,
    pub token: Address,
    pub target_amount: i128,
    pub current_amount: i128,
    pub deadline: u64,
    pub completed: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GoalProgress {
    pub goal_id: u32,
    pub circle_id: u32,
    pub token: Address,
    pub target_amount: i128,
    pub current_amount: i128,
    pub remaining_amount: i128,
    pub progress_bps: u32,
    pub is_completed: bool,
    pub deadline: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContributionRecord {
    pub id: u32,
    pub circle_id: u32,
    pub contributor: Address,
    pub amount: i128,
    pub asset: Address,
    pub timestamp: u64,
    pub ledger: u32,
    pub goal_id: Option<u32>,
}

#[contracttype]
enum Key {
    Count,
    Circle(u32),
    Member(u32, Address),
    Contribution(u32, Address),
    Pending(u32, Address),
    Payout(u32, u32),
    AssetBalance(Address),
    AssetReserved(Address),
    AssetContributed(Address),
    AssetPaid(Address),
    CircleDeadline(u32),
    Goal(u32, u32),
    CircleGoalCount(u32),
    MemberGoalContrib(u32, u32, Address),
    MemberAssetContrib(Address, Address),
    MemberContribCount(u32, Address),
    RecordCount,
    Record(u32),
    CircleRecordCount(u32),
    CircleRecordAt(u32, u32),
}

#[contract]
pub struct CircleContract;

fn load(env: &Env, id: u32) -> Result<Circle, Error> {
    env.storage().persistent().get(&Key::Circle(id)).ok_or(Error::NotFound)
}

fn save(env: &Env, id: u32, c: &Circle) {
    env.storage().persistent().set(&Key::Circle(id), c);
}

fn is_member(env: &Env, id: u32, who: &Address) -> bool {
    env.storage().persistent().get(&Key::Member(id, who.clone())).unwrap_or(false)
}

fn pending(env: &Env, id: u32, who: &Address) -> u32 {
    env.storage().persistent().get(&Key::Pending(id, who.clone())).unwrap_or(0)
}

fn check_governance(threshold: u32, approvers: &Vec<Address>) -> Result<(), Error> {
    if threshold == 0 || threshold > approvers.len() {
        return Err(Error::InvalidGovernance);
    }
    Ok(())
}

fn get_asset_bal(env: &Env, asset: &Address) -> i128 {
    env.storage().persistent().get(&Key::AssetBalance(asset.clone())).unwrap_or(0)
}

fn set_asset_bal(env: &Env, asset: &Address, val: i128) {
    env.storage().persistent().set(&Key::AssetBalance(asset.clone()), &val);
}

fn get_asset_res(env: &Env, asset: &Address) -> i128 {
    env.storage().persistent().get(&Key::AssetReserved(asset.clone())).unwrap_or(0)
}

fn set_asset_res(env: &Env, asset: &Address, val: i128) {
    env.storage().persistent().set(&Key::AssetReserved(asset.clone()), &val);
}

fn get_asset_contrib(env: &Env, asset: &Address) -> i128 {
    env.storage().persistent().get(&Key::AssetContributed(asset.clone())).unwrap_or(0)
}

fn set_asset_contrib(env: &Env, asset: &Address, val: i128) {
    env.storage().persistent().set(&Key::AssetContributed(asset.clone()), &val);
}

fn get_asset_p(env: &Env, asset: &Address) -> i128 {
    env.storage().persistent().get(&Key::AssetPaid(asset.clone())).unwrap_or(0)
}

fn set_asset_p(env: &Env, asset: &Address, val: i128) {
    env.storage().persistent().set(&Key::AssetPaid(asset.clone()), &val);
}

fn get_circle_deadl(env: &Env, id: u32) -> u64 {
    env.storage().persistent().get(&Key::CircleDeadline(id)).unwrap_or(0)
}

fn record_contribution_internal(
    env: &Env,
    circle_id: u32,
    contributor: &Address,
    amount: i128,
    asset: &Address,
    goal_id: Option<u32>,
) -> Result<u32, Error> {
    let rec_id: u32 = env.storage().persistent().get(&Key::RecordCount).unwrap_or(0);
    let next_rec_id = rec_id.checked_add(1).ok_or(Error::Overflow)?;

    let key = Key::Record(rec_id);
    if env.storage().persistent().has(&key) {
        return Err(Error::DuplicateRecord);
    }

    let record = ContributionRecord {
        id: rec_id,
        circle_id,
        contributor: contributor.clone(),
        amount,
        asset: asset.clone(),
        timestamp: env.ledger().timestamp(),
        ledger: env.ledger().sequence(),
        goal_id,
    };

    env.storage().persistent().set(&key, &record);
    env.storage().persistent().set(&Key::RecordCount, &next_rec_id);

    let circle_count: u32 = env
        .storage()
        .persistent()
        .get(&Key::CircleRecordCount(circle_id))
        .unwrap_or(0);
    let next_circle_count = circle_count.checked_add(1).ok_or(Error::Overflow)?;
    env.storage()
        .persistent()
        .set(&Key::CircleRecordAt(circle_id, circle_count), &rec_id);
    env.storage()
        .persistent()
        .set(&Key::CircleRecordCount(circle_id), &next_circle_count);

    let member_contrib_count: u32 = env
        .storage()
        .persistent()
        .get(&Key::MemberContribCount(circle_id, contributor.clone()))
        .unwrap_or(0);
    let next_m_count = member_contrib_count.checked_add(1).ok_or(Error::Overflow)?;
    env.storage()
        .persistent()
        .set(&Key::MemberContribCount(circle_id, contributor.clone()), &next_m_count);

    let member_asset_total: i128 = env
        .storage()
        .persistent()
        .get(&Key::MemberAssetContrib(contributor.clone(), asset.clone()))
        .unwrap_or(0);
    let next_asset_total = member_asset_total.checked_add(amount).ok_or(Error::Overflow)?;
    env.storage()
        .persistent()
        .set(&Key::MemberAssetContrib(contributor.clone(), asset.clone()), &next_asset_total);

    env.events()
        .publish((symbol_short!("rec_crt"), circle_id, rec_id), amount);

    Ok(rec_id)
}

#[contractimpl]
impl CircleContract {
    /// Creates a circle. The admin becomes the first member and sole approver (threshold 1).
    pub fn create_circle(env: Env, admin: Address, token: Address, max_payout: i128) -> Result<u32, Error> {
        admin.require_auth();
        if max_payout < 0 {
            return Err(Error::InvalidAmount);
        }
        let id: u32 = env.storage().instance().get(&Key::Count).unwrap_or(0);
        let next_count = id.checked_add(1).ok_or(Error::Overflow)?;
        env.storage().instance().set(&Key::Count, &next_count);
        let c = Circle {
            admin: admin.clone(),
            token,
            balance: 0,
            reserved: 0,
            max_payout,
            threshold: 1,
            approvers: Vec::from_array(&env, [admin.clone()]),
            next_request: 0,
            total_contributed: 0,
            total_paid: 0,
        };
        save(&env, id, &c);
        env.storage().persistent().set(&Key::Member(id, admin.clone()), &true);
        env.events().publish((symbol_short!("created"), id), admin);
        Ok(id)
    }

    pub fn join(env: Env, id: u32, member: Address) -> Result<(), Error> {
        member.require_auth();
        load(&env, id)?;
        env.storage().persistent().set(&Key::Member(id, member.clone()), &true);
        env.events().publish((symbol_short!("joined"), id), member);
        Ok(())
    }

    pub fn contribute(env: Env, id: u32, member: Address, amount: i128) -> Result<(), Error> {
        member.require_auth();
        let mut c = load(&env, id)?;
        if !is_member(&env, id, &member) {
            return Err(Error::NotMember);
        }
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }

        // Validate circle deadline
        let deadline = get_circle_deadl(&env, id);
        if deadline > 0 && env.ledger().timestamp() > deadline {
            return Err(Error::DeadlineExpired);
        }

        token::Client::new(&env, &c.token).transfer(&member, &env.current_contract_address(), &amount);

        c.balance = c.balance.checked_add(amount).ok_or(Error::Overflow)?;
        c.total_contributed = c.total_contributed.checked_add(amount).ok_or(Error::Overflow)?;
        save(&env, id, &c);

        let key = Key::Contribution(id, member.clone());
        let total: i128 = env.storage().persistent().get(&key).unwrap_or(0);
        let new_total = total.checked_add(amount).ok_or(Error::Overflow)?;
        env.storage().persistent().set(&key, &new_total);

        // Update asset-level tracking
        let ab = get_asset_bal(&env, &c.token).checked_add(amount).ok_or(Error::Overflow)?;
        set_asset_bal(&env, &c.token, ab);
        let ac = get_asset_contrib(&env, &c.token).checked_add(amount).ok_or(Error::Overflow)?;
        set_asset_contrib(&env, &c.token, ac);

        // Atomic contribution record creation
        record_contribution_internal(&env, id, &member, amount, &c.token, None)?;

        env.events().publish((symbol_short!("contrib"), id, member), amount);
        Ok(())
    }

    /// Contributes funds and returns the unique contribution record identifier.
    pub fn contribute_with_record(env: Env, id: u32, member: Address, amount: i128) -> Result<u32, Error> {
        member.require_auth();
        let mut c = load(&env, id)?;
        if !is_member(&env, id, &member) {
            return Err(Error::NotMember);
        }
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }

        // Validate circle deadline
        let deadline = get_circle_deadl(&env, id);
        if deadline > 0 && env.ledger().timestamp() > deadline {
            return Err(Error::DeadlineExpired);
        }

        token::Client::new(&env, &c.token).transfer(&member, &env.current_contract_address(), &amount);

        c.balance = c.balance.checked_add(amount).ok_or(Error::Overflow)?;
        c.total_contributed = c.total_contributed.checked_add(amount).ok_or(Error::Overflow)?;
        save(&env, id, &c);

        let key = Key::Contribution(id, member.clone());
        let total: i128 = env.storage().persistent().get(&key).unwrap_or(0);
        let new_total = total.checked_add(amount).ok_or(Error::Overflow)?;
        env.storage().persistent().set(&key, &new_total);

        // Update asset-level tracking
        let ab = get_asset_bal(&env, &c.token).checked_add(amount).ok_or(Error::Overflow)?;
        set_asset_bal(&env, &c.token, ab);
        let ac = get_asset_contrib(&env, &c.token).checked_add(amount).ok_or(Error::Overflow)?;
        set_asset_contrib(&env, &c.token, ac);

        // Atomic contribution record creation
        let rec_id = record_contribution_internal(&env, id, &member, amount, &c.token, None)?;

        env.events().publish((symbol_short!("contrib"), id, member), amount);
        Ok(rec_id)
    }

    /// Admin-only. Sets the per-request payout limit (0 disables the limit).
    pub fn set_max_payout(env: Env, id: u32, admin: Address, max_payout: i128) -> Result<(), Error> {
        admin.require_auth();
        let mut c = load(&env, id)?;
        if admin != c.admin {
            return Err(Error::Unauthorized);
        }
        if max_payout < 0 {
            return Err(Error::InvalidAmount);
        }
        c.max_payout = max_payout;
        save(&env, id, &c);
        env.events().publish((symbol_short!("limit"), id), max_payout);
        Ok(())
    }

    /// Admin-only. Sets full governance: threshold and approvers list. Existing requests keep their snapshot threshold.
    pub fn set_governance(env: Env, id: u32, admin: Address, threshold: u32, approvers: Vec<Address>) -> Result<(), Error> {
        admin.require_auth();
        let mut c = load(&env, id)?;
        if admin != c.admin {
            return Err(Error::Unauthorized);
        }
        check_governance(threshold, &approvers)?;
        c.threshold = threshold;
        c.approvers = approvers;
        save(&env, id, &c);
        env.events().publish((symbol_short!("gov"), id), threshold);
        Ok(())
    }

    /// Admin-only. Configures only the approval threshold.
    pub fn set_approval_threshold(env: Env, id: u32, admin: Address, threshold: u32) -> Result<(), Error> {
        admin.require_auth();
        let mut c = load(&env, id)?;
        if admin != c.admin {
            return Err(Error::Unauthorized);
        }
        check_governance(threshold, &c.approvers)?;
        c.threshold = threshold;
        save(&env, id, &c);
        env.events().publish((symbol_short!("gov_thresh"), id), threshold);
        Ok(())
    }

    /// Admin-only. Adds an approver.
    pub fn add_approver(env: Env, id: u32, admin: Address, approver: Address) -> Result<(), Error> {
        admin.require_auth();
        let mut c = load(&env, id)?;
        if admin != c.admin {
            return Err(Error::Unauthorized);
        }
        if !c.approvers.contains(&approver) {
            c.approvers.push_back(approver.clone());
            save(&env, id, &c);
            env.events().publish((symbol_short!("appr_add"), id), approver);
        }
        Ok(())
    }

    /// Admin-only. Removes an approver if remaining approvers count satisfies threshold.
    pub fn remove_approver(env: Env, id: u32, admin: Address, approver: Address) -> Result<(), Error> {
        admin.require_auth();
        let mut c = load(&env, id)?;
        if admin != c.admin {
            return Err(Error::Unauthorized);
        }
        let mut new_approvers = Vec::new(&env);
        for a in c.approvers.iter() {
            if a != approver {
                new_approvers.push_back(a);
            }
        }
        check_governance(c.threshold, &new_approvers)?;
        c.approvers = new_approvers;
        save(&env, id, &c);
        env.events().publish((symbol_short!("appr_rem"), id), approver);
        Ok(())
    }

    /// Submits a withdrawal / payout request. Only members may request.
    /// Destination is validated, amount is checked against max payout and available balance.
    pub fn request_payout(env: Env, id: u32, member: Address, to: Address, amount: i128) -> Result<u32, Error> {
        member.require_auth();
        let mut c = load(&env, id)?;
        if !is_member(&env, id, &member) {
            return Err(Error::NotMember);
        }
        if to == env.current_contract_address() {
            return Err(Error::InvalidDestination);
        }
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }
        if c.max_payout > 0 && amount > c.max_payout {
            return Err(Error::ExceedsLimit);
        }
        let available = c.balance.checked_sub(c.reserved).ok_or(Error::InsufficientBalance)?;
        if amount > available {
            return Err(Error::InsufficientBalance);
        }

        let rid = c.next_request;
        c.next_request = c.next_request.checked_add(1).ok_or(Error::Overflow)?;
        c.reserved = c.reserved.checked_add(amount).ok_or(Error::Overflow)?;

        // Update asset-level reserved balance
        let ar = get_asset_res(&env, &c.token).checked_add(amount).ok_or(Error::Overflow)?;
        set_asset_res(&env, &c.token, ar);

        let p = Payout {
            requester: member.clone(),
            to,
            amount,
            asset: c.token.clone(),
            status: RequestStatus::Pending,
            threshold: c.threshold,
            approvals: Vec::new(&env),
            executed: false,
        };
        env.storage().persistent().set(&Key::Payout(id, rid), &p);

        let n = pending(&env, id, &member);
        let new_n = n.checked_add(1).ok_or(Error::Overflow)?;
        env.storage().persistent().set(&Key::Pending(id, member), &new_n);

        save(&env, id, &c);
        env.events().publish((symbol_short!("request"), id, rid), amount);
        Ok(rid)
    }

    /// Alias for `request_payout` with withdrawal nomenclature.
    pub fn request_withdrawal(env: Env, id: u32, member: Address, destination: Address, amount: i128) -> Result<u32, Error> {
        Self::request_payout(env, id, member, destination, amount)
    }

    /// Authorizes a withdrawal/payout request. Only designated approvers may approve.
    pub fn approve(env: Env, id: u32, rid: u32, approver: Address) -> Result<(), Error> {
        approver.require_auth();
        let c = load(&env, id)?;
        if !c.approvers.contains(&approver) {
            return Err(Error::NotApprover);
        }
        let mut p: Payout = env.storage().persistent().get(&Key::Payout(id, rid)).ok_or(Error::NotFound)?;
        if p.executed {
            return Err(Error::AlreadyExecuted);
        }
        if p.approvals.contains(&approver) {
            return Err(Error::AlreadyApproved);
        }
        p.approvals.push_back(approver.clone());
        if p.approvals.len() >= p.threshold {
            p.status = RequestStatus::Approved;
        }
        env.storage().persistent().set(&Key::Payout(id, rid), &p);
        env.events().publish((symbol_short!("approved"), id, rid), approver);
        Ok(())
    }

    /// Executes an approved withdrawal request. Checks authorization threshold,
    /// verifies payout limit and balance, then atomically updates accounting and transfers funds.
    pub fn execute(env: Env, id: u32, rid: u32) -> Result<(), Error> {
        let mut c = load(&env, id)?;
        let mut p: Payout = env.storage().persistent().get(&Key::Payout(id, rid)).ok_or(Error::NotFound)?;
        if p.executed {
            return Err(Error::AlreadyExecuted);
        }
        if p.approvals.len() < p.threshold {
            return Err(Error::NotApproved);
        }
        // Re-check limit and balance so a limit lowered after the request cannot be bypassed.
        if c.max_payout > 0 && p.amount > c.max_payout {
            return Err(Error::ExceedsLimit);
        }
        if c.balance < p.amount {
            return Err(Error::InsufficientBalance);
        }

        p.executed = true;
        p.status = RequestStatus::Executed;

        c.balance = c.balance.checked_sub(p.amount).ok_or(Error::InsufficientBalance)?;
        c.reserved = c.reserved.checked_sub(p.amount).ok_or(Error::InsufficientBalance)?;
        c.total_paid = c.total_paid.checked_add(p.amount).ok_or(Error::Overflow)?;

        // Update asset-level tracking
        let ab = get_asset_bal(&env, &c.token).checked_sub(p.amount).ok_or(Error::InsufficientBalance)?;
        set_asset_bal(&env, &c.token, ab);
        let ar = get_asset_res(&env, &c.token).checked_sub(p.amount).ok_or(Error::InsufficientBalance)?;
        set_asset_res(&env, &c.token, ar);
        let ap = get_asset_p(&env, &c.token).checked_add(p.amount).ok_or(Error::Overflow)?;
        set_asset_p(&env, &c.token, ap);

        env.storage().persistent().set(&Key::Payout(id, rid), &p);

        let n = pending(&env, id, &p.requester);
        if n > 0 {
            env.storage().persistent().set(&Key::Pending(id, p.requester.clone()), &(n - 1));
        }
        save(&env, id, &c);

        token::Client::new(&env, &p.asset).transfer(&env.current_contract_address(), &p.to, &p.amount);
        env.events().publish((symbol_short!("paid"), id, rid), p.amount);
        Ok(())
    }

    /// Alias for `execute`.
    pub fn execute_payout(env: Env, id: u32, rid: u32) -> Result<(), Error> {
        Self::execute(env, id, rid)
    }

    /// Removes a member. Admins and approvers cannot exit, and members with
    /// pending payout requests must wait. Contribution history is kept.
    pub fn leave(env: Env, id: u32, member: Address) -> Result<(), Error> {
        member.require_auth();
        let c = load(&env, id)?;
        if !is_member(&env, id, &member) {
            return Err(Error::NotMember);
        }
        if member == c.admin || c.approvers.contains(&member) {
            return Err(Error::ExitRestricted);
        }
        if pending(&env, id, &member) > 0 {
            return Err(Error::PendingObligations);
        }
        env.storage().persistent().set(&Key::Member(id, member.clone()), &false);
        env.events().publish((symbol_short!("left"), id), member);
        Ok(())
    }

    // ── Queries & Reconciliation ─────────────────────────────────────────

    pub fn get_circle(env: Env, id: u32) -> Result<Circle, Error> {
        load(&env, id)
    }

    pub fn is_member(env: Env, id: u32, who: Address) -> bool {
        is_member(&env, id, &who)
    }

    pub fn contribution_of(env: Env, id: u32, who: Address) -> i128 {
        env.storage().persistent().get(&Key::Contribution(id, who)).unwrap_or(0)
    }

    /// Queries a withdrawal request / payout by circle ID and request ID.
    pub fn get_payout(env: Env, id: u32, rid: u32) -> Result<Payout, Error> {
        load(&env, id)?;
        env.storage().persistent().get(&Key::Payout(id, rid)).ok_or(Error::NotFound)
    }

    /// Alias for `get_payout`.
    pub fn get_withdrawal_request(env: Env, id: u32, rid: u32) -> Result<Payout, Error> {
        Self::get_payout(env, id, rid)
    }

    /// Queries the approval progress of a payout request.
    pub fn get_approval_progress(env: Env, id: u32, rid: u32) -> Result<ApprovalProgress, Error> {
        load(&env, id)?;
        let p: Payout = env.storage().persistent().get(&Key::Payout(id, rid)).ok_or(Error::NotFound)?;
        let count = p.approvals.len();
        Ok(ApprovalProgress {
            current_approvals: count,
            required_threshold: p.threshold,
            is_approved: count >= p.threshold,
            is_executed: p.executed,
            approvers: p.approvals,
        })
    }

    /// Queries circle balance breakdown (total, reserved, available, contributions, payouts).
    pub fn get_circle_balance(env: Env, id: u32) -> Result<CircleBalance, Error> {
        let c = load(&env, id)?;
        Ok(CircleBalance {
            asset: c.token,
            total_balance: c.balance,
            reserved_balance: c.reserved,
            available_balance: c.balance - c.reserved,
            total_contributed: c.total_contributed,
            total_paid: c.total_paid,
        })
    }

    /// Queries available funds for a circle (balance minus reserved).
    pub fn get_available_balance(env: Env, id: u32) -> Result<i128, Error> {
        let c = load(&env, id)?;
        Ok(c.balance - c.reserved)
    }

    /// Queries cumulative contributions made to the circle.
    pub fn get_total_contributions(env: Env, id: u32) -> Result<i128, Error> {
        let c = load(&env, id)?;
        Ok(c.total_contributed)
    }

    /// Queries cumulative payouts executed by the circle.
    pub fn get_total_payouts(env: Env, id: u32) -> Result<i128, Error> {
        let c = load(&env, id)?;
        Ok(c.total_paid)
    }

    /// Queries global balance breakdown across all circles for a specific asset.
    pub fn get_asset_balance_info(env: Env, asset: Address) -> AssetBalanceInfo {
        let total = get_asset_bal(&env, &asset);
        let res = get_asset_res(&env, &asset);
        AssetBalanceInfo {
            asset: asset.clone(),
            total_balance: total,
            reserved_balance: res,
            available_balance: total - res,
            total_contributed: get_asset_contrib(&env, &asset),
            total_paid: get_asset_p(&env, &asset),
        }
    }

    /// Queries total tracked balance for a specific asset across all circles.
    pub fn get_asset_balance(env: Env, asset: Address) -> i128 {
        get_asset_bal(&env, &asset)
    }

    /// Queries available balance for a specific asset across all circles.
    pub fn get_asset_available_balance(env: Env, asset: Address) -> i128 {
        get_asset_bal(&env, &asset) - get_asset_res(&env, &asset)
    }

    /// Reconciles the circle's recorded balance against the contract's actual on-chain token balance.
    pub fn reconcile_circle_balance(env: Env, id: u32) -> Result<ReconciliationResult, Error> {
        let c = load(&env, id)?;
        let actual = token::Client::new(&env, &c.token).balance(&env.current_contract_address());
        Ok(ReconciliationResult {
            on_chain_token_balance: actual,
            recorded_balance: c.balance,
            is_reconciled: actual >= c.balance,
        })
    }

    /// Reconciles the tracked total asset balance against the contract's actual on-chain token balance.
    pub fn reconcile_asset_balance(env: Env, asset: Address) -> ReconciliationResult {
        let recorded = get_asset_bal(&env, &asset);
        let actual = token::Client::new(&env, &asset).balance(&env.current_contract_address());
        ReconciliationResult {
            on_chain_token_balance: actual,
            recorded_balance: recorded,
            is_reconciled: actual >= recorded,
        }
    }

    // ── Contribution Deadlines (#1241) ───────────────────────────────────

    /// Admin-only. Sets or updates the contribution deadline for a circle (0 = no deadline).
    pub fn set_circle_deadline(env: Env, id: u32, admin: Address, deadline: u64) -> Result<(), Error> {
        admin.require_auth();
        let c = load(&env, id)?;
        if admin != c.admin {
            return Err(Error::Unauthorized);
        }
        env.storage().persistent().set(&Key::CircleDeadline(id), &deadline);
        env.events().publish((symbol_short!("deadl_set"), id), deadline);
        Ok(())
    }

    /// Queries the contribution deadline of a circle (0 if no deadline is set).
    pub fn get_circle_deadline(env: Env, id: u32) -> Result<u64, Error> {
        load(&env, id)?;
        Ok(get_circle_deadl(&env, id))
    }

    /// Checks if a circle is currently active and accepting contributions based on ledger time.
    pub fn is_circle_active(env: Env, id: u32) -> Result<bool, Error> {
        load(&env, id)?;
        let deadline = get_circle_deadl(&env, id);
        if deadline == 0 {
            Ok(true)
        } else {
            Ok(env.ledger().timestamp() <= deadline)
        }
    }

    // ── Savings Goal Contribution Tracking (#1240) ───────────────────────

    /// Creates a savings goal within a circle. Must be a circle member.
    pub fn create_goal(
        env: Env,
        circle_id: u32,
        creator: Address,
        target_amount: i128,
        deadline: u64,
    ) -> Result<u32, Error> {
        creator.require_auth();
        let c = load(&env, circle_id)?;
        if !is_member(&env, circle_id, &creator) {
            return Err(Error::NotMember);
        }
        if target_amount <= 0 {
            return Err(Error::InvalidAmount);
        }
        if deadline > 0 && deadline < env.ledger().timestamp() {
            return Err(Error::InvalidDeadline);
        }

        let goal_id: u32 = env
            .storage()
            .persistent()
            .get(&Key::CircleGoalCount(circle_id))
            .unwrap_or(0);
        let next_goal_id = goal_id.checked_add(1).ok_or(Error::Overflow)?;
        env.storage().persistent().set(&Key::CircleGoalCount(circle_id), &next_goal_id);

        let goal = Goal {
            id: goal_id,
            circle_id,
            creator: creator.clone(),
            token: c.token,
            target_amount,
            current_amount: 0,
            deadline,
            completed: false,
        };
        env.storage().persistent().set(&Key::Goal(circle_id, goal_id), &goal);

        env.events().publish((symbol_short!("goal_crt"), circle_id, goal_id), target_amount);
        Ok(goal_id)
    }

    /// Contributes towards a specific savings goal. Updates goal progress atomically.
    pub fn contribute_to_goal(
        env: Env,
        circle_id: u32,
        goal_id: u32,
        member: Address,
        amount: i128,
    ) -> Result<u32, Error> {
        member.require_auth();
        let mut c = load(&env, circle_id)?;
        if !is_member(&env, circle_id, &member) {
            return Err(Error::NotMember);
        }
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }

        // Validate circle deadline
        let circle_deadline = get_circle_deadl(&env, circle_id);
        let now = env.ledger().timestamp();
        if circle_deadline > 0 && now > circle_deadline {
            return Err(Error::DeadlineExpired);
        }

        // Load goal
        let mut goal: Goal = env
            .storage()
            .persistent()
            .get(&Key::Goal(circle_id, goal_id))
            .ok_or(Error::GoalNotFound)?;

        if goal.circle_id != circle_id {
            return Err(Error::GoalMismatch);
        }

        if goal.completed {
            return Err(Error::GoalAlreadyCompleted);
        }

        // Validate goal deadline if set
        if goal.deadline > 0 && now > goal.deadline {
            return Err(Error::DeadlineExpired);
        }

        // Validate correct asset denomination
        if goal.token != c.token {
            return Err(Error::GoalMismatch);
        }

        token::Client::new(&env, &c.token).transfer(&member, &env.current_contract_address(), &amount);

        c.balance = c.balance.checked_add(amount).ok_or(Error::Overflow)?;
        c.total_contributed = c.total_contributed.checked_add(amount).ok_or(Error::Overflow)?;
        save(&env, circle_id, &c);

        let key = Key::Contribution(circle_id, member.clone());
        let total: i128 = env.storage().persistent().get(&key).unwrap_or(0);
        let new_total = total.checked_add(amount).ok_or(Error::Overflow)?;
        env.storage().persistent().set(&key, &new_total);

        // Update member goal contribution
        let goal_member_key = Key::MemberGoalContrib(circle_id, goal_id, member.clone());
        let current_mgc: i128 = env.storage().persistent().get(&goal_member_key).unwrap_or(0);
        let new_mgc = current_mgc.checked_add(amount).ok_or(Error::Overflow)?;
        env.storage().persistent().set(&goal_member_key, &new_mgc);

        // Update asset-level tracking
        let ab = get_asset_bal(&env, &c.token).checked_add(amount).ok_or(Error::Overflow)?;
        set_asset_bal(&env, &c.token, ab);
        let ac = get_asset_contrib(&env, &c.token).checked_add(amount).ok_or(Error::Overflow)?;
        set_asset_contrib(&env, &c.token, ac);

        // Update goal progress and completion status
        goal.current_amount = goal.current_amount.checked_add(amount).ok_or(Error::Overflow)?;
        if goal.current_amount >= goal.target_amount {
            goal.completed = true;
            env.events().publish((symbol_short!("goal_cmp"), circle_id, goal_id), goal.current_amount);
        }
        env.storage().persistent().set(&Key::Goal(circle_id, goal_id), &goal);

        // Atomic record creation associated with goal
        let rec_id = record_contribution_internal(&env, circle_id, &member, amount, &c.token, Some(goal_id))?;

        env.events().publish((symbol_short!("contrib"), circle_id, member), amount);
        env.events().publish((symbol_short!("goal_con"), circle_id, goal_id), amount);

        Ok(rec_id)
    }

    /// Queries goal details.
    pub fn get_goal(env: Env, circle_id: u32, goal_id: u32) -> Result<Goal, Error> {
        load(&env, circle_id)?;
        env.storage()
            .persistent()
            .get(&Key::Goal(circle_id, goal_id))
            .ok_or(Error::GoalNotFound)
    }

    /// Queries goal progress metrics including remaining amount and percentage completion in bps.
    pub fn get_goal_progress(env: Env, circle_id: u32, goal_id: u32) -> Result<GoalProgress, Error> {
        load(&env, circle_id)?;
        let goal: Goal = env
            .storage()
            .persistent()
            .get(&Key::Goal(circle_id, goal_id))
            .ok_or(Error::GoalNotFound)?;

        let remaining = if goal.current_amount >= goal.target_amount {
            0
        } else {
            goal.target_amount - goal.current_amount
        };

        let progress_bps = if goal.target_amount <= 0 {
            0
        } else if goal.current_amount >= goal.target_amount {
            10_000
        } else {
            let p = (goal.current_amount.checked_mul(10_000).unwrap_or(0)) / goal.target_amount;
            p as u32
        };

        Ok(GoalProgress {
            goal_id,
            circle_id,
            token: goal.token,
            target_amount: goal.target_amount,
            current_amount: goal.current_amount,
            remaining_amount: remaining,
            progress_bps,
            is_completed: goal.completed,
            deadline: goal.deadline,
        })
    }

    /// Queries total goals created for a circle.
    pub fn get_goal_count(env: Env, circle_id: u32) -> Result<u32, Error> {
        load(&env, circle_id)?;
        Ok(env.storage().persistent().get(&Key::CircleGoalCount(circle_id)).unwrap_or(0))
    }

    /// Queries how much a specific member contributed to a specific goal.
    pub fn get_member_goal_contribution(env: Env, circle_id: u32, goal_id: u32, member: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&Key::MemberGoalContrib(circle_id, goal_id, member))
            .unwrap_or(0)
    }

    // ── Contribution Transaction Records & Accounting (#1237, #1238) ─────

    /// Queries an individual contribution record by its unique global ID.
    pub fn get_contribution_record(env: Env, record_id: u32) -> Result<ContributionRecord, Error> {
        env.storage()
            .persistent()
            .get(&Key::Record(record_id))
            .ok_or(Error::NotFound)
    }

    /// Queries paginated contribution history for a circle.
    pub fn get_circle_contributions(
        env: Env,
        circle_id: u32,
        start: u32,
        limit: u32,
    ) -> Result<Vec<ContributionRecord>, Error> {
        load(&env, circle_id)?;
        let count: u32 = env
            .storage()
            .persistent()
            .get(&Key::CircleRecordCount(circle_id))
            .unwrap_or(0);

        let mut records = Vec::new(&env);
        if start >= count || limit == 0 {
            return Ok(records);
        }

        let safe_limit = if limit > 50 { 50 } else { limit };
        let end = (start.saturating_add(safe_limit)).min(count);

        for i in start..end {
            let rec_id_opt: Option<u32> = env.storage().persistent().get(&Key::CircleRecordAt(circle_id, i));
            if let Some(rec_id) = rec_id_opt {
                let record_opt: Option<ContributionRecord> = env.storage().persistent().get(&Key::Record(rec_id));
                if let Some(record) = record_opt {
                    records.push_back(record);
                }
            }
        }

        Ok(records)
    }

    /// Queries total number of contributions recorded for a specific circle.
    pub fn get_circle_contribution_count(env: Env, circle_id: u32) -> Result<u32, Error> {
        load(&env, circle_id)?;
        Ok(env.storage().persistent().get(&Key::CircleRecordCount(circle_id)).unwrap_or(0))
    }

    /// Queries total number of contribution records created across all circles.
    pub fn get_total_records_count(env: Env) -> u32 {
        env.storage().persistent().get(&Key::RecordCount).unwrap_or(0)
    }

    /// Queries total cumulative contributions made by a member across all circles for a specific asset.
    pub fn get_member_asset_contribution(env: Env, who: Address, asset: Address) -> i128 {
        env.storage().persistent().get(&Key::MemberAssetContrib(who, asset)).unwrap_or(0)
    }

    /// Queries the number of contributions made by a specific member to a specific circle.
    pub fn get_member_contribution_count(env: Env, id: u32, who: Address) -> u32 {
        env.storage().persistent().get(&Key::MemberContribCount(id, who)).unwrap_or(0)
    }
}

#[cfg(test)]
mod test;
