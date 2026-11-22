//! #1199 / #1200 — Circle invitation service.
//!
//! Two kinds of invitation are supported:
//!
//! * **direct** — addressed to a single Zaps user. Only that user may accept or
//!   decline it, and at most one *pending* direct invitation exists per
//!   `(circle, invitee)` pair (re-inviting returns the active one).
//! * **link** — a shareable, high-entropy token any authenticated user can
//!   redeem, optionally capped by `max_uses`. Only the SHA-256 hash of the
//!   token is persisted; the raw token is returned to the creator once.
//!
//! Every invitation has an expiry (defaulting to the circle's
//! `invitation_expiry_hours` setting). Status transitions are decided by the
//! pure [`decide`] function so the rules are unit-testable without a database;
//! the async functions only load state under row locks and persist the result.

use crate::services::circle_membership::{self, CirclePermission};
use crate::services::circle_notifications as circle_notifier;
use crate::services::circle_sharing;
use chrono::{DateTime, Duration, Utc};
use serde::Serialize;
use sqlx::postgres::PgRow;
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

pub const MIN_EXPIRY_HOURS: i64 = 1;
pub const MAX_EXPIRY_HOURS: i64 = 720;
pub const MAX_LINK_USES: i32 = 1_000;
const MAX_ADDRESS_LEN: usize = 128;

// ── Types ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InvitationStatus {
    Pending,
    Accepted,
    Declined,
    Expired,
    Revoked,
}

impl InvitationStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Accepted => "accepted",
            Self::Declined => "declined",
            Self::Expired => "expired",
            Self::Revoked => "revoked",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "pending" => Some(Self::Pending),
            "accepted" => Some(Self::Accepted),
            "declined" => Some(Self::Declined),
            "expired" => Some(Self::Expired),
            "revoked" => Some(Self::Revoked),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InviteType {
    Direct,
    Link,
}

impl InviteType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Direct => "direct",
            Self::Link => "link",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "direct" => Some(Self::Direct),
            "link" => Some(Self::Link),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct InvitationEntry {
    pub id: Uuid,
    pub circle_id: Uuid,
    pub invite_type: InviteType,
    pub inviter_address: String,
    pub invitee_address: Option<String>,
    /// Effective status: a pending invitation past its expiry reports `expired`.
    pub status: InvitationStatus,
    pub max_uses: Option<i32>,
    pub use_count: i32,
    pub expires_at: Option<DateTime<Utc>>,
    pub responded_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

/// Input for [`create_invitation`]. `invitee = None` creates a link invitation.
#[derive(Debug, Clone, Default)]
pub struct NewInvitation {
    pub invitee: Option<String>,
    pub expires_in_hours: Option<i64>,
    pub max_uses: Option<i32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CreatedInvitation {
    pub invitation: InvitationEntry,
    /// Raw link token. Present only for newly created link invitations.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    /// True when an active direct invitation already existed and was returned.
    pub reused_existing: bool,
}

/// How the caller identifies the invitation they are responding to.
#[derive(Debug, Clone, Copy)]
pub enum InvitationTarget<'a> {
    Id(Uuid),
    Token(&'a str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseAction {
    Accept,
    Decline,
}

/// Outcome of a state-transition decision for an invitation response.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    AcceptNew,
    AlreadyAccepted,
    Decline,
    AlreadyDeclined,
}

#[derive(Debug, Clone, Serialize)]
pub struct ResponseOutcome {
    pub invitation_id: Uuid,
    pub circle_id: Uuid,
    pub status: InvitationStatus,
    /// True when this request repeated an earlier identical response.
    pub already_processed: bool,
    /// True when this request created a `circle_members` row.
    pub membership_created: bool,
}

/// The parts of an invitation row that [`decide`] needs.
#[derive(Debug, Clone)]
pub struct InvitationSnapshot<'a> {
    pub invite_type: InviteType,
    pub invitee: Option<&'a str>,
    pub status: InvitationStatus,
    pub expires_at: Option<DateTime<Utc>>,
    pub max_uses: Option<i32>,
    pub use_count: i32,
    pub responded_by: Option<&'a str>,
    /// Link invitations: whether the caller already redeemed this token.
    pub caller_redeemed: bool,
}

// ── Pure rules ────────────────────────────────────────────────────────────────

