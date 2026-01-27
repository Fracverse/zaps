use axum::{
    extract::{Request, State},
    http::Method,
    middleware::Next,
    response::Response,
};
use std::sync::Arc;

use crate::service::ServiceContainer;

/// Audit logging middleware that automatically logs all authenticated requests
pub async fn audit_logging(
    State(services): State<Arc<ServiceContainer>>,
    request: Request,
    next: Next,
) -> Response {
    // Extract actor_id from request extensions (set by auth middleware)
    let actor_id = request
        .extensions()
        .get::<String>()
        .cloned()
        .unwrap_or_else(|| "anonymous".to_string());

    // Extract IP address from headers
    let ip_address = request
        .headers()
        .get("x-forwarded-for")
        .and_then(|h| h.to_str().ok())
        .or_else(|| {
            request
                .headers()
                .get("x-real-ip")
                .and_then(|h| h.to_str().ok())
        })
        .map(|s| s.to_string());

    // Extract user agent
    let user_agent = request
        .headers()
        .get("user-agent")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string());

    // Parse request path and method to determine action and resource
    let method = request.method().clone();
    let path = request.uri().path().to_string();
    let (action, resource, resource_id) = parse_request_info(&method, &path);

    // Execute the request
    let response = next.run(request).await;

    // Only log successful requests (2xx status codes)
    let status = response.status();
    if status.is_success() {
        // Clone services for async task
        let audit_service = services.audit.clone();

        // Log asynchronously to avoid blocking the response
        tokio::spawn(async move {
            let _ = audit_service
                .create_audit_log(
                    actor_id,
                    action,
                    resource,
                    resource_id,
                    None, // metadata can be extended later
                    ip_address,
                    user_agent,
                )
                .await;
        });
    }

    response
}

/// Parse HTTP method and path to extract action, resource, and resource_id
fn parse_request_info(method: &Method, path: &str) -> (String, String, Option<String>) {
    let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

    if parts.is_empty() {
        return (method.to_string(), "unknown".to_string(), None);
    }

    // Determine resource from path
    let resource = parts.first().unwrap_or(&"unknown").to_string();

    // Try to extract resource ID (typically the second segment)
    let resource_id = if parts.len() > 1 {
        // Check if it looks like a UUID or ID (not a subresource like "status")
        let potential_id = parts[1];
        if !is_subresource(potential_id) {
            Some(potential_id.to_string())
        } else {
            None
        }
    } else {
        None
    };

    // Determine action based on HTTP method
    let action = match method {
        &Method::GET if resource_id.is_some() => format!("view_{}", resource),
        &Method::GET => format!("list_{}", resource),
        &Method::POST => format!("create_{}", resource),
        &Method::PUT | &Method::PATCH => format!("update_{}", resource),
        &Method::DELETE => format!("delete_{}", resource),
        _ => method.to_string(),
    };

    (action, resource, resource_id)
}

/// Check if a path segment is a subresource rather than an ID
fn is_subresource(segment: &str) -> bool {
    matches!(
        segment,
        "status" | "wallet" | "activity" | "stats" | "health" | "qr" | "nfc"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Method;

    #[test]
    fn test_parse_request_info() {
        // Test GET with ID
        let (action, resource, resource_id) = parse_request_info(&Method::GET, "/payments/123-456");
        assert_eq!(action, "view_payments");
        assert_eq!(resource, "payments");
        assert_eq!(resource_id, Some("123-456".to_string()));

        // Test GET list
        let (action, resource, resource_id) = parse_request_info(&Method::GET, "/payments");
        assert_eq!(action, "list_payments");
        assert_eq!(resource, "payments");
        assert_eq!(resource_id, None);

        // Test POST create
        let (action, resource, resource_id) = parse_request_info(&Method::POST, "/transfers");
        assert_eq!(action, "create_transfers");
        assert_eq!(resource, "transfers");
        assert_eq!(resource_id, None);

        // Test subresource (not treated as ID)
        let (action, resource, resource_id) =
            parse_request_info(&Method::GET, "/payments/123/status");
        assert_eq!(action, "view_payments");
        assert_eq!(resource, "payments");
        assert_eq!(resource_id, Some("123".to_string()));
    }
}
