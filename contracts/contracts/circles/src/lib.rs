#![no_std]
#![allow(unexpected_cfgs)]
//! Zaps Circles: a single savings/payment circle per contract instance.
//!
//! - Issue 73: single-use, validated, creator-authorized initialization.
//! - Issue 74: membership storage, governance-gated add/remove, limits and
//!   verification helpers used by financial actions.
//! - Issue 75: on-chain roles (`Owner`, `Admin`, `Member`), permission gates,
//!   role-change events and administrative recovery paths.

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, Address, Env, String, Symbol, Vec,
};

#[cfg(test)]
mod test;

/// Smallest circle that can be configured (an owner plus at least one peer).
pub const MIN_MEMBERS: u32 = 2;
/// Upper bound on `max_members` to keep per-circle storage bounded.
pub const MAX_MEMBERS_CAP: u32 = 1_000;
/// Maximum length, in bytes, of a circle name.
pub const MAX_NAME_LEN: u32 = 64;
/// Maximum number of supported assets per circle.
pub const MAX_ASSETS: u32 = 10;
/// Delay an `Admin` must wait before a recovery request can take ownership.
pub const RECOVERY_DELAY_SECS: u64 = 7 * 86_400; // 7 days

/// ~7 days at 5s/ledger: extend storage once the remaining TTL drops below this.
const TTL_THRESHOLD: u32 = 120_960;
/// ~30 days at 5s/ledger: TTL that storage entries are extended to.
const TTL_EXTEND_TO: u32 = 518_400;

#[contracttype]
#[derive(Clone)]
enum DataKey {
    // ── Instance storage ──
    Owner,
    Settings,
    Assets,
    MemberCount,
    CreatedAt,
    PendingOwner,
    Recovery,
    WithdrawalCount,
    // ── Persistent storage ──
    Member(Address),
    Withdrawal(u64),
}

/// On-chain role held by a circle participant.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Role {
    /// Regular participant: may contribute and receive withdrawals.
    Member,
    /// Manages membership and authorizes withdrawals.
    Admin,
    /// Full control, including financial rules and role assignment.
    Owner,
}

/// Actions gated by role.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Permission {
    /// Take part in the circle's financial activity (any member).
    Participate,
    /// Add or remove members (`Admin`, `Owner`).
    ManageMembers,
    /// Approve withdrawals from the circle (`Admin`, `Owner`).
    AuthorizeWithdrawal,
    /// Change settings and supported assets (`Owner`).
    ModifyFinancialRules,
    /// Grant or revoke the `Admin` role (`Owner`).
    ManageRoles,
}

/// Configurable circle parameters.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CircleSettings {
    /// Human-readable circle name, 1..=MAX_NAME_LEN bytes.
    pub name: String,
    /// Maximum number of members, MIN_MEMBERS..=MAX_MEMBERS_CAP.
    pub max_members: u32,
    /// Expected contribution per round, in the asset's smallest unit.
    pub contribution_amount: i128,
    /// Largest amount a single withdrawal authorization may cover.
    pub max_withdrawal: i128,
}

/// A pending, timelocked request by an `Admin` to take over ownership.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoveryRequest {
    pub candidate: Address,
    pub unlocks_at: u64,
}

/// A withdrawal approved by an `Admin` or `Owner`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WithdrawalAuthorization {
    pub id: u64,
    pub member: Address,
    pub asset: Address,
    pub amount: i128,
    pub approved_by: Address,
    pub approved_at: u64,
}

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    /// The circle has not been initialized yet.
    NotInitialized = 1,
    /// The circle was already initialized; initialization is single-use.
    AlreadyInitialized = 2,
    /// Circle name is empty or longer than MAX_NAME_LEN.
    InvalidName = 3,
    /// `max_members` is out of range or below the current member count.
    InvalidMaxMembers = 4,
    /// An amount is zero or negative.
    InvalidAmount = 5,
    /// At least one supported asset is required.
    NoSupportedAssets = 6,
    /// More than MAX_ASSETS supported assets were supplied.
    TooManyAssets = 7,
    /// The same asset appears more than once.
    DuplicateAsset = 8,
    /// Caller's role does not grant the required permission.
    Unauthorized = 9,
    /// The account is already a member of the circle.
    AlreadyMember = 10,
    /// The account is not a member of the circle.
    NotMember = 11,
    /// The circle is at `max_members`.
    MembershipLimitReached = 12,
    /// The owner cannot be removed, demoted or leave the circle.
    CannotRemoveOwner = 13,
    /// The requested role assignment is not allowed.
    InvalidRole = 14,
    /// The asset is not supported by this circle.
    UnsupportedAsset = 15,
    /// Requested withdrawal is above `max_withdrawal`.
    WithdrawalLimitExceeded = 16,
    /// There is no pending ownership transfer for this account.
    NoPendingTransfer = 17,
    /// There is no pending recovery request.
    NoPendingRecovery = 18,
    /// The recovery timelock has not elapsed.
    RecoveryTimelockActive = 19,
    /// A recovery request is already pending.
    RecoveryAlreadyPending = 20,
    /// The target of the action is invalid (e.g. the caller themselves).
    InvalidTarget = 21,
    /// A counter overflowed.
    Overflow = 22,
}

