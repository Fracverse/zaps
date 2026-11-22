//! #1202 — Circle settings service.
//!
//! Validation and change detection live in the pure [`apply_update`] function;
//! [`update_settings`] loads the circle under a row lock, applies it, persists
//! the result and records a `settings_updated` activity for important changes.

use crate::services::circle_membership::{self, CirclePermission};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize};
use sqlx::postgres::PgRow;
use sqlx::{PgPool, Row};
use uuid::Uuid;

pub const MAX_NAME_LEN: usize = 80;
pub const MAX_DESCRIPTION_LEN: usize = 1_000;
pub const MAX_CATEGORY_LEN: usize = 32;
pub const MAX_IMAGE_URL_LEN: usize = 2_048;
pub const MIN_MEMBER_LIMIT: i32 = 2;
pub const MAX_MEMBER_LIMIT: i32 = 10_000;
pub const VALID_VISIBILITIES: &[&str] = &["public", "private", "invite_only"];

/// Fields whose change is recorded as a circle activity visible to members.
const IMPORTANT_FIELDS: &[&str] = &[
    "name",
    "visibility",
    "is_discoverable",
    "member_limit",
    "allow_join_requests",
];

/// Distinguish "field absent" (`None`) from "field explicitly null" (`Some(None)`).
fn double_option<'de, T, D>(de: D) -> Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    Option::<T>::deserialize(de).map(Some)
}

/// PATCH body. Absent fields are left unchanged; nullable fields accept `null`
/// to clear them.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateCircleSettingsRequest {
    pub name: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    pub description: Option<Option<String>>,
    pub category: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    pub image_url: Option<Option<String>>,
    pub visibility: Option<String>,
    pub is_discoverable: Option<bool>,
    #[serde(default, deserialize_with = "double_option")]
    pub member_limit: Option<Option<i32>>,
    pub allow_join_requests: Option<bool>,
    pub invitation_expiry_hours: Option<i32>,
    pub max_active_goals: Option<i32>,
}

impl UpdateCircleSettingsRequest {
    fn is_empty(&self) -> bool {
        self.name.is_none()
            && self.description.is_none()
            && self.category.is_none()
            && self.image_url.is_none()
            && self.visibility.is_none()
            && self.is_discoverable.is_none()
            && self.member_limit.is_none()
            && self.allow_join_requests.is_none()
            && self.invitation_expiry_hours.is_none()
            && self.max_active_goals.is_none()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CircleSettings {
    pub circle_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub category: String,
    pub image_url: Option<String>,
    pub visibility: String,
    pub is_discoverable: bool,
    pub member_limit: Option<i32>,
    pub allow_join_requests: bool,
    pub invitation_expiry_hours: i32,
    pub max_active_goals: i32,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SettingsChange {
    pub field: &'static str,
    pub old: serde_json::Value,
    pub new: serde_json::Value,
}

impl SettingsChange {
    pub fn is_important(&self) -> bool {
        IMPORTANT_FIELDS.contains(&self.field)
    }
}

// ── Validation ────────────────────────────────────────────────────────────────

fn clean_optional_text(raw: Option<String>) -> Option<String> {
    raw.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

pub fn validate_name(raw: &str) -> Result<String, &'static str> {
    let name = raw.trim();
    if name.is_empty() {
        return Err("name_required");
    }
    if name.chars().count() > MAX_NAME_LEN {
        return Err("name_too_long");
    }
    Ok(name.to_string())
}

pub fn validate_category(raw: &str) -> Result<String, &'static str> {
    let category = raw.trim().to_lowercase();
    let well_formed = !category.is_empty()
        && category.len() <= MAX_CATEGORY_LEN
        && category
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if well_formed {
        Ok(category)
    } else {
        Err("invalid_category")
    }
}

/// Images must be served over HTTPS from object storage / a CDN.
pub fn validate_image_url(raw: &str) -> Result<String, &'static str> {
    let url = raw.trim();
    let host = url.strip_prefix("https://").unwrap_or("");
    let host = host.split(['/', '?', '#']).next().unwrap_or("");
    if host.is_empty()
        || url.len() > MAX_IMAGE_URL_LEN
        || url.chars().any(|c| c.is_whitespace() || c.is_control())
        || host.contains('@')
    {
        return Err("invalid_image_url");
    }
    Ok(url.to_string())
}

/// Validate `req` against `current` and return the new settings plus the list
/// of fields that actually changed. `member_count` guards `member_limit`.
pub fn apply_update(
    current: &CircleSettings,
    req: UpdateCircleSettingsRequest,
    member_count: i64,
) -> Result<(CircleSettings, Vec<SettingsChange>), &'static str> {
    if req.is_empty() {
        return Err("empty_update");
    }

