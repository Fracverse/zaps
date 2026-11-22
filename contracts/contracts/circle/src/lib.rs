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
    NotInitialized = 22,
    AlreadyInitialized = 23,
    UnsupportedAsset = 24,
    AssetInUse = 25,
    InvalidContributionRules = 26,
    ContributionRuleViolation = 27,
    InvalidConfiguration = 28,
    MembershipLimitReached = 29,
    // Emergency pause errors (#1250)
    EmergencyPaused = 30,
    EmergencyAdminRequired = 31,
    // Recovery mechanism errors (#1251)
    RecoveryThresholdNotMet = 32,
    RecoveryNotAuthorized = 33,
    UnauthorizedRecoveryAttempt = 34,
    RecoveryInProgress = 35,
    InvalidRecoveryTarget = 36,
    // Rule versioning errors (#1252)
    InvalidRuleVersion = 37,
    RuleVersionMismatch = 38,
    UnauthorizedRuleChange = 39,
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
    /// Emergency pause state (#1250)
    pub paused: bool,
    /// Emergency admin address (separate from regular admin)
    pub emergency_admin: Address,
    /// Recovery threshold - number of backup admins required for recovery (#1251)
    pub recovery_threshold: u32,
    /// Backup admin addresses for recovery
    pub backup_admins: Vec<Address>,
    /// Flag indicating if a recovery is in progress
    pub recovery_in_progress: bool,
    /// Current contribution rules version (#1252)
    pub rules_version: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContributionRules {
    /// Exact amount required per contribution; 0 disables the fixed amount.
    pub fixed_amount: i128,
    /// Minimum amount per contribution; 0 disables the minimum.
    pub min_amount: i128,
    /// Maximum amount per contribution; 0 disables the maximum.
    pub max_amount: i128,
}