/// Validate a user-supplied Zaps address / identifier.
pub fn normalize_address(raw: &str) -> Result<String, &'static str> {
    let addr = raw.trim();
    if addr.is_empty() {
        return Err("invitee_required");
    }
    if addr.len() > MAX_ADDRESS_LEN || addr.chars().any(char::is_whitespace) {
        return Err("invalid_invitee");
    }
    Ok(addr.to_string())
}

/// Resolve the expiry window, falling back to the circle's configured default.
pub fn resolve_expiry_hours(
    requested: Option<i64>,
    circle_default: i32,
) -> Result<i64, &'static str> {
    let hours = requested.unwrap_or(circle_default as i64);
    if (MIN_EXPIRY_HOURS..=MAX_EXPIRY_HOURS).contains(&hours) {
        Ok(hours)
    } else {
        Err("invalid_expiry")
    }
}

/// Validate `max_uses` for the given invitation type.
pub fn validate_max_uses(
    invite_type: InviteType,
    max_uses: Option<i32>,
) -> Result<Option<i32>, &'static str> {
    match (invite_type, max_uses) {
        (InviteType::Direct, Some(_)) => Err("max_uses_not_allowed"),
        (InviteType::Direct, None) => Ok(None),
        (InviteType::Link, None) => Ok(None),
        (InviteType::Link, Some(n)) if (1..=MAX_LINK_USES).contains(&n) => Ok(Some(n)),
        (InviteType::Link, Some(_)) => Err("invalid_max_uses"),
    }
}

/// Status as observed now: pending invitations past their expiry are expired.
pub fn effective_status(
    stored: InvitationStatus,
    expires_at: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> InvitationStatus {
    match (stored, expires_at) {
        (InvitationStatus::Pending, Some(exp)) if exp <= now => InvitationStatus::Expired,
        (status, _) => status,
    }
}

/// Decide what an accept/decline request does to an invitation.
///
/// Repeating a response that already succeeded is idempotent
/// (`AlreadyAccepted` / `AlreadyDeclined`) instead of an error.
pub fn decide(
    inv: &InvitationSnapshot<'_>,
    caller: &str,
    action: ResponseAction,
    now: DateTime<Utc>,
) -> Result<Decision, &'static str> {
    if inv.invite_type == InviteType::Direct && inv.invitee != Some(caller) {
        return Err("not_invitation_recipient");
    }

    let status = effective_status(inv.status, inv.expires_at, now);

    match (inv.invite_type, action) {
        (InviteType::Direct, ResponseAction::Accept) => match status {
            InvitationStatus::Pending => Ok(Decision::AcceptNew),
            InvitationStatus::Accepted if inv.responded_by == Some(caller) => {
                Ok(Decision::AlreadyAccepted)
            }
            InvitationStatus::Accepted => Err("invitation_already_accepted"),
            InvitationStatus::Declined => Err("invitation_already_declined"),
            InvitationStatus::Expired => Err("invitation_expired"),
            InvitationStatus::Revoked => Err("invitation_revoked"),
        },
        (InviteType::Direct, ResponseAction::Decline) => match status {
            InvitationStatus::Pending => Ok(Decision::Decline),
            InvitationStatus::Declined => Ok(Decision::AlreadyDeclined),
            InvitationStatus::Accepted => Err("invitation_already_accepted"),
            InvitationStatus::Expired => Err("invitation_expired"),
            InvitationStatus::Revoked => Err("invitation_revoked"),
        },
        (InviteType::Link, ResponseAction::Accept) => {
            if status == InvitationStatus::Revoked {
                return Err("invitation_revoked");
            }
            if inv.caller_redeemed {
                return Ok(Decision::AlreadyAccepted);
            }
            match status {
                InvitationStatus::Expired => Err("invitation_expired"),
                InvitationStatus::Pending
                    if inv.max_uses.map(|m| inv.use_count < m).unwrap_or(true) =>
                {
                    Ok(Decision::AcceptNew)
                }
                _ => Err("invitation_exhausted"),
            }
        }
        // A shared link has no single recipient whose answer could be recorded.
        (InviteType::Link, ResponseAction::Decline) => Err("link_invitation_not_declinable"),
    }
}

// ── Persistence helpers ───────────────────────────────────────────────────────

const INVITATION_COLUMNS: &str = "id, circle_id, invite_type, inviter_address, invitee_address, \
     status, max_uses, use_count, expires_at, responded_by, responded_at, created_at";

