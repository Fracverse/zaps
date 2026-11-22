//! Circle Membership & Role-Based Access Control (RBAC) Service (#1198, #1201).
//!
//! Provides centralized role definitions, permission matrix evaluation,
//! privilege escalation prevention, membership lifecycle management (join, leave,
//! approve, remove, eligibility checks), and persistent audit trails.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

// ── Roles & Permissions (#1201) ───────────────────────────────────────────────

/// Supported Circle Roles ordered by hierarchy rank.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CircleRole {
    Member = 1,
    Moderator = 2,
    Administrator = 3,
    Owner = 4,
}

impl CircleRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Owner => "owner",
            Self::Administrator => "admin",
            Self::Moderator => "moderator",
            Self::Member => "member",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "owner" => Some(Self::Owner),
            "admin" | "administrator" => Some(Self::Administrator),
            "moderator" => Some(Self::Moderator),
            "member" => Some(Self::Member),
            _ => None,
        }
    }

    /// Numerical hierarchy rank for privilege comparison.
    /// Higher number means higher authority.
    pub fn hierarchy_rank(&self) -> u8 {
        *self as u8
    }

    /// Centrally defined permission evaluation for each role.
    pub fn has_permission(&self, perm: CirclePermission) -> bool {
        match perm {
            CirclePermission::ManageSettings => matches!(self, Self::Owner | Self::Administrator),
            CirclePermission::InviteMembers => {
                matches!(self, Self::Owner | Self::Administrator | Self::Moderator)
            }
            CirclePermission::ModerateContent => {
                matches!(self, Self::Owner | Self::Administrator | Self::Moderator)
            }
            CirclePermission::ViewFinancialInfo => true, // All members can view financial info
            CirclePermission::ManagePayouts => matches!(self, Self::Owner | Self::Administrator),
            CirclePermission::ManageMembers => matches!(self, Self::Owner | Self::Administrator),
            CirclePermission::ManageRoles => matches!(self, Self::Owner | Self::Administrator),
            CirclePermission::Contribute => true,
        }
    }
}

/// Protected operational permissions within a circle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CirclePermission {
    ManageSettings,
    InviteMembers,
    ModerateContent,
    ViewFinancialInfo,
    ManagePayouts,
    ManageMembers,
    ManageRoles,
    Contribute,
}

/// Enforces privilege escalation prevention rules.
///
/// Returns `Ok(())` if `actor_role` is permitted to set `target_role` to `new_role`.
pub fn validate_role_transition(
    actor_address: &str,
    actor_role: CircleRole,
    target_address: &str,
    target_role: CircleRole,
    new_role: CircleRole,
) -> Result<(), &'static str> {
    // 1. Users cannot change their own role (self-elevation or self-demotion to bypass rules).
    if actor_address == target_address {
        return Err("cannot_modify_own_role");
    }

    // 2. Actor must hold role management permissions.
    if !actor_role.has_permission(CirclePermission::ManageRoles) {
        return Err("insufficient_permission");
    }

    // 3. Target is currently the Owner: only Owner can reassign ownership.
    if target_role == CircleRole::Owner && actor_role != CircleRole::Owner {
        return Err("cannot_modify_owner");
    }

    // 4. Assigning Owner role is an ownership transfer: only the current Owner can perform it.
    if new_role == CircleRole::Owner && actor_role != CircleRole::Owner {
        return Err("only_owner_can_transfer_ownership");
    }

    // 5. Actors cannot grant roles equal to or higher than their own authority rank.
    if actor_role != CircleRole::Owner && new_role.hierarchy_rank() >= actor_role.hierarchy_rank() {
        return Err("cannot_grant_role_equal_or_higher_than_own");
    }

    // 6. Actors cannot modify the role of someone with equal or higher rank (e.g. admin cannot modify admin).
    if actor_role != CircleRole::Owner
        && target_role.hierarchy_rank() >= actor_role.hierarchy_rank()
    {
        return Err("cannot_modify_equal_or_higher_role");
    }

    Ok(())
}

