use axum::{
    extract::Request,
    http::{header::HeaderName, HeaderValue},
    middleware::Next,
    response::Response,
};
use crate::service::MetricsService;

/// Supported API versions
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ApiVersion {
    V1,
    V2,
}

impl ApiVersion {
    pub fn as_str(&self) -> &'static str {
        match self {
            ApiVersion::V1 => "v1",
            ApiVersion::V2 => "v2",
        }
    }

    /// Whether this version is deprecated
    pub fn is_deprecated(&self) -> bool {
        matches!(self, ApiVersion::V1)
    }

    /// Sunset date for deprecated versions (RFC 7231 HTTP-date format)
    pub fn sunset_date(&self) -> Option<&'static str> {
        match self {
            ApiVersion::V1 => Some("Sun, 01 Jan 2027 00:00:00 GMT"),
            ApiVersion::V2 => None,
        }
    }

    /// Migration guide URL for deprecated versions
    pub fn migration_guide_url(&self) -> Option<&'static str> {
        match self {
            ApiVersion::V1 => Some("https://docs.blinks.app/api/migration/v1-to-v2"),
            ApiVersion::V2 => None,
        }
    }

    /// Parse version from path segment (e.g. "v1", "v2")
    pub fn from_path_segment(segment: &str) -> Option<Self> {
        match segment {
            "v1" => Some(ApiVersion::V1),
            "v2" => Some(ApiVersion::V2),
            _ => None,
        }
    }
}

/// Axum middleware that injects API version headers and records version usage analytics.
///
/// Adds the following response headers:
/// - `X-API-Version`: the resolved version (e.g. "v1")
/// - `Deprecation`: RFC 8594 deprecation header (if version is deprecated)
/// - `Sunset`: date after which the version will be removed (if deprecated)
/// - `Link`: link to migration guide (if deprecated)
pub async fn version_middleware(request: Request, next: Next) -> Response {
    // Extract version from the request path (e.g. /api/v1/... → "v1")
    let path = request.uri().path().to_string();
    let version = extract_version_from_path(&path);

    // Record version usage analytics via Prometheus
    MetricsService::record_api_version_usage(version.as_str(), &path);

    let mut response = next.run(request).await;
    let headers = response.headers_mut();

    // Always inject the resolved version
    if let Ok(val) = HeaderValue::from_str(version.as_str()) {
        headers.insert(
            HeaderName::from_static("x-api-version"),
            val,
        );
    }

    // Inject deprecation headers when applicable
    if version.is_deprecated() {
        // RFC 8594 Deprecation header — use a boolean "true" value
        headers.insert(
            HeaderName::from_static("deprecation"),
            HeaderValue::from_static("true"),
        );

        if let Some(sunset) = version.sunset_date() {
            if let Ok(val) = HeaderValue::from_str(sunset) {
                headers.insert(HeaderName::from_static("sunset"), val);
            }
        }

        if let Some(guide_url) = version.migration_guide_url() {
            let link_value = format!("<{}>; rel=\"deprecation\"", guide_url);
            if let Ok(val) = HeaderValue::from_str(&link_value) {
                headers.insert(HeaderName::from_static("link"), val);
            }
        }

        tracing::debug!(
            api_version = version.as_str(),
            path = %path,
            "Deprecated API version used"
        );
    }

    response
}

/// Extract the API version from a URL path.
/// Looks for a path segment matching "v1", "v2", etc.
/// Falls back to V1 if no version segment is found (backward compatibility).
fn extract_version_from_path(path: &str) -> ApiVersion {
    for segment in path.split('/') {
        if let Some(version) = ApiVersion::from_path_segment(segment) {
            return version;
        }
    }
    // Default to V1 for unversioned paths (backward compat)
    ApiVersion::V1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_version_from_path() {
        assert_eq!(extract_version_from_path("/api/v1/payments"), ApiVersion::V1);
        assert_eq!(extract_version_from_path("/api/v2/payments"), ApiVersion::V2);
        assert_eq!(extract_version_from_path("/health"), ApiVersion::V1); // default
    }

    #[test]
    fn test_v1_is_deprecated() {
        assert!(ApiVersion::V1.is_deprecated());
        assert!(!ApiVersion::V2.is_deprecated());
    }

    #[test]
    fn test_sunset_date() {
        assert!(ApiVersion::V1.sunset_date().is_some());
        assert!(ApiVersion::V2.sunset_date().is_none());
    }
}