/// Versioned contribution rules for audit trail (#1252)
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VersionedContributionRules {
    pub version: u32,
    pub rules: ContributionRules,
    pub created_at: u64,
    pub created_by: Address,
    pub circle_id: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CircleConfiguration {
    pub asset: Address,
    pub asset_supported: bool,
    pub contribution_rules: ContributionRules,
    /// Maximum member count; 0 means no limit.
    pub max_members: u32,
    pub member_count: u32,
    pub max_payout: i128,
    pub withdrawal_threshold: u32,
    pub withdrawal_approvers: Vec<Address>,
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
    /// Rules version in force when request was made (#1252)
    pub rules_version: u32,
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
    Admin,
    SupportedAsset(Address),
    SupportedAssets,
    Count,
    Circle(u32),
    ContributionRules(u32),
    MaxMembers(u32),
    MemberCount(u32),
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
    // Emergency pause keys (#1250)
    GlobalPause,
    CirclePause(u32),
    // Recovery mechanism keys (#1251)
    RecoveryApproval(u32, Address),
    RecoveryAttemptCount(u32),
    // Rule version keys (#1252)
    RulesVersion(u32),
    RulesHistory(u32, u32),
    RulesVersionCount(u32),
}

// Helper to check if circle is paused (#1250)
fn check_not_paused(env: &Env, circle_id: u32) -> Result<(), Error> {
    let paused: bool = env
        .storage()
        .persistent()
        .get(&Key::CirclePause(circle_id))
        .unwrap_or(false);
    if paused {
        return Err(Error::EmergencyPaused);
    }
    Ok(())
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

fn load_contribution_rules(env: &Env, id: u32) -> ContributionRules {
    env.storage()
        .persistent()
        .get(&Key::ContributionRules(id))
        .unwrap_or(ContributionRules {
            fixed_amount: 0,
            min_amount: 0,
            max_amount: 0,
        })
}

fn validate_contribution_rules(
    fixed_amount: i128,
    min_amount: i128,
    max_amount: i128,
) -> Result<(), Error> {
    if fixed_amount < 0
        || min_amount < 0
        || max_amount < 0
        || (max_amount > 0 && min_amount > max_amount)
        || (fixed_amount > 0
            && ((min_amount > 0 && fixed_amount < min_amount)
                || (max_amount > 0 && fixed_amount > max_amount)))
    {
        return Err(Error::InvalidContributionRules);
    }
    Ok(())
}

fn validate_contribution_amount(env: &Env, id: u32, amount: i128) -> Result<(), Error> {
    if amount <= 0 {
        return Err(Error::InvalidAmount);
    }
    let rules = load_contribution_rules(env, id);
    if (rules.fixed_amount > 0 && amount != rules.fixed_amount)
        || (rules.min_amount > 0 && amount < rules.min_amount)
        || (rules.max_amount > 0 && amount > rules.max_amount)
    {
        return Err(Error::ContributionRuleViolation);
    }
    Ok(())
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

fn is_supported_asset(env: &Env, asset: &Address) -> bool {
    env.storage()
        .persistent()
        .get(&Key::SupportedAsset(asset.clone()))
        .unwrap_or(false)
}

fn require_supported_asset(env: &Env, asset: &Address) -> Result<(), Error> {
    if is_supported_asset(env, asset) {
        Ok(())
    } else {
        Err(Error::UnsupportedAsset)
    }
}

fn require_asset_admin(env: &Env, admin: &Address) -> Result<(), Error> {
    let configured_admin: Address = env
        .storage()
        .instance()
        .get(&Key::Admin)
        .ok_or(Error::NotInitialized)?;
    if admin != &configured_admin {
        return Err(Error::Unauthorized);
    }
    Ok(())
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
    /// One-time initialization. The administrator manages the supported token registry.
    pub fn initialize(env: Env, admin: Address) -> Result<(), Error> {
        admin.require_auth();
        if env.storage().instance().has(&Key::Admin) {
            return Err(Error::AlreadyInitialized);
        }
        env.storage().instance().set(&Key::Admin, &admin);
        env.storage()
            .persistent()
            .set(&Key::SupportedAssets, &Vec::<Address>::new(&env));
        Ok(())
    }

    /// Registers a Soroban token contract that circles may use.
    pub fn add_supported_asset(env: Env, admin: Address, asset: Address) -> Result<(), Error> {
        admin.require_auth();
        require_asset_admin(&env, &admin)?;

        if !is_supported_asset(&env, &asset) {
            let mut assets: Vec<Address> = env
                .storage()
                .persistent()
                .get(&Key::SupportedAssets)
                .unwrap_or(Vec::new(&env));
            assets.push_back(asset.clone());
            env.storage()
                .persistent()
                .set(&Key::SupportedAssets, &assets);
            env.storage()
                .persistent()
                .set(&Key::SupportedAsset(asset.clone()), &true);
            env.events().publish((symbol_short!("asset_add"),), asset);
        }
        Ok(())
    }

    /// Removes an asset only when no circle has an outstanding tracked balance or reservation.
    pub fn remove_supported_asset(env: Env, admin: Address, asset: Address) -> Result<(), Error> {
        admin.require_auth();
        require_asset_admin(&env, &admin)?;
        require_supported_asset(&env, &asset)?;
        if get_asset_bal(&env, &asset) != 0 || get_asset_res(&env, &asset) != 0 {
            return Err(Error::AssetInUse);
        }

        let current: Vec<Address> = env
            .storage()
            .persistent()
            .get(&Key::SupportedAssets)
            .unwrap_or(Vec::new(&env));
        let mut assets = Vec::new(&env);
        for registered_asset in current.iter() {
            if registered_asset != asset {
                assets.push_back(registered_asset);
            }
        }
        env.storage()
            .persistent()
            .set(&Key::SupportedAssets, &assets);
        env.storage()
            .persistent()
            .remove(&Key::SupportedAsset(asset.clone()));
        env.events().publish((symbol_short!("asset_rem"),), asset);
        Ok(())
    }

    /// Queries whether an asset is currently registered for circle operations.
    pub fn is_supported_asset(env: Env, asset: Address) -> bool {
        is_supported_asset(&env, &asset)
    }

    /// Queries all currently registered token contract addresses.
    pub fn get_supported_assets(env: Env) -> Vec<Address> {
        env.storage()
            .persistent()
            .get(&Key::SupportedAssets)
            .unwrap_or(Vec::new(&env))
    }

    /// Creates a circle. The admin becomes the first member and sole approver (threshold 1).
    pub fn create_circle(env: Env, admin: Address, token: Address, max_payout: i128) -> Result<u32, Error> {
        admin.require_auth();
        require_supported_asset(&env, &token)?;
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
            paused: false,
            emergency_admin: admin.clone(),
            recovery_threshold: 1,
            backup_admins: Vec::new(&env),
            recovery_in_progress: false,
            rules_version: 1,
        };

        // Initialize rule version history (#1252)
        env.storage().persistent().set(&Key::RulesVersionCount(id), &1u32);
        let initial_rules = VersionedContributionRules {
            version: 1,
            rules: ContributionRules {
                fixed_amount: 0,
                min_amount: 0,
                max_amount: 0,
            },
            created_at: env.ledger().timestamp(),
            created_by: admin.clone(),
            circle_id: id,
        };
        env.storage().persistent().set(&Key::RulesHistory(id, 1), &initial_rules);
        env.storage().persistent().set(&Key::RulesVersion(id), &1u32);
        save(&env, id, &c);
        env.storage().persistent().set(&Key::Member(id, admin.clone()), &true);
        env.storage().persistent().set(&Key::MaxMembers(id), &0u32);
        env.storage().persistent().set(&Key::MemberCount(id), &1u32);
        env.events().publish((symbol_short!("created"), id), admin);
        Ok(id)
    }

    pub fn join(env: Env, id: u32, member: Address) -> Result<(), Error> {
        member.require_auth();
        load(&env, id)?;
        if !is_member(&env, id, &member) {
            let max_members: u32 = env
                .storage()
                .persistent()
                .get(&Key::MaxMembers(id))
                .unwrap_or(0);
            let member_count: u32 = env
                .storage()
                .persistent()
                .get(&Key::MemberCount(id))
                .unwrap_or(1);
            if max_members > 0 && member_count >= max_members {
                return Err(Error::MembershipLimitReached);
            }
            let next_member_count = member_count.checked_add(1).ok_or(Error::Overflow)?;
            env.storage()
                .persistent()
                .set(&Key::MemberCount(id), &next_member_count);
        }
        env.storage().persistent().set(&Key::Member(id, member.clone()), &true);
        env.events().publish((symbol_short!("joined"), id), member);
        Ok(())
    }

    pub fn contribute(env: Env, id: u32, member: Address, amount: i128) -> Result<(), Error> {
        member.require_auth();
        // Emergency pause check - read-only queries remain available (#1250)
        check_not_paused(&env, id)?;
        let mut c = load(&env, id)?;
        if !is_member(&env, id, &member) {
            return Err(Error::NotMember);
        }
        require_supported_asset(&env, &c.token)?;
        validate_contribution_amount(&env, id, amount)?;

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
        // Emergency pause check
        check_not_paused(&env, id)?;
        let mut c = load(&env, id)?;
        if !is_member(&env, id, &member) {
            return Err(Error::NotMember);
        }
        require_supported_asset(&env, &c.token)?;
        validate_contribution_amount(&env, id, amount)?;

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

    // ── Emergency Pause Mechanism (#1250) ────────────────────────────────

    /// Emergency-only pause. Stops contributions, withdrawals, and governance changes.
    /// Only the designated emergency admin can pause.
    pub fn emergency_pause(env: Env, circle_id: u32, emergency_admin: Address) -> Result<(), Error> {
        emergency_admin.require_auth();
        let c = load(&env, circle_id)?;
        if emergency_admin != c.emergency_admin && emergency_admin != c.admin {
            return Err(Error::EmergencyAdminRequired);
        }

        let mut circle = c;
        circle.paused = true;
        save(&env, circle_id, &circle);

        env.storage().persistent().set(&Key::CirclePause(circle_id), &true);
        env.events().publish((symbol_short!("emrg_pause"), circle_id), emergency_admin);
        Ok(())
    }

    /// Emergency-only unpause. Only emergency admin can unpause.
    pub fn emergency_unpause(env: Env, circle_id: u32, emergency_admin: Address) -> Result<(), Error> {
        emergency_admin.require_auth();
        let c = load(&env, circle_id)?;
        if emergency_admin != c.emergency_admin && emergency_admin != c.admin {
            return Err(Error::EmergencyAdminRequired);
        }

        let mut circle = c;
        circle.paused = false;
        save(&env, circle_id, &circle);

        env.storage().persistent().set(&Key::CirclePause(circle_id), &false);
        env.events().publish((symbol_short!("emrg_unp"), circle_id), emergency_admin);
        Ok(())
    }

    /// Queries whether a circle is currently paused.
    pub fn is_paused(env: Env, circle_id: u32) -> Result<bool, Error> {
        load(&env, circle_id)?;
        Ok(env.storage().persistent().get(&Key::CirclePause(circle_id)).unwrap_or(false))
    }

    /// Sets the designated emergency admin (only current emergency admin or regular admin can set).
    pub fn set_emergency_admin(env: Env, circle_id: u32, admin: Address, new_emergency_admin: Address) -> Result<(), Error> {
        admin.require_auth();
        let mut c = load(&env, circle_id)?;
        if admin != c.admin && admin != c.emergency_admin {
            return Err(Error::Unauthorized);
        }
        c.emergency_admin = new_emergency_admin;
        save(&env, circle_id, &c);
        env.events().publish((symbol_short!("emrg_adm"), circle_id), new_emergency_admin);
        Ok(())
    }

    /// Queries the emergency admin address.
    pub fn get_emergency_admin(env: Env, circle_id: u32) -> Result<Address, Error> {
        let c = load(&env, circle_id)?;
        Ok(c.emergency_admin)
    }

    // ── Recovery Mechanism (#1251) ────────────────────────────────────────

    /// Sets backup admins and recovery threshold (only regular admin can configure).
    pub fn configure_recovery(
        env: Env,
        circle_id: u32,
        admin: Address,
        backup_admins: Vec<Address>,
        recovery_threshold: u32,
    ) -> Result<(), Error> {
        admin.require_auth();
        let mut c = load(&env, circle_id)?;
        if admin != c.admin {
            return Err(Error::Unauthorized);
        }
        if recovery_threshold > backup_admins.len() {
            return Err(Error::RecoveryThresholdNotMet);
        }

        c.backup_admins = backup_admins;
        c.recovery_threshold = recovery_threshold;
        save(&env, circle_id, &c);

        env.events().publish((symbol_short!("rcvry_cfg"), circle_id), recovery_threshold);
        Ok(())
    }

    /// Initiates recovery by setting recovery_in_progress flag.
    /// Requires multi-signature: backup admins must approve.
    pub fn initiate_recovery(env: Env, circle_id: u32, initiator: Address) -> Result<(), Error> {
        initiator.require_auth();
        let c = load(&env, circle_id)?;

        if !c.backup_admins.contains(&initiator) {
            return Err(Error::RecoveryNotAuthorized);
        }

        let mut circle = c;
        if circle.recovery_in_progress {
            return Err(Error::RecoveryInProgress);
        }
        circle.recovery_in_progress = true;
        save(&env, circle_id, &circle);

        // Record this backup admin's approval
        env.storage().persistent().set(&Key::RecoveryApproval(circle_id, initiator.clone()), &true);

        env.events().publish((symbol_short!("rcvry_init"), circle_id), initiator);
        Ok(())
    }

    /// Approves recovery by another backup admin (accumulates approvals).
    pub fn approve_recovery(env: Env, circle_id: u32, approver: Address) -> Result<bool, Error> {
        approver.require_auth();
        let c = load(&env, circle_id)?;

        if !c.backup_admins.contains(&approver) {
            return Err(Error::RecoveryNotAuthorized);
        }

        // Check if already approved by this approver
        let already_approved: bool = env
            .storage()
            .persistent()
            .get(&Key::RecoveryApproval(circle_id, approver.clone()))
            .unwrap_or(false);

        if already_approved {
            return Ok(false);
        }

        env.storage().persistent().set(&Key::RecoveryApproval(circle_id, approver.clone()), &true);

        // Count total approvals
        let mut approval_count: u32 = 0;
        for backup in c.backup_admins.iter() {
            let approved: bool = env
                .storage()
                .persistent()
                .get(&Key::RecoveryApproval(circle_id, backup))
                .unwrap_or(false);
            if approved {
                approval_count += 1;
            }
        }

        env.events().publish((symbol_short!("rcvry_appr"), circle_id), approval_count);

        // Check if threshold met
        Ok(approval_count >= c.recovery_threshold)
    }

    /// Executes recovery action (transfers admin rights to specified new admin).
    /// Requires recovery threshold to be met.
    pub fn execute_recovery(
        env: Env,
        circle_id: u32,
        approver: Address,
        new_admin: Address,
    ) -> Result<(), Error> {
        approver.require_auth();
        let mut c = load(&env, circle_id)?;

        if !c.backup_admins.contains(&approver) && approver != c.admin {
            return Err(Error::UnauthorizedRecoveryAttempt);
        }

        if !c.recovery_in_progress {
            return Err(Error::RecoveryNotAuthorized);
        }

        // Count approvals
        let mut approval_count: u32 = 0;
        for backup in c.backup_admins.iter() {
            let approved: bool = env
                .storage()
                .persistent()
                .get(&Key::RecoveryApproval(circle_id, backup))
                .unwrap_or(false);
            if approved {
                approval_count += 1;
            }
        }

        if approval_count < c.recovery_threshold {
            return Err(Error::RecoveryThresholdNotMet);
        }

        // Execute recovery: transfer admin rights
        let old_admin = c.admin;
        c.admin = new_admin;
        c.recovery_in_progress = false;
        save(&env, circle_id, &c);

        // Clear approvals
        for backup in c.backup_admins.iter() {
            env.storage().persistent().remove(&Key::RecoveryApproval(circle_id, backup));
        }

        env.events().publish((symbol_short!("rcvry_exec"), circle_id), (old_admin, new_admin));
        Ok(())
    }

    /// Cancels an in-progress recovery (only current admin can cancel).
    pub fn cancel_recovery(env: Env, circle_id: u32, admin: Address) -> Result<(), Error> {
        admin.require_auth();
        let mut c = load(&env, circle_id)?;
        if admin != c.admin {
            return Err(Error::Unauthorized);
        }

        if !c.recovery_in_progress {
            return Ok(());
        }

        c.recovery_in_progress = false;
        save(&env, circle_id, &c);

        // Clear approvals
        for backup in c.backup_admins.iter() {
            env.storage().persistent().remove(&Key::RecoveryApproval(circle_id, backup));
        }

        env.events().publish((symbol_short!("rcvry_cnl"), circle_id), admin);
        Ok(())
    }

    /// Queries recovery configuration.
    pub fn get_recovery_config(env: Env, circle_id: u32) -> Result<(u32, Vec<Address>, bool), Error> {
        let c = load(&env, circle_id)?;
        Ok((c.recovery_threshold, c.backup_admins.clone(), c.recovery_in_progress))
    }

    // ── Rule Versioning (#1252) ───────────────────────────────────────────

    /// Updates contribution rules and creates a new version.
    /// Existing requests retain their agreed rule context.
    pub fn set_contribution_rules(
        env: Env,
        id: u32,
        admin: Address,
        fixed_amount: i128,
        min_amount: i128,
        max_amount: i128,
    ) -> Result<u32, Error> {
        admin.require_auth();
        let circle = load(&env, id)?;
        if admin != circle.admin {
            return Err(Error::Unauthorized);
        }
        validate_contribution_rules(fixed_amount, min_amount, max_amount)?;

        // Get current version and increment
        let current_version: u32 = env.storage().persistent().get(&Key::RulesVersion(id)).unwrap_or(1);
        let new_version = current_version.checked_add(1).ok_or(Error::Overflow)?;

        // Store new versioned rules
        let new_rules = VersionedContributionRules {
            version: new_version,
            rules: ContributionRules {
                fixed_amount,
                min_amount,
                max_amount,
            },
            created_at: env.ledger().timestamp(),
            created_by: admin.clone(),
            circle_id: id,
        };
        env.storage().persistent().set(&Key::RulesHistory(id, new_version), &new_rules);
        env.storage().persistent().set(&Key::RulesVersion(id), &new_version);

        // Update version count
        let count: u32 = env.storage().persistent().get(&Key::RulesVersionCount(id)).unwrap_or(1);
        env.storage().persistent().set(&Key::RulesVersionCount(id), &count.saturating_add(1));

        // Also update current rules for efficient lookup
        env.storage()
            .persistent()
            .set(&Key::ContributionRules(id), &ContributionRules {
                fixed_amount,
                min_amount,
                max_amount,
            });

        env.events().publish((symbol_short!("rules_v"),), new_version);
        Ok(new_version)
    }

    /// Queries the circle's contribution rules; unset rules are returned as zeros.
    pub fn get_contribution_rules(env: Env, id: u32) -> Result<ContributionRules, Error> {
        load(&env, id)?;
        Ok(load_contribution_rules(&env, id))
    }

    /// Queries the current rule version number.
    pub fn get_rules_version(env: Env, id: u32) -> Result<u32, Error> {
        load(&env, id)?;
        Ok(env.storage().persistent().get(&Key::RulesVersion(id)).unwrap_or(1))
    }

    /// Queries historical rules by version number.
    pub fn get_rules_at_version(env: Env, id: u32, version: u32) -> Result<VersionedContributionRules, Error> {
        load(&env, id)?;
        let max_version: u32 = env.storage().persistent().get(&Key::RulesVersionCount(id)).unwrap_or(1);
        if version == 0 || version > max_version {
            return Err(Error::InvalidRuleVersion);
        }
        env.storage()
            .persistent()
            .get(&Key::RulesHistory(id, version))
            .ok_or(Error::NotFound)
    }

    /// Queries all historical rule versions for a circle.
    pub fn get_rules_history(
        env: Env,
        id: u32,
        start_version: u32,
        limit: u32,
    ) -> Result<Vec<VersionedContributionRules>, Error> {
        load(&env, id)?;
        let count: u32 = env.storage().persistent().get(&Key::RulesVersionCount(id)).unwrap_or(1);

        let mut versions = Vec::new(&env);
        if start_version == 0 || start_version > count || limit == 0 {
            return Ok(versions);
        }

        let safe_limit = if limit > 20 { 20 } else { limit };
        let end = (start_version.saturating_add(safe_limit)).min(count);

        for v in start_version..=end {
            if let Some(rules) = env.storage().persistent().get::<_, VersionedContributionRules>(&Key::RulesHistory(id, v)) {
                versions.push_back(rules);
            }
        }

        Ok(versions)
    }

    /// Admin-only. Sets the maximum number of circle members; 0 means unlimited.
    pub fn set_member_limit(env: Env, id: u32, admin: Address, max_members: u32) -> Result<(), Error> {
        admin.require_auth();
        let circle = load(&env, id)?;
        if admin != circle.admin {
            return Err(Error::Unauthorized);
        }
        let member_count: u32 = env
            .storage()
            .persistent()
            .get(&Key::MemberCount(id))
            .unwrap_or(1);
        if max_members > 0 && max_members < member_count {
            return Err(Error::InvalidConfiguration);
        }
        env.storage()
            .persistent()
            .set(&Key::MaxMembers(id), &max_members);
        env.events().publish((symbol_short!("mem_cap"), id), max_members);
        Ok(())
    }

    /// Queries all circle-level financial, asset, membership, and withdrawal configuration.
    pub fn get_circle_configuration(env: Env, id: u32) -> Result<CircleConfiguration, Error> {
        let circle = load(&env, id)?;
        Ok(CircleConfiguration {
            asset_supported: is_supported_asset(&env, &circle.token),
            asset: circle.token,
            contribution_rules: load_contribution_rules(&env, id),
            max_members: env
                .storage()
                .persistent()
                .get(&Key::MaxMembers(id))
                .unwrap_or(0),
            member_count: env
                .storage()
                .persistent()
                .get(&Key::MemberCount(id))
                .unwrap_or(1),
            max_payout: circle.max_payout,
            withdrawal_threshold: circle.threshold,
            withdrawal_approvers: circle.approvers,
        })
    }

    /// Admin-only. Sets full governance: threshold and approvers list. Existing requests keep their snapshot threshold.
    pub fn set_governance(env: Env, id: u32, admin: Address, threshold: u32, approvers: Vec<Address>) -> Result<(), Error> {
        admin.require_auth();
        // Emergency pause check - governance blocked during emergency
        check_not_paused(&env, id)?;
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
        // Emergency pause check - governance changes blocked during emergency
        check_not_paused(&env, id)?;
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
        // Emergency pause check - governance changes blocked
        check_not_paused(&env, id)?;
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
        // Emergency pause check - governance changes blocked
        check_not_paused(&env, id)?;
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
        // Emergency pause check - allows read-only queries (#1250)
        check_not_paused(&env, id)?;
        let mut c = load(&env, id)?;
        if !is_member(&env, id, &member) {
            return Err(Error::NotMember);
        }
        require_supported_asset(&env, &c.token)?;
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

        // Get current rules version for this request (#1252)
        let current_rules_version: u32 = env
            .storage()
            .persistent()
            .get(&Key::RulesVersion(id))
            .unwrap_or(1);

        let p = Payout {
            requester: member.clone(),
            to,
            amount,
            asset: c.token.clone(),
            status: RequestStatus::Pending,
            threshold: c.threshold,
            approvals: Vec::new(&env),
            executed: false,
            rules_version: current_rules_version,
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
    /// Emergency controls cannot be abused to bypass withdrawal protections (#1250).
    pub fn execute(env: Env, id: u32, rid: u32) -> Result<(), Error> {
        // Check if circle is paused - withdrawals protected during emergency
        check_not_paused(&env, id)?;
        let mut c = load(&env, id)?;
        let mut p: Payout = env.storage().persistent().get(&Key::Payout(id, rid)).ok_or(Error::NotFound)?;
        if p.executed {
            return Err(Error::AlreadyExecuted);
        }
        if p.approvals.len() < p.threshold {
            return Err(Error::NotApproved);
        }
        require_supported_asset(&env, &p.asset)?;
        if p.asset != c.token {
            return Err(Error::UnsupportedAsset);
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
        let member_count: u32 = env
            .storage()
            .persistent()
            .get(&Key::MemberCount(id))
            .unwrap_or(1);
        let next_member_count = member_count.checked_sub(1).ok_or(Error::InvalidConfiguration)?;
        env.storage().persistent().set(&Key::Member(id, member.clone()), &false);
        env.storage()
            .persistent()
            .set(&Key::MemberCount(id), &next_member_count);
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

    /// Queries the approval progress of a payout request. Read-only - works even when paused (#1250).
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
    /// Goal creation emits events (#1240).
    pub fn create_goal(
        env: Env,
        circle_id: u32,
        creator: Address,
        target_amount: i128,
        deadline: u64,
    ) -> Result<u32, Error> {
        creator.require_auth();
        // Emergency pause check
        check_not_paused(&env, circle_id)?;
        let c = load(&env, circle_id)?;
        if !is_member(&env, circle_id, &creator) {
            return Err(Error::NotMember);
        }
        require_supported_asset(&env, &c.token)?;
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
        // Emergency pause check
        check_not_paused(&env, circle_id)?;
        let mut c = load(&env, circle_id)?;
        if !is_member(&env, circle_id, &member) {
            return Err(Error::NotMember);
        }
        require_supported_asset(&env, &c.token)?;
        validate_contribution_amount(&env, circle_id, amount)?;

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