#[contract]
pub struct CirclesContract;

#[contractimpl]
impl CirclesContract {
    // ─── Issue 73: initialization ───────────────────────────────────────────

    /// Initialize the circle. May only be called once.
    ///
    /// `creator` becomes the `Owner`. `admin` is the initial administrator and
    /// is granted the `Admin` role; passing the creator's own address leaves
    /// the owner as the sole administrator. Both accounts must authorize.
    pub fn initialize(
        env: Env,
        creator: Address,
        admin: Address,
        settings: CircleSettings,
        assets: Vec<Address>,
    ) -> Result<(), Error> {
        if Self::is_initialized(env.clone()) {
            return Err(Error::AlreadyInitialized);
        }
        creator.require_auth();
        if admin != creator {
            admin.require_auth();
        }

        Self::validate_settings(&settings)?;
        Self::validate_assets(&assets)?;

        let storage = env.storage().instance();
        storage.set(&DataKey::Owner, &creator);
        storage.set(&DataKey::Settings, &settings);
        storage.set(&DataKey::Assets, &assets);
        storage.set(&DataKey::CreatedAt, &env.ledger().timestamp());
        storage.set(&DataKey::WithdrawalCount, &0u64);

        Self::write_role(&env, &creator, Role::Owner);
        let mut count: u32 = 1;
        if admin != creator {
            Self::write_role(&env, &admin, Role::Admin);
            count = 2;
        }
        storage.set(&DataKey::MemberCount, &count);
        Self::extend_instance(&env);

        env.events().publish(
            (Symbol::new(&env, "CircleInitialized"), creator.clone()),
            (admin.clone(), settings.max_members, assets.len()),
        );
        if admin != creator {
            env.events().publish(
                (Symbol::new(&env, "RoleChanged"), admin),
                (creator, Option::<Role>::None, Some(Role::Admin)),
            );
        }
        Ok(())
    }

    pub fn is_initialized(env: Env) -> bool {
        env.storage().instance().has(&DataKey::Owner)
    }

    pub fn get_owner(env: Env) -> Result<Address, Error> {
        env.storage()
            .instance()
            .get(&DataKey::Owner)
            .ok_or(Error::NotInitialized)
    }

    pub fn get_settings(env: Env) -> Result<CircleSettings, Error> {
        env.storage()
            .instance()
            .get(&DataKey::Settings)
            .ok_or(Error::NotInitialized)
    }

    pub fn get_supported_assets(env: Env) -> Result<Vec<Address>, Error> {
        env.storage()
            .instance()
            .get(&DataKey::Assets)
            .ok_or(Error::NotInitialized)
    }

    pub fn is_supported_asset(env: Env, asset: Address) -> bool {
        match Self::get_supported_assets(env) {
            Ok(assets) => assets.iter().any(|a| a == asset),
            Err(_) => false,
        }
    }

    pub fn created_at(env: Env) -> Result<u64, Error> {
        env.storage()
            .instance()
            .get(&DataKey::CreatedAt)
            .ok_or(Error::NotInitialized)
    }

    // ─── Issue 73/75: financial rules (Owner only) ──────────────────────────

