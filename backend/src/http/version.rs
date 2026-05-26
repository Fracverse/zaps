/// API Version documentation and migration guide endpoints.
use axum::{extract::Path, Json};
use serde::Serialize;

use crate::api_error::ApiError;

#[derive(Debug, Serialize)]
pub struct VersionInfo {
    pub version: String,
    pub status: VersionStatus,
    pub released: &'static str,
    pub sunset: Option<&'static str>,
    pub migration_guide: Option<&'static str>,
    pub changelog_url: &'static str,
    pub features: Vec<&'static str>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum VersionStatus {
    Current,
    Deprecated,
    Sunset,
}

#[derive(Debug, Serialize)]
pub struct VersionListResponse {
    pub versions: Vec<VersionInfo>,
    pub current_version: &'static str,
    pub latest_version: &'static str,
}

#[derive(Debug, Serialize)]
pub struct MigrationGuide {
    pub from_version: String,
    pub to_version: String,
    pub breaking_changes: Vec<BreakingChange>,
    pub new_features: Vec<&'static str>,
    pub migration_steps: Vec<MigrationStep>,
    pub documentation_url: &'static str,
}

#[derive(Debug, Serialize)]
pub struct BreakingChange {
    pub endpoint: &'static str,
    pub description: &'static str,
    pub before: &'static str,
    pub after: &'static str,
}

#[derive(Debug, Serialize)]
pub struct MigrationStep {
    pub step: u8,
    pub title: &'static str,
    pub description: &'static str,
}

fn v1_info() -> VersionInfo {
    VersionInfo {
        version: "v1".to_string(),
        status: VersionStatus::Deprecated,
        released: "2026-01-20",
        sunset: Some("2027-01-01"),
        migration_guide: Some("/api/versions/v1/migration"),
        changelog_url: "https://docs.blinks.app/api/changelog/v1",
        features: vec![
            "payments",
            "transfers",
            "withdrawals",
            "identity",
            "notifications",
            "profiles",
            "audit-logs",
        ],
    }
}

fn v2_info() -> VersionInfo {
    VersionInfo {
        version: "v2".to_string(),
        status: VersionStatus::Current,
        released: "2026-05-01",
        sunset: None,
        migration_guide: None,
        changelog_url: "https://docs.blinks.app/api/changelog/v2",
        features: vec![
            "payments",
            "payment-disputes",
            "transfers",
            "withdrawals",
            "identity",
            "notifications",
            "profiles",
            "audit-logs",
            "enhanced-caching",
            "version-analytics",
        ],
    }
}

/// GET /api/versions
/// Returns a list of all supported API versions with their status.
pub async fn list_versions() -> Json<VersionListResponse> {
    Json(VersionListResponse {
        versions: vec![v1_info(), v2_info()],
        current_version: "v1",
        latest_version: "v2",
    })
}

/// GET /api/versions/:version
/// Returns detailed information about a specific API version.
pub async fn get_version(Path(version): Path<String>) -> Result<Json<VersionInfo>, ApiError> {
    let info = match version.as_str() {
        "v1" => v1_info(),
        "v2" => v2_info(),
        _ => {
            return Err(ApiError::NotFound(format!(
                "API version '{}' not found. Supported versions: v1, v2",
                version
            )))
        }
    };
    Ok(Json(info))
}

/// GET /api/versions/:version/migration
/// Returns the migration guide from the given version to the next version.
pub async fn get_migration_guide(
    Path(version): Path<String>,
) -> Result<Json<MigrationGuide>, ApiError> {
    match version.as_str() {
        "v1" => Ok(Json(MigrationGuide {
            from_version: "v1".to_string(),
            to_version: "v2".to_string(),
            breaking_changes: vec![
                BreakingChange {
                    endpoint: "POST /api/v1/payments",
                    description: "Response now includes dispute_eligible field",
                    before: r#"{"id":"...","status":"completed"}"#,
                    after: r#"{"id":"...","status":"completed","dispute_eligible":true}"#,
                },
                BreakingChange {
                    endpoint: "GET /api/v1/payments/:id",
                    description: "Response includes active_dispute field when a dispute exists",
                    before: r#"{"id":"...","status":"completed"}"#,
                    after: r#"{"id":"...","status":"completed","active_dispute":null}"#,
                },
            ],
            new_features: vec![
                "Payment dispute management (POST /api/v2/payments/:id/disputes)",
                "Cache hit rate analytics via /metrics endpoint",
                "API version usage analytics via /metrics endpoint",
                "Deprecation headers on all v1 responses",
            ],
            migration_steps: vec![
                MigrationStep {
                    step: 1,
                    title: "Update base URL",
                    description: "Replace /api/v1/ with /api/v2/ in all API calls.",
                },
                MigrationStep {
                    step: 2,
                    title: "Handle new response fields",
                    description: "Update response parsing to handle new optional fields: dispute_eligible, active_dispute.",
                },
                MigrationStep {
                    step: 3,
                    title: "Implement dispute handling",
                    description: "Integrate the new dispute management endpoints for payment dispute workflows.",
                },
                MigrationStep {
                    step: 4,
                    title: "Update error handling",
                    description: "The error response shape is unchanged. No action required.",
                },
            ],
            documentation_url: "https://docs.blinks.app/api/migration/v1-to-v2",
        })),
        "v2" => Err(ApiError::BadRequest(
            "v2 is the latest version. No migration guide available.".to_string(),
        )),
        _ => Err(ApiError::NotFound(format!(
            "API version '{}' not found",
            version
        ))),
    }
}