// ── Models & Errors ───────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct EligibilityResponse {
    pub eligible: bool,
    pub is_member: bool,
    pub member_limit_reached: bool,
    pub requires_invitation: bool,
    pub current_members: i64,
    pub member_limit: Option<i32>,
    pub reason: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MembershipHistoryEntry {
    pub id: Uuid,
    pub circle_id: Uuid,
    pub member_address: String,
    pub action: String,
    pub actor_address: String,
    pub old_role: Option<String>,
    pub new_role: Option<String>,
    pub reason: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MemberRoleEntry {
    pub address: String,
    pub role: String,
    pub joined_at: DateTime<Utc>,
    pub total_contributed: String,
}

// ── Service Implementation (#1198) ───────────────────────────────────────────

/// Fetch member's role in a circle. Returns `None` if address is not a member.
pub async fn get_member_role(
    pool: &PgPool,
    circle_id: Uuid,
    address: &str,
) -> Result<Option<CircleRole>, sqlx::Error> {
    let row = sqlx::query_scalar!(
        "SELECT role FROM circle_members WHERE circle_id = $1 AND address = $2",
        circle_id,
        address
    )
    .fetch_optional(pool)
    .await?;

    Ok(row.and_then(|r| CircleRole::from_str(&r)))
}

/// Verify that a user belongs to the circle and holds the required permission.
pub async fn require_circle_permission(
    pool: &PgPool,
    circle_id: Uuid,
    address: &str,
    perm: CirclePermission,
) -> Result<CircleRole, &'static str> {
    let role = get_member_role(pool, circle_id, address)
        .await
        .map_err(|_| "internal_error")?
        .ok_or("not_member")?;

    if role.has_permission(perm) {
        Ok(role)
    } else {
        Err("permission_denied")
    }
}

/// Check membership eligibility for a given user.
pub async fn check_eligibility(
    pool: &PgPool,
    circle_id: Uuid,
    address: &str,
) -> Result<EligibilityResponse, &'static str> {
    let circle = sqlx::query!(
        "SELECT visibility, member_limit FROM circles WHERE id = $1",
        circle_id
    )
    .fetch_optional(pool)
    .await
    .map_err(|_| "internal_error")?
    .ok_or("circle_not_found")?;

    let is_member = sqlx::query_scalar!(
        "SELECT 1 FROM circle_members WHERE circle_id = $1 AND address = $2",
        circle_id,
        address
    )
    .fetch_optional(pool)
    .await
    .map_err(|_| "internal_error")?
    .is_some();

    let current_members: i64 = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM circle_members WHERE circle_id = $1",
        circle_id
    )
    .fetch_one(pool)
    .await
    .map_err(|_| "internal_error")?
    .unwrap_or(0);

    let member_limit_reached = if let Some(limit) = circle.member_limit {
        current_members >= limit as i64
    } else {
        false
    };

    let has_invitation = if circle.visibility == "invite_only" {
        sqlx::query_scalar!(
            "SELECT 1 FROM circle_invitations WHERE circle_id = $1 AND invitee_address = $2 AND invite_type = 'direct' AND status = 'pending' AND (expires_at IS NULL OR expires_at > NOW())",
            circle_id,
            address
        )
        .fetch_optional(pool)
        .await
        .map_err(|_| "internal_error")?
        .is_some()
    } else {
        false
    };

    let requires_invitation = circle.visibility == "invite_only" && !has_invitation;
    let is_private = circle.visibility == "private";

    let (eligible, reason) = if is_member {
        (false, Some("already_member".to_string()))
    } else if member_limit_reached {
        (false, Some("member_limit_reached".to_string()))
    } else if is_private {
        (false, Some("private_circle".to_string()))
    } else if requires_invitation {
        (false, Some("invitation_required".to_string()))
    } else {
        (true, None)
    };

    Ok(EligibilityResponse {
        eligible,
        is_member,
        member_limit_reached,
        requires_invitation,
        current_members,
        member_limit: circle.member_limit,
        reason,
    })
}