    /// Replace the circle settings. `max_members` may not drop below the
    /// current member count.
    pub fn update_settings(
        env: Env,
        caller: Address,
        settings: CircleSettings,
    ) -> Result<(), Error> {
        caller.require_auth();
        Self::require_permission(&env, &caller, Permission::ModifyFinancialRules)?;
        Self::validate_settings(&settings)?;
        if settings.max_members < Self::read_member_count(&env) {
            return Err(Error::InvalidMaxMembers);
        }

        env.storage().instance().set(&DataKey::Settings, &settings);
        Self::extend_instance(&env);
        env.events().publish(
            (Symbol::new(&env, "SettingsUpdated"), caller),
            (
                settings.max_members,
                settings.contribution_amount,
                settings.max_withdrawal,
            ),
        );
        Ok(())
    }

    /// Replace the list of supported assets.
    pub fn set_supported_assets(
        env: Env,
        caller: Address,
        assets: Vec<Address>,
    ) -> Result<(), Error> {
        caller.require_auth();
        Self::require_permission(&env, &caller, Permission::ModifyFinancialRules)?;
        Self::validate_assets(&assets)?;

        env.storage().instance().set(&DataKey::Assets, &assets);
        Self::extend_instance(&env);
        env.events().publish(
            (Symbol::new(&env, "AssetsUpdated"), caller),
            assets.len(),
        );
        Ok(())
    }

    // ─── Issue 74: membership ───────────────────────────────────────────────

    /// Add `member` as a regular `Member`. Requires `ManageMembers`.
    pub fn add_member(env: Env, caller: Address, member: Address) -> Result<(), Error> {
        caller.require_auth();
        Self::require_permission(&env, &caller, Permission::ManageMembers)?;
        if Self::read_role(&env, &member).is_some() {
            return Err(Error::AlreadyMember);
        }

        let settings = Self::get_settings(env.clone())?;
        let count = Self::read_member_count(&env);
        if count >= settings.max_members {
            return Err(Error::MembershipLimitReached);
        }
        let new_count = count.checked_add(1).ok_or(Error::Overflow)?;

        Self::write_role(&env, &member, Role::Member);
        env.storage()
            .instance()
            .set(&DataKey::MemberCount, &new_count);
        Self::extend_instance(&env);

        env.events().publish(
            (Symbol::new(&env, "MemberAdded"), member),
            (caller, new_count),
        );
        Ok(())
    }

    /// Remove `member`. Requires `ManageMembers`; only the `Owner` may remove
    /// an `Admin`, and the `Owner` can never be removed.
    pub fn remove_member(env: Env, caller: Address, member: Address) -> Result<(), Error> {
        caller.require_auth();
        let caller_role = Self::require_permission(&env, &caller, Permission::ManageMembers)?;
        let target_role = Self::read_role(&env, &member).ok_or(Error::NotMember)?;
        match target_role {
            Role::Owner => return Err(Error::CannotRemoveOwner),
            Role::Admin if caller_role != Role::Owner && caller != member => {
                return Err(Error::Unauthorized)
            }
            _ => {}
        }

        let new_count = Self::delete_member(&env, &member)?;
        env.events().publish(
            (Symbol::new(&env, "MemberRemoved"), member),
            (caller, new_count),
        );
        Ok(())
    }

    /// Voluntarily leave the circle. The `Owner` must transfer ownership first.
    pub fn leave_circle(env: Env, member: Address) -> Result<(), Error> {
        member.require_auth();
        let role = Self::read_role(&env, &member).ok_or(Error::NotMember)?;
        if role == Role::Owner {
            return Err(Error::CannotRemoveOwner);
        }

        let new_count = Self::delete_member(&env, &member)?;
        env.events().publish(
            (Symbol::new(&env, "MemberRemoved"), member.clone()),
            (member, new_count),
        );
        Ok(())
    }

    pub fn is_member(env: Env, account: Address) -> bool {
        Self::read_role(&env, &account).is_some()
    }

    pub fn member_count(env: Env) -> u32 {
        Self::read_member_count(&env)
    }

    /// Verification helper for financial actions: succeeds only if the circle
    /// is initialized and `account` is a member.
    pub fn verify_member(env: Env, account: Address) -> Result<(), Error> {
        Self::require_member(&env, &account).map(|_| ())
    }

    // ─── Issue 75: roles ────────────────────────────────────────────────────

    pub fn get_role(env: Env, account: Address) -> Option<Role> {
        Self::read_role(&env, &account)
    }