    let mut next = current.clone();

    if let Some(name) = req.name.as_deref() {
        next.name = validate_name(name)?;
    }
    if let Some(description) = req.description {
        let description = clean_optional_text(description);
        if description
            .as_deref()
            .map(|d| d.chars().count() > MAX_DESCRIPTION_LEN)
            .unwrap_or(false)
        {
            return Err("description_too_long");
        }
        next.description = description;
    }
    if let Some(category) = req.category.as_deref() {
        next.category = validate_category(category)?;
    }
    if let Some(image_url) = req.image_url {
        next.image_url = clean_optional_text(image_url)
            .as_deref()
            .map(validate_image_url)
            .transpose()?;
    }
    if let Some(visibility) = req.visibility.as_deref() {
        let visibility = visibility.trim().to_lowercase();
        if !VALID_VISIBILITIES.contains(&visibility.as_str()) {
            return Err("invalid_visibility");
        }
        // Mirror create-circle: switching to private hides the circle unless
        // the request says otherwise (which is rejected below).
        if visibility == "private" && req.is_discoverable.is_none() {
            next.is_discoverable = false;
        }
        next.visibility = visibility;
    }
    if let Some(discoverable) = req.is_discoverable {
        next.is_discoverable = discoverable;
    }
    if next.visibility == "private" && next.is_discoverable {
        return Err("private_circle_cannot_be_discoverable");
    }
    if let Some(limit) = req.member_limit {
        if let Some(l) = limit {
            if !(MIN_MEMBER_LIMIT..=MAX_MEMBER_LIMIT).contains(&l) {
                return Err("invalid_member_limit");
            }
            if (l as i64) < member_count {
                return Err("member_limit_below_member_count");
            }
        }
        next.member_limit = limit;
    }
    if let Some(allow) = req.allow_join_requests {
        next.allow_join_requests = allow;
    }
    if let Some(hours) = req.invitation_expiry_hours {
        if !(1..=720).contains(&hours) {
            return Err("invalid_invitation_expiry");
        }
        next.invitation_expiry_hours = hours;
    }
    if let Some(max) = req.max_active_goals {
        if !(1..=50).contains(&max) {
            return Err("invalid_max_active_goals");
        }
        next.max_active_goals = max;
    }

    let changes = diff(current, &next);
    Ok((next, changes))
}

fn diff(old: &CircleSettings, new: &CircleSettings) -> Vec<SettingsChange> {
    use serde_json::json;
    let mut changes = Vec::new();
    macro_rules! track {
        ($field:ident) => {
            if old.$field != new.$field {
                changes.push(SettingsChange {
                    field: stringify!($field),
                    old: json!(old.$field),
                    new: json!(new.$field),
                });
            }
        };
    }
    track!(name);
    track!(description);
    track!(category);
    track!(image_url);
    track!(visibility);
    track!(is_discoverable);
    track!(member_limit);
    track!(allow_join_requests);
    track!(invitation_expiry_hours);
    track!(max_active_goals);
    changes
}

// ── Persistence ───────────────────────────────────────────────────────────────

const SETTINGS_COLUMNS: &str = "id, name, description, category, image_url, visibility, \
     is_discoverable, member_limit, allow_join_requests, invitation_expiry_hours, \
     max_active_goals, updated_at";

fn row_to_settings(row: &PgRow) -> CircleSettings {
    CircleSettings {
        circle_id: row.get("id"),
        name: row.get("name"),
        description: row.get("description"),
        category: row.get("category"),
        image_url: row.get("image_url"),
        visibility: row.get("visibility"),
        is_discoverable: row.get("is_discoverable"),
        member_limit: row.get("member_limit"),
        allow_join_requests: row.get("allow_join_requests"),
        invitation_expiry_hours: row.get("invitation_expiry_hours"),
        max_active_goals: row.get("max_active_goals"),
        updated_at: row.get("updated_at"),
    }
}

/// Read a circle's settings. Any member may view them.
pub async fn get_settings(
    pool: &PgPool,
    circle_id: Uuid,
    viewer: &str,
) -> Result<CircleSettings, &'static str> {
    let row = sqlx::query(&format!("SELECT {SETTINGS_COLUMNS} FROM circles WHERE id = $1"))
        .bind(circle_id)
        .fetch_optional(pool)
        .await
        .map_err(|_| "internal_error")?
        .ok_or("circle_not_found")?;

    circle_membership::require_circle_permission(
        pool,
        circle_id,
        viewer,
        CirclePermission::ViewFinancialInfo,
    )
    .await?;

    Ok(row_to_settings(&row))
}