/// Join a circle (atomic transaction with limit check, invitation check, and audit trail).
pub async fn join_circle(
    pool: &PgPool,
    circle_id: Uuid,
    address: &str,
) -> Result<(), &'static str> {
    let mut tx = pool.begin().await.map_err(|_| "internal_error")?;

    // Lock circle record to prevent race condition on member limits
    let circle = sqlx::query!(
        "SELECT visibility, member_limit FROM circles WHERE id = $1 FOR UPDATE",
        circle_id
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| "internal_error")?
    .ok_or("circle_not_found")?;

    // Check if user is already a member
    let is_member = sqlx::query_scalar!(
        "SELECT 1 FROM circle_members WHERE circle_id = $1 AND address = $2",
        circle_id,
        address
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| "internal_error")?
    .is_some();

    if is_member {
        let _ = tx.rollback().await;
        return Err("already_member");
    }

    // Check member limits
    let current_members: i64 = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM circle_members WHERE circle_id = $1",
        circle_id
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(|_| "internal_error")?
    .unwrap_or(0);

    if let Some(limit) = circle.member_limit {
        if current_members >= limit as i64 {
            let _ = tx.rollback().await;
            return Err("member_limit_reached");
        }
    }

    // Check visibility permissions
    if circle.visibility == "private" {
        let _ = tx.rollback().await;
        return Err("private_circle_direct_join_disabled");
    } else if circle.visibility == "invite_only" {
        let invite = sqlx::query!(
            "SELECT id FROM circle_invitations WHERE circle_id = $1 AND invitee_address = $2 AND invite_type = 'direct' AND status = 'pending' AND (expires_at IS NULL OR expires_at > NOW()) FOR UPDATE",
            circle_id,
            address
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| "internal_error")?;

        match invite {
            Some(inv) => {
                let _ = sqlx::query!(
                    "UPDATE circle_invitations SET status = 'accepted', responded_by = $2, responded_at = NOW(), updated_at = NOW() WHERE id = $1",
                    inv.id,
                    address
                )
                .execute(&mut *tx)
                .await;
            }
            None => {
                let _ = tx.rollback().await;
                return Err("invitation_required");
            }
        }
    }

    // Insert member with default role 'member'
    let insert_res = sqlx::query!(
        "INSERT INTO circle_members (circle_id, address, role) VALUES ($1, $2, 'member')",
        circle_id,
        address
    )
    .execute(&mut *tx)
    .await;

    if insert_res.is_err() {
        let _ = tx.rollback().await;
        return Err("already_member");
    }

    // Record membership history
    let _ = sqlx::query!(
        r#"
        INSERT INTO circle_membership_history (circle_id, member_address, action, actor_address, new_role)
        VALUES ($1, $2, 'joined', $2, 'member')
        "#,
        circle_id,
        address
    )
    .execute(&mut *tx)
    .await;

    // Record activity
    let _ = sqlx::query!(
        r#"
        INSERT INTO circle_activities (circle_id, actor_address, activity_type, metadata)
        VALUES ($1, $2, 'member_joined', '{"role": "member"}'::jsonb)
        "#,
        circle_id,
        address
    )
    .execute(&mut *tx)
    .await;

    tx.commit().await.map_err(|_| "internal_error")?;
    Ok(())
}

/// Leave a circle voluntarily.
pub async fn leave_circle(
    pool: &PgPool,
    circle_id: Uuid,
    address: &str,
) -> Result<(), &'static str> {
    let mut tx = pool.begin().await.map_err(|_| "internal_error")?;

    let role = sqlx::query_scalar!(
        "SELECT role FROM circle_members WHERE circle_id = $1 AND address = $2",
        circle_id,
        address
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| "internal_error")?
    .ok_or("not_member")?;

    // Owners cannot leave without first transferring ownership
    if role == "owner" {
        let _ = tx.rollback().await;
        return Err("owner_cannot_leave_must_transfer");
    }

    // Check pending payout obligations
    let pending: i64 = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM circle_payout_requests WHERE circle_id = $1 AND requester_address = $2 AND status = 'pending'",
        circle_id,
        address
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(|_| "internal_error")?
    .unwrap_or(0);

    if pending > 0 {
        let _ = tx.rollback().await;
        return Err("pending_obligations");
    }

    // Delete membership
    sqlx::query!(
        "DELETE FROM circle_members WHERE circle_id = $1 AND address = $2",
        circle_id,
        address
    )
    .execute(&mut *tx)
    .await
    .map_err(|_| "internal_error")?;

    // Record membership history
    let _ = sqlx::query!(
        r#"
        INSERT INTO circle_membership_history (circle_id, member_address, action, actor_address, old_role)
        VALUES ($1, $2, 'left', $2, $3)
        "#,
        circle_id,
        address,
        role
    )
    .execute(&mut *tx)
    .await;

    // Record activity
    let _ = sqlx::query!(
        r#"
        INSERT INTO circle_activities (circle_id, actor_address, activity_type)
        VALUES ($1, $2, 'member_left')
        "#,
        circle_id,
        address
    )
    .execute(&mut *tx)
    .await;

    tx.commit().await.map_err(|_| "internal_error")?;
    Ok(())
}