fn row_to_entry(row: &PgRow, now: DateTime<Utc>) -> InvitationEntry {
    let stored = InvitationStatus::parse(row.get::<String, _>("status").as_str())
        .unwrap_or(InvitationStatus::Pending);
    let expires_at: Option<DateTime<Utc>> = row.get("expires_at");
    InvitationEntry {
        id: row.get("id"),
        circle_id: row.get("circle_id"),
        invite_type: InviteType::parse(row.get::<String, _>("invite_type").as_str())
            .unwrap_or(InviteType::Direct),
        inviter_address: row.get("inviter_address"),
        invitee_address: row.get("invitee_address"),
        status: effective_status(stored, expires_at, now),
        max_uses: row.get("max_uses"),
        use_count: row.get("use_count"),
        expires_at,
        responded_at: row.get("responded_at"),
        created_at: row.get("created_at"),
    }
}

async fn is_member(
    tx: &mut Transaction<'_, Postgres>,
    circle_id: Uuid,
    address: &str,
) -> Result<bool, &'static str> {
    sqlx::query("SELECT 1 FROM circle_members WHERE circle_id = $1 AND address = $2")
        .bind(circle_id)
        .bind(address)
        .fetch_optional(&mut **tx)
        .await
        .map(|r| r.is_some())
        .map_err(|_| "internal_error")
}

async fn member_count(
    tx: &mut Transaction<'_, Postgres>,
    circle_id: Uuid,
) -> Result<i64, &'static str> {
    sqlx::query_scalar("SELECT COUNT(*) FROM circle_members WHERE circle_id = $1")
        .bind(circle_id)
        .fetch_one(&mut **tx)
        .await
        .map_err(|_| "internal_error")
}

// ── Service API ───────────────────────────────────────────────────────────────

/// Create a direct or link invitation. The inviter needs `InviteMembers`.
pub async fn create_invitation(
    pool: &PgPool,
    circle_id: Uuid,
    inviter: &str,
    input: NewInvitation,
) -> Result<CreatedInvitation, &'static str> {
    circle_membership::require_circle_permission(
        pool,
        circle_id,
        inviter,
        CirclePermission::InviteMembers,
    )
    .await?;

    let invite_type = if input.invitee.is_some() {
        InviteType::Direct
    } else {
        InviteType::Link
    };
    let invitee = input
        .invitee
        .as_deref()
        .map(normalize_address)
        .transpose()?;
    let max_uses = validate_max_uses(invite_type, input.max_uses)?;
    if invitee.as_deref() == Some(inviter) {
        return Err("cannot_invite_self");
    }

    let mut tx = pool.begin().await.map_err(|_| "internal_error")?;

    // Lock the circle so concurrent invites observe a consistent member count.
    let circle = sqlx::query(
        "SELECT invitation_expiry_hours, member_limit FROM circles WHERE id = $1 FOR UPDATE",
    )
    .bind(circle_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| "internal_error")?
    .ok_or("circle_not_found")?;

    let hours = resolve_expiry_hours(
        input.expires_in_hours,
        circle.get("invitation_expiry_hours"),
    )?;
    let member_limit: Option<i32> = circle.get("member_limit");
    if let Some(limit) = member_limit {
        if member_count(&mut tx, circle_id).await? >= limit as i64 {
            return Err("member_limit_reached");
        }
    }

    let now = Utc::now();

    if let Some(addr) = invitee.as_deref() {
        if is_member(&mut tx, circle_id, addr).await? {
            return Err("already_member");
        }

        let existing = sqlx::query(&format!(
            "SELECT {INVITATION_COLUMNS} FROM circle_invitations
             WHERE circle_id = $1 AND invitee_address = $2
               AND invite_type = 'direct' AND status = 'pending'
             FOR UPDATE"
        ))
        .bind(circle_id)
        .bind(addr)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| "internal_error")?;

        if let Some(row) = existing {
            let entry = row_to_entry(&row, now);
            if entry.status == InvitationStatus::Pending {
                tx.commit().await.map_err(|_| "internal_error")?;
                return Ok(CreatedInvitation {
                    invitation: entry,
                    token: None,
                    reused_existing: true,
                });
            }
            // Stale pending row: retire it so the partial unique index frees up.
            sqlx::query(
                "UPDATE circle_invitations SET status = 'expired', updated_at = NOW() WHERE id = $1",
            )
            .bind(entry.id)
            .execute(&mut *tx)
            .await
            .map_err(|_| "internal_error")?;
        }
    }

    let token = (invite_type == InviteType::Link).then(circle_sharing::generate_token);
    let token_hash = token.as_deref().map(circle_sharing::hash_token);
    let expires_at = now + Duration::hours(hours);

    let row = sqlx::query(&format!(
        "INSERT INTO circle_invitations
             (id, circle_id, inviter_address, invitee_address, invite_type,
              token_hash, max_uses, expires_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
         RETURNING {INVITATION_COLUMNS}"
    ))
    .bind(Uuid::new_v4())
    .bind(circle_id)
    .bind(inviter)
    .bind(invitee.as_deref())
    .bind(invite_type.as_str())
    .bind(token_hash.as_deref())
    .bind(max_uses)
    .bind(expires_at)
    .fetch_one(&mut *tx)
    .await
    .map_err(|_| "internal_error")?;
    let entry = row_to_entry(&row, now);

    sqlx::query(
        r#"
        INSERT INTO circle_activities (circle_id, actor_address, activity_type, metadata)
        VALUES ($1, $2, 'invitation_created',
                jsonb_build_object('invitation_id', $3::uuid, 'invite_type', $4::text))
        "#,
    )
    .bind(circle_id)
    .bind(inviter)
    .bind(entry.id)
    .bind(invite_type.as_str())
    .execute(&mut *tx)
    .await
    .map_err(|_| "internal_error")?;

    tx.commit().await.map_err(|_| "internal_error")?;

    if let Some(addr) = entry.invitee_address.as_deref() {
        let dedupe_key = format!("circle_invitation:{}", entry.id);
        if let Err(e) = circle_notifier::notify(
            pool,
            &circle_notifier::NewCircleNotification {
                circle_id: Some(circle_id),
                recipient_address: addr,
                kind: circle_notifier::CircleNotificationType::Invitation,
                title: "You've been invited to a circle",
                body: "Open Zaps to accept or decline the invitation.",
                metadata: serde_json::json!({
                    "invitation_id": entry.id,
                    "inviter": inviter,
                    "expires_at": entry.expires_at,
                }),
                dedupe_key: Some(&dedupe_key),
                push_token: None,
            },
        )
        .await
        {
            tracing::warn!(invitation_id = %entry.id, error = %e, "Failed to notify invitee");
        }
    }

    Ok(CreatedInvitation {
        invitation: entry,
        token,
        reused_existing: false,
    })
}