    pub fn has_permission(env: Env, account: Address, permission: Permission) -> bool {
        match Self::read_role(&env, &account) {
            Some(role) => Self::role_allows(role, permission),
            None => false,
        }
    }

    /// Grant (`Admin`) or revoke (`Member`) administrative rights. Requires
    /// `ManageRoles`. Ownership changes go through the transfer/recovery flows.
    pub fn set_role(env: Env, caller: Address, account: Address, role: Role) -> Result<(), Error> {
        caller.require_auth();
        Self::require_permission(&env, &caller, Permission::ManageRoles)?;
        if role == Role::Owner {
            return Err(Error::InvalidRole);
        }
        let old_role = Self::read_role(&env, &account).ok_or(Error::NotMember)?;
        if old_role == Role::Owner {
            return Err(Error::CannotRemoveOwner);
        }
        if old_role == role {
            return Err(Error::InvalidRole);
        }

        Self::write_role(&env, &account, role);
        env.events().publish(
            (Symbol::new(&env, "RoleChanged"), account),
            (caller, Some(old_role), Some(role)),
        );
        Ok(())
    }

    // ─── Issue 75: withdrawals ──────────────────────────────────────────────

    /// Approve a withdrawal of `amount` of `asset` for `member`. Requires
    /// `AuthorizeWithdrawal`; the recipient must be a member.
    pub fn authorize_withdrawal(
        env: Env,
        caller: Address,
        member: Address,
        asset: Address,
        amount: i128,
    ) -> Result<u64, Error> {
        caller.require_auth();
        Self::require_permission(&env, &caller, Permission::AuthorizeWithdrawal)?;
        Self::require_member(&env, &member)?;
        if !Self::is_supported_asset(env.clone(), asset.clone()) {
            return Err(Error::UnsupportedAsset);
        }
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }
        if amount > Self::get_settings(env.clone())?.max_withdrawal {
            return Err(Error::WithdrawalLimitExceeded);
        }

        let storage = env.storage().instance();
        let id: u64 = storage
            .get::<_, u64>(&DataKey::WithdrawalCount)
            .unwrap_or(0)
            .checked_add(1)
            .ok_or(Error::Overflow)?;
        storage.set(&DataKey::WithdrawalCount, &id);

        let record = WithdrawalAuthorization {
            id,
            member: member.clone(),
            asset: asset.clone(),
            amount,
            approved_by: caller.clone(),
            approved_at: env.ledger().timestamp(),
        };
        let key = DataKey::Withdrawal(id);
        env.storage().persistent().set(&key, &record);
        env.storage()
            .persistent()
            .extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
        Self::extend_instance(&env);