/// Remove a member by an authorized administrator/owner.
pub async fn remove_member_authorized(
    pool: &PgPool,
    circle_id: Uuid,
    actor_address: &str,
    target_address: &str,
    reason: Option<String>,
) -> Result<(), &'static str> {
    let mut tx = pool.begin().await.map_err(|_| "internal_error")?;

    // Fetch actor role
    let actor_role_str = sqlx::query_scalar!(
        "SELECT role FROM circle_members WHERE circle_id = $1 AND address = $2",
        circle_id,
        actor_address
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| "internal_error")?
    .ok_or("not_member")?;

    let actor_role = CircleRole::from_str(&actor_role_str).ok_or("internal_error")?;
    if !actor_role.has_permission(CirclePermission::ManageMembers) {
        let _ = tx.rollback().await;
        return Err("permission_denied");
    }

    // Fetch target role
    let target_role_str = sqlx::query_scalar!(
        "SELECT role FROM circle_members WHERE circle_id = $1 AND address = $2",
        circle_id,
        target_address
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| "internal_error")?
    .ok_or("member_not_found")?;

    let target_role = CircleRole::from_str(&target_role_str).ok_or("internal_error")?;

    // Cannot remove Owner
    if target_role == CircleRole::Owner {
        let _ = tx.rollback().await;
        return Err("cannot_remove_owner");
    }

    // Actor must rank strictly higher than target
    if actor_role != CircleRole::Owner
        && target_role.hierarchy_rank() >= actor_role.hierarchy_rank()
    {
        let _ = tx.rollback().await;
        return Err("cannot_remove_equal_or_higher_role");
    }

    // Check pending payout obligations
    let pending: i64 = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM circle_payout_requests WHERE circle_id = $1 AND requester_address = $2 AND status = 'pending'",
        circle_id,
        target_address
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(|_| "internal_error")?
    .unwrap_or(0);

    if pending > 0 {
        let _ = tx.rollback().await;
        return Err("pending_obligations");
    }

    // Delete membership
    sqlx::query!(
        "DELETE FROM circle_members WHERE circle_id = $1 AND address = $2",
        circle_id,
        target_address
    )
    .execute(&mut *tx)
    .await
    .map_err(|_| "internal_error")?;

    // Record membership history
    let _ = sqlx::query!(
        r#"
        INSERT INTO circle_membership_history (circle_id, member_address, action, actor_address, old_role, reason)
        VALUES ($1, $2, 'removed', $3, $4, $5)
        "#,
        circle_id,
        target_address,
        actor_address,
        target_role_str,
        reason
    )
    .execute(&mut *tx)
    .await;

    // Record activity
    let _ = sqlx::query!(
        r#"
        INSERT INTO circle_activities (circle_id, actor_address, activity_type, metadata)
        VALUES ($1, $2, 'member_removed', json_build_object('target', $3::text))
        "#,
        circle_id,
        actor_address,
        target_address
    )
    .execute(&mut *tx)
    .await;

    tx.commit().await.map_err(|_| "internal_error")?;
    Ok(())
}

/// Update member role with privilege escalation prevention.
pub async fn update_member_role(
    pool: &PgPool,
    circle_id: Uuid,
    actor_address: &str,
    target_address: &str,
    new_role: CircleRole,
) -> Result<(), &'static str> {
    let mut tx = pool.begin().await.map_err(|_| "internal_error")?;

    let actor_role_str = sqlx::query_scalar!(
        "SELECT role FROM circle_members WHERE circle_id = $1 AND address = $2",
        circle_id,
        actor_address
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| "internal_error")?
    .ok_or("not_member")?;

    let actor_role = CircleRole::from_str(&actor_role_str).ok_or("internal_error")?;

    let target_role_str = sqlx::query_scalar!(
        "SELECT role FROM circle_members WHERE circle_id = $1 AND address = $2",
        circle_id,
        target_address
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| "internal_error")?
    .ok_or("member_not_found")?;

    let target_role = CircleRole::from_str(&target_role_str).ok_or("internal_error")?;

    // Enforce central RBAC validation
    validate_role_transition(
        actor_address,
        actor_role,
        target_address,
        target_role,
        new_role,
    )?;

    // If transferring ownership: demote existing owner to admin
    if new_role == CircleRole::Owner {
        sqlx::query!(
            "UPDATE circle_members SET role = 'admin' WHERE circle_id = $1 AND address = $2",
            circle_id,
            actor_address
        )
        .execute(&mut *tx)
        .await
        .map_err(|_| "internal_error")?;

        sqlx::query!(
            "UPDATE circles SET admin_address = $1 WHERE id = $2",
            target_address,
            circle_id
        )
        .execute(&mut *tx)
        .await
        .map_err(|_| "internal_error")?;
    }

    // Apply new role to target
    sqlx::query!(
        "UPDATE circle_members SET role = $1 WHERE circle_id = $2 AND address = $3",
        new_role.as_str(),
        circle_id,
        target_address
    )
    .execute(&mut *tx)
    .await
    .map_err(|_| "internal_error")?;

    // Record audit history
    let _ = sqlx::query!(
        r#"
        INSERT INTO circle_membership_history (circle_id, member_address, action, actor_address, old_role, new_role)
        VALUES ($1, $2, 'role_changed', $3, $4, $5)
        "#,
        circle_id,
        target_address,
        actor_address,
        target_role_str,
        new_role.as_str()
    )
    .execute(&mut *tx)
    .await;

    // Record activity
    let _ = sqlx::query!(
        r#"
        INSERT INTO circle_activities (circle_id, actor_address, activity_type, metadata)
        VALUES ($1, $2, 'role_changed', json_build_object('target', $3::text, 'old_role', $4::text, 'new_role', $5::text))
        "#,
        circle_id,
        actor_address,
        target_address,
        target_role_str,
        new_role.as_str()
    )
    .execute(&mut *tx)
    .await;

    tx.commit().await.map_err(|_| "internal_error")?;
    Ok(())
}