/// List a circle's invitations (newest first). Requires `InviteMembers`.
pub async fn list_circle_invitations(
    pool: &PgPool,
    circle_id: Uuid,
    viewer: &str,
    status: Option<InvitationStatus>,
    page: i64,
    per_page: i64,
) -> Result<(Vec<InvitationEntry>, i64), &'static str> {
    circle_membership::require_circle_permission(
        pool,
        circle_id,
        viewer,
        CirclePermission::InviteMembers,
    )
    .await?;

    // The stored status of a lapsed invitation is still `pending`, so filter on
    // the *effective* status the caller will see.
    let filter = r#"
        circle_id = $1 AND (
            $2::text IS NULL
            OR ($2 = 'expired' AND (status = 'expired'
                OR (status = 'pending' AND expires_at <= NOW())))
            OR ($2 = 'pending' AND status = 'pending'
                AND (expires_at IS NULL OR expires_at > NOW()))
            OR ($2 NOT IN ('pending', 'expired') AND status = $2)
        )
    "#;
    let status = status.map(InvitationStatus::as_str);

    let total: i64 = sqlx::query_scalar(&format!(
        "SELECT COUNT(*) FROM circle_invitations WHERE {filter}"
    ))
    .bind(circle_id)
    .bind(status)
    .fetch_one(pool)
    .await
    .map_err(|_| "internal_error")?;

    let rows = sqlx::query(&format!(
        "SELECT {INVITATION_COLUMNS} FROM circle_invitations WHERE {filter}
         ORDER BY created_at DESC LIMIT $3 OFFSET $4"
    ))
    .bind(circle_id)
    .bind(status)
    .bind(per_page)
    .bind((page - 1) * per_page)
    .fetch_all(pool)
    .await
    .map_err(|_| "internal_error")?;

    let now = Utc::now();
    Ok((rows.iter().map(|r| row_to_entry(r, now)).collect(), total))
}