/// Update a circle's settings. Requires `ManageSettings` (owner / admin).
///
/// Returns the persisted settings and the applied changes; an update that
/// changes nothing is a successful no-op and writes nothing.
pub async fn update_settings(
    pool: &PgPool,
    circle_id: Uuid,
    actor: &str,
    req: UpdateCircleSettingsRequest,
) -> Result<(CircleSettings, Vec<SettingsChange>), &'static str> {
    circle_membership::require_circle_permission(
        pool,
        circle_id,
        actor,
        CirclePermission::ManageSettings,
    )
    .await?;

    let mut tx = pool.begin().await.map_err(|_| "internal_error")?;

    let row = sqlx::query(&format!(
        "SELECT {SETTINGS_COLUMNS} FROM circles WHERE id = $1 FOR UPDATE"
    ))
    .bind(circle_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| "internal_error")?
    .ok_or("circle_not_found")?;
    let current = row_to_settings(&row);

    let member_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM circle_members WHERE circle_id = $1")
            .bind(circle_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(|_| "internal_error")?;

    let (next, changes) = apply_update(&current, req, member_count)?;
    if changes.is_empty() {
        return Ok((current, changes));
    }

    let row = sqlx::query(&format!(
        "UPDATE circles SET
             name = $2, description = $3, category = $4, image_url = $5,
             visibility = $6, is_discoverable = $7, member_limit = $8,
             allow_join_requests = $9, invitation_expiry_hours = $10,
             max_active_goals = $11, updated_at = NOW()
         WHERE id = $1
         RETURNING {SETTINGS_COLUMNS}"
    ))
    .bind(circle_id)
    .bind(&next.name)
    .bind(&next.description)
    .bind(&next.category)
    .bind(&next.image_url)
    .bind(&next.visibility)
    .bind(next.is_discoverable)
    .bind(next.member_limit)
    .bind(next.allow_join_requests)
    .bind(next.invitation_expiry_hours)
    .bind(next.max_active_goals)
    .fetch_one(&mut *tx)
    .await
    .map_err(|_| "internal_error")?;
    let saved = row_to_settings(&row);

    let important: Vec<&SettingsChange> = changes.iter().filter(|c| c.is_important()).collect();
    if !important.is_empty() {
        sqlx::query(
            r#"
            INSERT INTO circle_activities (circle_id, actor_address, activity_type, metadata)
            VALUES ($1, $2, 'settings_updated', jsonb_build_object('changes', $3::jsonb))
            "#,
        )
        .bind(circle_id)
        .bind(actor)
        .bind(serde_json::json!(important))
        .execute(&mut *tx)
        .await
        .map_err(|_| "internal_error")?;
    }

    tx.commit().await.map_err(|_| "internal_error")?;
    Ok((saved, changes))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> CircleSettings {
        CircleSettings {
            circle_id: Uuid::nil(),
            name: "Lagos Ajo".into(),
            description: Some("weekly".into()),
            category: "savings".into(),
            image_url: None,
            visibility: "public".into(),
            is_discoverable: true,
            member_limit: Some(10),
            allow_join_requests: true,
            invitation_expiry_hours: 168,
            max_active_goals: 5,
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn empty_update_is_rejected() {
        let res = apply_update(&settings(), UpdateCircleSettingsRequest::default(), 1);
        assert_eq!(res.unwrap_err(), "empty_update");
    }

    #[test]
    fn name_is_trimmed_and_bounded() {
        let req = UpdateCircleSettingsRequest {
            name: Some("  New Name  ".into()),
            ..Default::default()
        };
        let (next, changes) = apply_update(&settings(), req, 1).unwrap();
        assert_eq!(next.name, "New Name");
        assert_eq!(changes.len(), 1);
        assert!(changes[0].is_important());

        let blank = UpdateCircleSettingsRequest {
            name: Some("   ".into()),
            ..Default::default()
        };
        assert_eq!(apply_update(&settings(), blank, 1).unwrap_err(), "name_required");

        let long = UpdateCircleSettingsRequest {
            name: Some("x".repeat(MAX_NAME_LEN + 1)),
            ..Default::default()
        };
        assert_eq!(apply_update(&settings(), long, 1).unwrap_err(), "name_too_long");
    }

    #[test]
    fn nullable_fields_can_be_cleared() {
        let req: UpdateCircleSettingsRequest =
            serde_json::from_str(r#"{"description": null, "member_limit": null}"#).unwrap();
        let (next, changes) = apply_update(&settings(), req, 3).unwrap();
        assert_eq!(next.description, None);
        assert_eq!(next.member_limit, None);
        assert_eq!(changes.len(), 2);
    }

    #[test]
    fn unknown_fields_are_rejected() {
        let res = serde_json::from_str::<UpdateCircleSettingsRequest>(r#"{"admin": "GXYZ"}"#);
        assert!(res.is_err());
    }

    #[test]
    fn visibility_rules() {
        let bad = UpdateCircleSettingsRequest {
            visibility: Some("secret".into()),
            ..Default::default()
        };
        assert_eq!(apply_update(&settings(), bad, 1).unwrap_err(), "invalid_visibility");

        let private = UpdateCircleSettingsRequest {
            visibility: Some("Private".into()),
            ..Default::default()
        };
        let (next, _) = apply_update(&settings(), private, 1).unwrap();
        assert_eq!(next.visibility, "private");
        assert!(!next.is_discoverable);

        let contradictory = UpdateCircleSettingsRequest {
            visibility: Some("private".into()),
            is_discoverable: Some(true),
            ..Default::default()
        };
        assert_eq!(
            apply_update(&settings(), contradictory, 1).unwrap_err(),
            "private_circle_cannot_be_discoverable"
        );
    }

    #[test]
    fn member_limit_cannot_drop_below_current_members() {
        let req = UpdateCircleSettingsRequest {
            member_limit: Some(Some(4)),
            ..Default::default()
        };
        assert_eq!(
            apply_update(&settings(), req, 5).unwrap_err(),
            "member_limit_below_member_count"
        );
        let too_small = UpdateCircleSettingsRequest {
            member_limit: Some(Some(1)),
            ..Default::default()
        };
        assert_eq!(
            apply_update(&settings(), too_small, 1).unwrap_err(),
            "invalid_member_limit"
        );
    }

    #[test]
    fn image_url_must_be_https() {
        assert!(validate_image_url("https://cdn.zaps.app/a.png").is_ok());
        assert_eq!(validate_image_url("http://cdn.zaps.app/a.png"), Err("invalid_image_url"));
        assert_eq!(validate_image_url("https:///a.png"), Err("invalid_image_url"));
        assert_eq!(validate_image_url("https://user@evil.test/"), Err("invalid_image_url"));
        assert_eq!(validate_image_url("https://a.b/x y"), Err("invalid_image_url"));
    }

    #[test]
    fn category_is_normalized() {
        assert_eq!(validate_category(" Savings "), Ok("savings".to_string()));
        assert_eq!(validate_category("high-yield_2"), Ok("high-yield_2".to_string()));
        assert_eq!(validate_category("bad category"), Err("invalid_category"));
        assert_eq!(validate_category(""), Err("invalid_category"));
    }

    #[test]
    fn unchanged_values_produce_no_changes() {
        let req = UpdateCircleSettingsRequest {
            name: Some("Lagos Ajo".into()),
            allow_join_requests: Some(true),
            ..Default::default()
        };
        let (next, changes) = apply_update(&settings(), req, 1).unwrap();
        assert_eq!(next, settings_with_ts(next.updated_at));
        assert!(changes.is_empty());
    }

    fn settings_with_ts(ts: DateTime<Utc>) -> CircleSettings {
        CircleSettings {
            updated_at: ts,
            ..settings()
        }
    }

    #[test]
    fn configurable_limits_are_bounded() {
        let req = UpdateCircleSettingsRequest {
            invitation_expiry_hours: Some(0),
            ..Default::default()
        };
        assert_eq!(
            apply_update(&settings(), req, 1).unwrap_err(),
            "invalid_invitation_expiry"
        );
        let req = UpdateCircleSettingsRequest {
            max_active_goals: Some(51),
            ..Default::default()
        };
        assert_eq!(
            apply_update(&settings(), req, 1).unwrap_err(),
            "invalid_max_active_goals"
        );
        let req = UpdateCircleSettingsRequest {
            invitation_expiry_hours: Some(48),
            max_active_goals: Some(3),
            ..Default::default()
        };
        let (_, changes) = apply_update(&settings(), req, 1).unwrap();
        assert_eq!(changes.len(), 2);
        assert!(changes.iter().all(|c| !c.is_important()));
    }
}