/// Retrieve paginated membership history.
pub async fn get_membership_history(
    pool: &PgPool,
    circle_id: Uuid,
    page: i64,
    per_page: i64,
) -> Result<(Vec<MembershipHistoryEntry>, i64), sqlx::Error> {
    let offset = (page.max(1) - 1) * per_page;

    let total: i64 = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM circle_membership_history WHERE circle_id = $1",
        circle_id
    )
    .fetch_one(pool)
    .await?
    .unwrap_or(0);

    let rows = sqlx::query_as!(
        MembershipHistoryRow,
        r#"
        SELECT id, circle_id, member_address, action, actor_address, old_role, new_role, reason, created_at
        FROM circle_membership_history
        WHERE circle_id = $1
        ORDER BY created_at DESC
        LIMIT $2 OFFSET $3
        "#,
        circle_id,
        per_page,
        offset
    )
    .fetch_all(pool)
    .await?;

    let entries = rows
        .into_iter()
        .map(|r| MembershipHistoryEntry {
            id: r.id,
            circle_id: r.circle_id,
            member_address: r.member_address,
            action: r.action,
            actor_address: r.actor_address,
            old_role: r.old_role,
            new_role: r.new_role,
            reason: r.reason,
            created_at: r.created_at,
        })
        .collect();

    Ok((entries, total))
}

#[derive(sqlx::FromRow)]
struct MembershipHistoryRow {
    id: Uuid,
    circle_id: Uuid,
    member_address: String,
    action: String,
    actor_address: String,
    old_role: Option<String>,
    new_role: Option<String>,
    reason: Option<String>,
    created_at: DateTime<Utc>,
}