/// Pending, unexpired direct invitations addressed to `address`.
pub async fn list_pending_for(
    pool: &PgPool,
    address: &str,
) -> Result<Vec<InvitationEntry>, &'static str> {
    let rows = sqlx::query(&format!(
        "SELECT {INVITATION_COLUMNS} FROM circle_invitations
         WHERE invitee_address = $1 AND invite_type = 'direct' AND status = 'pending'
           AND (expires_at IS NULL OR expires_at > NOW())
         ORDER BY created_at DESC"
    ))
    .bind(address)
    .fetch_all(pool)
    .await
    .map_err(|_| "internal_error")?;

    let now = Utc::now();
    Ok(rows.iter().map(|r| row_to_entry(r, now)).collect())
}

/// Revoke a pending invitation. Allowed for the inviter or a `ManageMembers` role.
pub async fn revoke_invitation(
    pool: &PgPool,
    circle_id: Uuid,
    invitation_id: Uuid,
    actor: &str,
) -> Result<(), &'static str> {
    let row = sqlx::query(
        "SELECT inviter_address, status FROM circle_invitations WHERE id = $1 AND circle_id = $2",
    )
    .bind(invitation_id)
    .bind(circle_id)
    .fetch_optional(pool)
    .await
    .map_err(|_| "internal_error")?
    .ok_or("invitation_not_found")?;

    if row.get::<String, _>("inviter_address") != actor {
        circle_membership::require_circle_permission(
            pool,
            circle_id,
            actor,
            CirclePermission::ManageMembers,
        )
        .await?;
    }

    let updated = sqlx::query(
        "UPDATE circle_invitations SET status = 'revoked', updated_at = NOW()
         WHERE id = $1 AND status = 'pending'",
    )
    .bind(invitation_id)
    .execute(pool)
    .await
    .map_err(|_| "internal_error")?;

    if updated.rows_affected() == 0 {
        return Err("invitation_not_pending");
    }
    Ok(())
}

/// Mark every lapsed pending invitation as `expired`. Returns rows updated.
pub async fn expire_stale_invitations(pool: &PgPool) -> Result<u64, sqlx::Error> {
    let res = sqlx::query(
        "UPDATE circle_invitations SET status = 'expired', updated_at = NOW()
         WHERE status = 'pending' AND expires_at IS NOT NULL AND expires_at <= NOW()",
    )
    .execute(pool)
    .await?;
    Ok(res.rows_affected())
}