        env.events().publish(
            (Symbol::new(&env, "WithdrawalAuthorized"), member),
            (id, asset, amount, caller),
        );
        Ok(id)
    }

    pub fn get_withdrawal(env: Env, id: u64) -> Option<WithdrawalAuthorization> {
        env.storage().persistent().get(&DataKey::Withdrawal(id))
    }

    // ─── Issue 75: ownership transfer (owner-initiated recovery path) ───────

    /// Propose `new_owner` (an existing member) as the next owner. The
    /// transfer completes once `new_owner` calls `accept_ownership`.
    pub fn transfer_ownership(env: Env, owner: Address, new_owner: Address) -> Result<(), Error> {
        owner.require_auth();
        Self::require_owner(&env, &owner)?;
        if new_owner == owner {
            return Err(Error::InvalidTarget);
        }
        Self::require_member(&env, &new_owner)?;

        env.storage()
            .instance()
            .set(&DataKey::PendingOwner, &new_owner);
        Self::extend_instance(&env);
        env.events().publish(
            (Symbol::new(&env, "OwnershipTransferProposed"), owner),
            new_owner,
        );
        Ok(())
    }

    pub fn cancel_ownership_transfer(env: Env, owner: Address) -> Result<(), Error> {
        owner.require_auth();
        Self::require_owner(&env, &owner)?;
        if !env.storage().instance().has(&DataKey::PendingOwner) {
            return Err(Error::NoPendingTransfer);
        }
        env.storage().instance().remove(&DataKey::PendingOwner);
        env.events().publish(
            (Symbol::new(&env, "OwnershipTransferCancelled"), owner),
            (),
        );
        Ok(())
    }

    /// Complete a pending transfer. The previous owner is demoted to `Admin`.
    pub fn accept_ownership(env: Env, new_owner: Address) -> Result<(), Error> {
        new_owner.require_auth();
        let pending: Option<Address> = env.storage().instance().get(&DataKey::PendingOwner);
        if pending != Some(new_owner.clone()) {
            return Err(Error::NoPendingTransfer);
        }
        Self::require_member(&env, &new_owner)?;

        let old_owner = Self::get_owner(env.clone())?;
        Self::set_owner(&env, &old_owner, &new_owner, Role::Admin);
        env.events().publish(
            (Symbol::new(&env, "OwnershipTransferred"), new_owner),
            old_owner,
        );
        Ok(())
    }

    pub fn get_pending_owner(env: Env) -> Option<Address> {
        env.storage().instance().get(&DataKey::PendingOwner)
    }

    // ─── Issue 75: timelocked admin recovery (owner key lost) ───────────────

    /// An `Admin` starts a recovery request. After RECOVERY_DELAY_SECS the
    /// admin may take ownership unless the owner cancels in the meantime.
    pub fn initiate_recovery(env: Env, admin: Address) -> Result<u64, Error> {
        admin.require_auth();
        if Self::read_role(&env, &admin) != Some(Role::Admin) {
            return Err(Error::Unauthorized);
        }
        if env.storage().instance().has(&DataKey::Recovery) {
            return Err(Error::RecoveryAlreadyPending);
        }

        let unlocks_at = env
            .ledger()
            .timestamp()
            .checked_add(RECOVERY_DELAY_SECS)
            .ok_or(Error::Overflow)?;
        let request = RecoveryRequest {
            candidate: admin.clone(),
            unlocks_at,
        };
        env.storage().instance().set(&DataKey::Recovery, &request);
        Self::extend_instance(&env);
        env.events().publish(
            (Symbol::new(&env, "RecoveryInitiated"), admin),
            unlocks_at,
        );
        Ok(unlocks_at)
    }

    /// Cancel a pending recovery. Callable by the `Owner` or the candidate.
    pub fn cancel_recovery(env: Env, caller: Address) -> Result<(), Error> {
        caller.require_auth();
        let request: RecoveryRequest = env
            .storage()
            .instance()
            .get(&DataKey::Recovery)
            .ok_or(Error::NoPendingRecovery)?;
        if caller != request.candidate && Self::read_role(&env, &caller) != Some(Role::Owner) {
            return Err(Error::Unauthorized);
        }

        env.storage().instance().remove(&DataKey::Recovery);
        env.events().publish(
            (Symbol::new(&env, "RecoveryCancelled"), request.candidate),
            caller,
        );
        Ok(())
    }

    /// Finalize recovery once the timelock has elapsed. The candidate becomes
    /// `Owner` and the previous owner is demoted to `Member`.
    pub fn execute_recovery(env: Env, admin: Address) -> Result<(), Error> {
        admin.require_auth();
        let request: RecoveryRequest = env
            .storage()
            .instance()
            .get(&DataKey::Recovery)
            .ok_or(Error::NoPendingRecovery)?;
        if request.candidate != admin || Self::read_role(&env, &admin) != Some(Role::Admin) {
            return Err(Error::Unauthorized);
        }
        if env.ledger().timestamp() < request.unlocks_at {
            return Err(Error::RecoveryTimelockActive);
        }

        let old_owner = Self::get_owner(env.clone())?;
        Self::set_owner(&env, &old_owner, &admin, Role::Member);
        env.events().publish(
            (Symbol::new(&env, "OwnershipRecovered"), admin),
            old_owner,
        );
        Ok(())
    }

    pub fn get_recovery(env: Env) -> Option<RecoveryRequest> {
        env.storage().instance().get(&DataKey::Recovery)
    }

    // ─── Internal helpers ───────────────────────────────────────────────────

    fn role_allows(role: Role, permission: Permission) -> bool {
        match permission {
            Permission::Participate => true,
            Permission::ManageMembers | Permission::AuthorizeWithdrawal => {
                matches!(role, Role::Admin | Role::Owner)
            }
            Permission::ModifyFinancialRules | Permission::ManageRoles => role == Role::Owner,
        }
    }

    /// Return the caller's role if it grants `permission`.
    fn require_permission(
        env: &Env,
        caller: &Address,
        permission: Permission,
    ) -> Result<Role, Error> {
        if !env.storage().instance().has(&DataKey::Owner) {
            return Err(Error::NotInitialized);
        }
        match Self::read_role(env, caller) {
            Some(role) if Self::role_allows(role, permission) => Ok(role),
            _ => Err(Error::Unauthorized),
        }
    }

    fn require_owner(env: &Env, caller: &Address) -> Result<(), Error> {
        if &Self::get_owner(env.clone())? != caller {
            return Err(Error::Unauthorized);
        }
        Ok(())
    }

    fn require_member(env: &Env, account: &Address) -> Result<Role, Error> {
        if !env.storage().instance().has(&DataKey::Owner) {
            return Err(Error::NotInitialized);
        }
        Self::read_role(env, account).ok_or(Error::NotMember)
    }

    fn read_role(env: &Env, account: &Address) -> Option<Role> {
        let key = DataKey::Member(account.clone());
        let role: Option<Role> = env.storage().persistent().get(&key);
        if role.is_some() {
            env.storage()
                .persistent()
                .extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
        }
        role
    }

    fn write_role(env: &Env, account: &Address, role: Role) {
        let key = DataKey::Member(account.clone());
        env.storage().persistent().set(&key, &role);
        env.storage()
            .persistent()
            .extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
    }

    fn delete_member(env: &Env, account: &Address) -> Result<u32, Error> {
        env.storage()
            .persistent()
            .remove(&DataKey::Member(account.clone()));
        let new_count = Self::read_member_count(env)
            .checked_sub(1)
            .ok_or(Error::Overflow)?;
        env.storage()
            .instance()
            .set(&DataKey::MemberCount, &new_count);

        // Drop any pending role elevation tied to the departing account.
        if env.storage().instance().get::<_, Address>(&DataKey::PendingOwner)
            == Some(account.clone())
        {
            env.storage().instance().remove(&DataKey::PendingOwner);
        }
        if let Some(request) = env
            .storage()
            .instance()
            .get::<_, RecoveryRequest>(&DataKey::Recovery)
        {
            if &request.candidate == account {
                env.storage().instance().remove(&DataKey::Recovery);
            }
        }
        Self::extend_instance(env);
        Ok(new_count)
    }

    /// Move ownership to `new_owner`, demoting the previous owner to
    /// `old_owner_role`, and clear any pending transfer or recovery.
    fn set_owner(env: &Env, old_owner: &Address, new_owner: &Address, old_owner_role: Role) {
        Self::write_role(env, old_owner, old_owner_role);
        Self::write_role(env, new_owner, Role::Owner);
        let storage = env.storage().instance();
        storage.set(&DataKey::Owner, new_owner);
        storage.remove(&DataKey::PendingOwner);
        storage.remove(&DataKey::Recovery);
        Self::extend_instance(env);

        env.events().publish(
            (Symbol::new(env, "RoleChanged"), old_owner.clone()),
            (new_owner.clone(), Some(Role::Owner), Some(old_owner_role)),
        );
    }

    fn read_member_count(env: &Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::MemberCount)
            .unwrap_or(0)
    }

    fn validate_settings(settings: &CircleSettings) -> Result<(), Error> {
        let name_len = settings.name.len();
        if name_len == 0 || name_len > MAX_NAME_LEN {
            return Err(Error::InvalidName);
        }
        if settings.max_members < MIN_MEMBERS || settings.max_members > MAX_MEMBERS_CAP {
            return Err(Error::InvalidMaxMembers);
        }
        if settings.contribution_amount <= 0 || settings.max_withdrawal <= 0 {
            return Err(Error::InvalidAmount);
        }
        Ok(())
    }

    fn validate_assets(assets: &Vec<Address>) -> Result<(), Error> {
        let len = assets.len();
        if len == 0 {
            return Err(Error::NoSupportedAssets);
        }
        if len > MAX_ASSETS {
            return Err(Error::TooManyAssets);
        }
        for i in 0..len {
            let asset = assets.get_unchecked(i);
            for j in (i + 1)..len {
                if assets.get_unchecked(j) == asset {
                    return Err(Error::DuplicateAsset);
                }
            }
        }
        Ok(())
    }

    fn extend_instance(env: &Env) {
        env.storage()
            .instance()
            .extend_ttl(TTL_THRESHOLD, TTL_EXTEND_TO);
    }
}