// ── Unit Tests (#1201) ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_role_hierarchy_ordering() {
        assert!(CircleRole::Owner.hierarchy_rank() > CircleRole::Administrator.hierarchy_rank());
        assert!(
            CircleRole::Administrator.hierarchy_rank() > CircleRole::Moderator.hierarchy_rank()
        );
        assert!(CircleRole::Moderator.hierarchy_rank() > CircleRole::Member.hierarchy_rank());
    }

    #[test]
    fn test_permission_matrix_across_all_roles() {
        // Owner permissions
        assert!(CircleRole::Owner.has_permission(CirclePermission::ManageSettings));
        assert!(CircleRole::Owner.has_permission(CirclePermission::InviteMembers));
        assert!(CircleRole::Owner.has_permission(CirclePermission::ModerateContent));
        assert!(CircleRole::Owner.has_permission(CirclePermission::ViewFinancialInfo));
        assert!(CircleRole::Owner.has_permission(CirclePermission::ManagePayouts));
        assert!(CircleRole::Owner.has_permission(CirclePermission::ManageMembers));
        assert!(CircleRole::Owner.has_permission(CirclePermission::ManageRoles));
        assert!(CircleRole::Owner.has_permission(CirclePermission::Contribute));

        // Administrator permissions
        assert!(CircleRole::Administrator.has_permission(CirclePermission::ManageSettings));
        assert!(CircleRole::Administrator.has_permission(CirclePermission::InviteMembers));
        assert!(CircleRole::Administrator.has_permission(CirclePermission::ModerateContent));
        assert!(CircleRole::Administrator.has_permission(CirclePermission::ViewFinancialInfo));
        assert!(CircleRole::Administrator.has_permission(CirclePermission::ManagePayouts));
        assert!(CircleRole::Administrator.has_permission(CirclePermission::ManageMembers));
        assert!(CircleRole::Administrator.has_permission(CirclePermission::ManageRoles));
        assert!(CircleRole::Administrator.has_permission(CirclePermission::Contribute));

        // Moderator permissions
        assert!(!CircleRole::Moderator.has_permission(CirclePermission::ManageSettings));
        assert!(CircleRole::Moderator.has_permission(CirclePermission::InviteMembers));
        assert!(CircleRole::Moderator.has_permission(CirclePermission::ModerateContent));
        assert!(CircleRole::Moderator.has_permission(CirclePermission::ViewFinancialInfo));
        assert!(!CircleRole::Moderator.has_permission(CirclePermission::ManagePayouts));
        assert!(!CircleRole::Moderator.has_permission(CirclePermission::ManageMembers));
        assert!(!CircleRole::Moderator.has_permission(CirclePermission::ManageRoles));
        assert!(CircleRole::Moderator.has_permission(CirclePermission::Contribute));

        // Member permissions
        assert!(!CircleRole::Member.has_permission(CirclePermission::ManageSettings));
        assert!(!CircleRole::Member.has_permission(CirclePermission::InviteMembers));
        assert!(!CircleRole::Member.has_permission(CirclePermission::ModerateContent));
        assert!(CircleRole::Member.has_permission(CirclePermission::ViewFinancialInfo));
        assert!(!CircleRole::Member.has_permission(CirclePermission::ManagePayouts));
        assert!(!CircleRole::Member.has_permission(CirclePermission::ManageMembers));
        assert!(!CircleRole::Member.has_permission(CirclePermission::ManageRoles));
        assert!(CircleRole::Member.has_permission(CirclePermission::Contribute));
    }

    #[test]
    fn test_prevent_self_role_modification() {
        let res = validate_role_transition(
            "GADMIN",
            CircleRole::Administrator,
            "GADMIN",
            CircleRole::Administrator,
            CircleRole::Owner,
        );
        assert_eq!(res, Err("cannot_modify_own_role"));
    }

    #[test]
    fn test_prevent_non_owner_elevating_to_owner() {
        let res = validate_role_transition(
            "GADMIN",
            CircleRole::Administrator,
            "GMEMBER",
            CircleRole::Member,
            CircleRole::Owner,
        );
        assert_eq!(res, Err("only_owner_can_transfer_ownership"));
    }

    #[test]
    fn test_prevent_admin_granting_admin() {
        let res = validate_role_transition(
            "GADMIN",
            CircleRole::Administrator,
            "GMEMBER",
            CircleRole::Member,
            CircleRole::Administrator,
        );
        assert_eq!(res, Err("cannot_grant_role_equal_or_higher_than_own"));
    }

    #[test]
    fn test_prevent_admin_modifying_owner() {
        let res = validate_role_transition(
            "GADMIN",
            CircleRole::Administrator,
            "GOWNER",
            CircleRole::Owner,
            CircleRole::Member,
        );
        assert_eq!(res, Err("cannot_modify_owner"));
    }

    #[test]
    fn test_prevent_admin_modifying_another_admin() {
        let res = validate_role_transition(
            "GADMIN1",
            CircleRole::Administrator,
            "GADMIN2",
            CircleRole::Administrator,
            CircleRole::Moderator,
        );
        assert_eq!(res, Err("cannot_modify_equal_or_higher_role"));
    }

    #[test]
    fn test_owner_can_appoint_admin_and_transfer_ownership() {
        let res = validate_role_transition(
            "GOWNER",
            CircleRole::Owner,
            "GMEMBER",
            CircleRole::Member,
            CircleRole::Administrator,
        );
        assert!(res.is_ok());

        let transfer = validate_role_transition(
            "GOWNER",
            CircleRole::Owner,
            "GADMIN",
            CircleRole::Administrator,
            CircleRole::Owner,
        );
        assert!(transfer.is_ok());
    }

    #[test]
    fn test_admin_can_appoint_moderator() {
        let res = validate_role_transition(
            "GADMIN",
            CircleRole::Administrator,
            "GMEMBER",
            CircleRole::Member,
            CircleRole::Moderator,
        );
        assert!(res.is_ok());
    }
}