/// #1200 — Accept or decline an invitation.
///
/// The invitation row (and, on acceptance, the circle row) is locked for the
/// duration of the transaction, so concurrent duplicate requests serialise and
/// the second one observes the first one's result.
pub async fn respond(
    pool: &PgPool,
    target: InvitationTarget<'_>,
    caller: &str,
    action: ResponseAction,
) -> Result<ResponseOutcome, &'static str> {
    let mut tx = pool.begin().await.map_err(|_| "internal_error")?;

    let row = match target {
        InvitationTarget::Id(id) => {
            let sql = format!(
                "SELECT {INVITATION_COLUMNS} FROM circle_invitations WHERE id = $1 FOR UPDATE"
            );
            sqlx::query(&sql).bind(id).fetch_optional(&mut *tx).await
        }
        InvitationTarget::Token(token) => {
            let token = token.trim();
            if token.len() != 64 || !token.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err("invitation_not_found");
            }
            let sql = format!(
                "SELECT {INVITATION_COLUMNS} FROM circle_invitations
                 WHERE token_hash = $1 AND invite_type = 'link' FOR UPDATE"
            );
            sqlx::query(&sql)
                .bind(circle_sharing::hash_token(token))
                .fetch_optional(&mut *tx)
                .await
        }
    }
    .map_err(|_| "internal_error")?
    .ok_or("invitation_not_found")?;

    let invitation_id: Uuid = row.get("id");
    let circle_id: Uuid = row.get("circle_id");
    let invite_type =
        InviteType::parse(row.get::<String, _>("invite_type").as_str()).ok_or("internal_error")?;
    let stored_status =
        InvitationStatus::parse(row.get::<String, _>("status").as_str()).ok_or("internal_error")?;
    let invitee: Option<String> = row.get("invitee_address");
    let responded_by: Option<String> = row.get("responded_by");

    let caller_redeemed = invite_type == InviteType::Link
        && sqlx::query(
            "SELECT 1 FROM circle_invitation_redemptions WHERE invitation_id = $1 AND address = $2",
        )
        .bind(invitation_id)
        .bind(caller)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| "internal_error")?
        .is_some();

    let snapshot = InvitationSnapshot {
        invite_type,
        invitee: invitee.as_deref(),
        status: stored_status,
        expires_at: row.get("expires_at"),
        max_uses: row.get("max_uses"),
        use_count: row.get("use_count"),
        responded_by: responded_by.as_deref(),
        caller_redeemed,
    };

    let decision = match decide(&snapshot, caller, action, Utc::now()) {
        Ok(d) => d,
        Err("invitation_expired") if stored_status == InvitationStatus::Pending => {
            // Persist the lapse so listings and eligibility checks agree.
            let _ = sqlx::query(
                "UPDATE circle_invitations SET status = 'expired', updated_at = NOW() WHERE id = $1",
            )
            .bind(invitation_id)
            .execute(&mut *tx)
            .await;
            let _ = tx.commit().await;
            return Err("invitation_expired");
        }
        Err(e) => return Err(e),
    };

    let outcome = |status, already_processed, membership_created| ResponseOutcome {
        invitation_id,
        circle_id,
        status,
        already_processed,
        membership_created,
    };

    match decision {
        Decision::AlreadyAccepted => Ok(outcome(InvitationStatus::Accepted, true, false)),
        Decision::AlreadyDeclined => Ok(outcome(InvitationStatus::Declined, true, false)),
        Decision::Decline => {
            sqlx::query(
                "UPDATE circle_invitations
                 SET status = 'declined', responded_by = $2, responded_at = NOW(), updated_at = NOW()
                 WHERE id = $1",
            )
            .bind(invitation_id)
            .bind(caller)
            .execute(&mut *tx)
            .await
            .map_err(|_| "internal_error")?;

            sqlx::query(
                r#"
                INSERT INTO circle_activities (circle_id, actor_address, activity_type, metadata)
                VALUES ($1, $2, 'invitation_declined', jsonb_build_object('invitation_id', $3::uuid))
                "#,
            )
            .bind(circle_id)
            .bind(caller)
            .bind(invitation_id)
            .execute(&mut *tx)
            .await
            .map_err(|_| "internal_error")?;

            tx.commit().await.map_err(|_| "internal_error")?;
            Ok(outcome(InvitationStatus::Declined, false, false))
        }
        Decision::AcceptNew => {
            let membership_created = accept_in_tx(
                &mut tx,
                circle_id,
                invitation_id,
                invite_type,
                caller,
                &snapshot,
            )
            .await?;
            tx.commit().await.map_err(|_| "internal_error")?;
            Ok(outcome(
                InvitationStatus::Accepted,
                false,
                membership_created,
            ))
        }
    }
}

async fn accept_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    circle_id: Uuid,
    invitation_id: Uuid,
    invite_type: InviteType,
    caller: &str,
    snapshot: &InvitationSnapshot<'_>,
) -> Result<bool, &'static str> {
    let circle = sqlx::query("SELECT member_limit FROM circles WHERE id = $1 FOR UPDATE")
        .bind(circle_id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(|_| "internal_error")?
        .ok_or("circle_not_found")?;

    let already_member = is_member(tx, circle_id, caller).await?;

    if !already_member {
        if let Some(limit) = circle.get::<Option<i32>, _>("member_limit") {
            if member_count(tx, circle_id).await? >= limit as i64 {
                return Err("member_limit_reached");
            }
        }

        sqlx::query(
            "INSERT INTO circle_members (circle_id, address, role) VALUES ($1, $2, 'member')",
        )
        .bind(circle_id)
        .bind(caller)
        .execute(&mut **tx)
        .await
        .map_err(|_| "internal_error")?;

        sqlx::query(
            r#"
            INSERT INTO circle_membership_history
                (circle_id, member_address, action, actor_address, new_role, reason)
            VALUES ($1, $2, 'joined', $2, 'member', 'invitation')
            "#,
        )
        .bind(circle_id)
        .bind(caller)
        .execute(&mut **tx)
        .await
        .map_err(|_| "internal_error")?;

        sqlx::query(
            r#"
            INSERT INTO circle_activities (circle_id, actor_address, activity_type, metadata)
            VALUES ($1, $2, 'member_joined',
                    jsonb_build_object('role', 'member', 'via', 'invitation',
                                       'invitation_id', $3::uuid))
            "#,
        )
        .bind(circle_id)
        .bind(caller)
        .bind(invitation_id)
        .execute(&mut **tx)
        .await
        .map_err(|_| "internal_error")?;
    }

    match invite_type {
        InviteType::Direct => {
            sqlx::query(
                "UPDATE circle_invitations
                 SET status = 'accepted', responded_by = $2, responded_at = NOW(), updated_at = NOW()
                 WHERE id = $1",
            )
            .bind(invitation_id)
            .bind(caller)
            .execute(&mut **tx)
            .await
            .map_err(|_| "internal_error")?;
        }
        // An existing member redeeming a link does not consume one of its uses.
        InviteType::Link if !already_member => {
            sqlx::query(
                "INSERT INTO circle_invitation_redemptions (invitation_id, address) VALUES ($1, $2)",
            )
            .bind(invitation_id)
            .bind(caller)
            .execute(&mut **tx)
            .await
            .map_err(|_| "internal_error")?;

            let exhausted = snapshot
                .max_uses
                .map(|m| snapshot.use_count + 1 >= m)
                .unwrap_or(false);
            sqlx::query(
                "UPDATE circle_invitations
                 SET use_count = use_count + 1,
                     status = CASE WHEN $2 THEN 'accepted' ELSE status END,
                     responded_at = NOW(), updated_at = NOW()
                 WHERE id = $1",
            )
            .bind(invitation_id)
            .bind(exhausted)
            .execute(&mut **tx)
            .await
            .map_err(|_| "internal_error")?;
        }
        InviteType::Link => {}
    }

    Ok(!already_member)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn direct<'a>(invitee: &'a str, status: InvitationStatus) -> InvitationSnapshot<'a> {
        InvitationSnapshot {
            invite_type: InviteType::Direct,
            invitee: Some(invitee),
            status,
            expires_at: Some(Utc::now() + Duration::hours(1)),
            max_uses: None,
            use_count: 0,
            responded_by: None,
            caller_redeemed: false,
        }
    }

    fn link<'a>(max_uses: Option<i32>, use_count: i32) -> InvitationSnapshot<'a> {
        InvitationSnapshot {
            invite_type: InviteType::Link,
            invitee: None,
            status: InvitationStatus::Pending,
            expires_at: Some(Utc::now() + Duration::hours(1)),
            max_uses,
            use_count,
            responded_by: None,
            caller_redeemed: false,
        }
    }

    #[test]
    fn status_round_trips() {
        for s in [
            InvitationStatus::Pending,
            InvitationStatus::Accepted,
            InvitationStatus::Declined,
            InvitationStatus::Expired,
            InvitationStatus::Revoked,
        ] {
            assert_eq!(InvitationStatus::parse(s.as_str()), Some(s));
        }
        assert_eq!(InvitationStatus::parse("bogus"), None);
    }

    #[test]
    fn expiry_hours_default_and_bounds() {
        assert_eq!(resolve_expiry_hours(None, 168), Ok(168));
        assert_eq!(resolve_expiry_hours(Some(1), 168), Ok(1));
        assert_eq!(resolve_expiry_hours(Some(720), 168), Ok(720));
        assert_eq!(resolve_expiry_hours(Some(0), 168), Err("invalid_expiry"));
        assert_eq!(resolve_expiry_hours(Some(721), 168), Err("invalid_expiry"));
        assert_eq!(resolve_expiry_hours(Some(-5), 168), Err("invalid_expiry"));
    }

    #[test]
    fn max_uses_rules() {
        assert_eq!(validate_max_uses(InviteType::Direct, None), Ok(None));
        assert_eq!(
            validate_max_uses(InviteType::Direct, Some(3)),
            Err("max_uses_not_allowed")
        );
        assert_eq!(validate_max_uses(InviteType::Link, Some(10)), Ok(Some(10)));
        assert_eq!(
            validate_max_uses(InviteType::Link, Some(0)),
            Err("invalid_max_uses")
        );
        assert_eq!(
            validate_max_uses(InviteType::Link, Some(MAX_LINK_USES + 1)),
            Err("invalid_max_uses")
        );
    }

    #[test]
    fn address_normalization() {
        assert_eq!(normalize_address("  GABC  "), Ok("GABC".to_string()));
        assert_eq!(normalize_address("   "), Err("invitee_required"));
        assert_eq!(normalize_address("GA BC"), Err("invalid_invitee"));
        assert_eq!(normalize_address(&"G".repeat(200)), Err("invalid_invitee"));
    }

    #[test]
    fn pending_past_expiry_is_expired() {
        let now = Utc::now();
        let past = Some(now - Duration::seconds(1));
        assert_eq!(
            effective_status(InvitationStatus::Pending, past, now),
            InvitationStatus::Expired
        );
        assert_eq!(
            effective_status(InvitationStatus::Accepted, past, now),
            InvitationStatus::Accepted
        );
        assert_eq!(
            effective_status(InvitationStatus::Pending, None, now),
            InvitationStatus::Pending
        );
    }

    #[test]
    fn only_intended_recipient_can_respond_to_direct_invite() {
        let inv = direct("alice", InvitationStatus::Pending);
        let now = Utc::now();
        assert_eq!(
            decide(&inv, "mallory", ResponseAction::Accept, now),
            Err("not_invitation_recipient")
        );
        assert_eq!(
            decide(&inv, "mallory", ResponseAction::Decline, now),
            Err("not_invitation_recipient")
        );
        assert_eq!(
            decide(&inv, "alice", ResponseAction::Accept, now),
            Ok(Decision::AcceptNew)
        );
    }

    #[test]
    fn expired_invitation_is_rejected() {
        let mut inv = direct("alice", InvitationStatus::Pending);
        inv.expires_at = Some(Utc::now() - Duration::minutes(1));
        let now = Utc::now();
        assert_eq!(
            decide(&inv, "alice", ResponseAction::Accept, now),
            Err("invitation_expired")
        );
        assert_eq!(
            decide(&inv, "alice", ResponseAction::Decline, now),
            Err("invitation_expired")
        );

        let mut lnk = link(None, 0);
        lnk.expires_at = Some(Utc::now() - Duration::minutes(1));
        assert_eq!(
            decide(&lnk, "bob", ResponseAction::Accept, now),
            Err("invitation_expired")
        );
    }

    #[test]
    fn duplicate_responses_are_idempotent() {
        let now = Utc::now();
        let mut accepted = direct("alice", InvitationStatus::Accepted);
        accepted.responded_by = Some("alice");
        assert_eq!(
            decide(&accepted, "alice", ResponseAction::Accept, now),
            Ok(Decision::AlreadyAccepted)
        );
        assert_eq!(
            decide(&accepted, "alice", ResponseAction::Decline, now),
            Err("invitation_already_accepted")
        );

        let declined = direct("alice", InvitationStatus::Declined);
        assert_eq!(
            decide(&declined, "alice", ResponseAction::Decline, now),
            Ok(Decision::AlreadyDeclined)
        );
        assert_eq!(
            decide(&declined, "alice", ResponseAction::Accept, now),
            Err("invitation_already_declined")
        );

        let mut redeemed = link(Some(1), 1);
        redeemed.status = InvitationStatus::Accepted;
        redeemed.caller_redeemed = true;
        assert_eq!(
            decide(&redeemed, "bob", ResponseAction::Accept, now),
            Ok(Decision::AlreadyAccepted)
        );
    }

    #[test]
    fn revoked_invitations_cannot_be_used() {
        let now = Utc::now();
        let inv = direct("alice", InvitationStatus::Revoked);
        assert_eq!(
            decide(&inv, "alice", ResponseAction::Accept, now),
            Err("invitation_revoked")
        );
        let mut lnk = link(None, 0);
        lnk.status = InvitationStatus::Revoked;
        lnk.caller_redeemed = true;
        assert_eq!(
            decide(&lnk, "bob", ResponseAction::Accept, now),
            Err("invitation_revoked")
        );
    }

    #[test]
    fn link_invitation_use_cap() {
        let now = Utc::now();
        assert_eq!(
            decide(&link(Some(2), 1), "bob", ResponseAction::Accept, now),
            Ok(Decision::AcceptNew)
        );
        assert_eq!(
            decide(&link(Some(2), 2), "bob", ResponseAction::Accept, now),
            Err("invitation_exhausted")
        );
        assert_eq!(
            decide(&link(None, 500), "bob", ResponseAction::Accept, now),
            Ok(Decision::AcceptNew)
        );
        assert_eq!(
            decide(&link(None, 0), "bob", ResponseAction::Decline, now),
            Err("link_invitation_not_declinable")
        );
    }
}
