use axum::{
    body::Body,
    extract::{Request, State},
    http::Method,
    middleware::Next,
    response::Response,
};
use http_body_util::BodyExt;
use std::sync::Arc;

use crate::middleware::auth::AuthenticatedUser;
use crate::service::ServiceContainer;

const MAX_BODY_SNIPPET_LEN: usize = 2048;

pub async fn audit_logging(
    State(services): State<Arc<ServiceContainer>>,
    request: Request,
    next: Next,
) -> Response {
    let method = request.method().clone();

    if !matches!(
        method,
        Method::POST | Method::PUT | Method::PATCH | Method::DELETE
    ) {
        return next.run(request).await;
    }

    let actor_id = request
        .extensions()
        .get::<AuthenticatedUser>()
        .map(|u| u.user_id.clone())
        .unwrap_or_else(|| "anonymous".to_string());

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

    let user_agent = request
        .headers()
        .get("user-agent")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string());

    let path = request.uri().path().to_string();
    let (action, resource, resource_id) = parse_request_info(&method, &path);

    let is_multipart = request
        .headers()
        .get("content-type")
        .and_then(|ct| ct.to_str().ok())
        .map(|ct| ct.starts_with("multipart/"))
        .unwrap_or(false);

    let (request, request_snippet) = if is_multipart {
        (request, None)
    } else {
        let (parts, body) = request.into_parts();
        let body_bytes = body
            .collect()
            .await
            .map(|c| c.to_bytes())
            .unwrap_or_default();
        let snippet = body_snippet(&body_bytes);
        (Request::from_parts(parts, Body::from(body_bytes)), snippet)
    };

    let response = next.run(request).await;

    let status_code = response.status().as_u16();

    let (res_parts, res_body) = response.into_parts();
    let res_bytes = res_body
        .collect()
        .await
        .map(|c| c.to_bytes())
        .unwrap_or_default();
    let response_snippet = body_snippet(&res_bytes);
    let response = Response::from_parts(res_parts, Body::from(res_bytes));

    let metadata = serde_json::json!({
        "request_body": request_snippet,
        "response_body": response_snippet,
        "status_code": status_code,
    });

    let audit_service = services.audit.clone();
    tokio::spawn(async move {
        if let Err(e) = audit_service
            .create_audit_log(crate::models::CreateAuditLogParams {
                actor_id,
                action,
                resource,
                resource_id,
                metadata: Some(metadata),
                ip_address,
                user_agent,
            })
            .await
        {
            tracing::error!("failed to write audit log: {}", e);
        }
    });

    response
}

fn body_snippet(bytes: &[u8]) -> Option<serde_json::Value> {
    if bytes.is_empty() {
        return None;
    }

    let end = bytes.len().min(MAX_BODY_SNIPPET_LEN);
    let text = String::from_utf8_lossy(&bytes[..end]);

    match serde_json::from_str::<serde_json::Value>(&text) {
        Ok(value) => Some(redact_sensitive_fields(value)),
        Err(_) => Some(serde_json::Value::String(text.into_owned())),
    }
}

fn redact_sensitive_fields(mut value: serde_json::Value) -> serde_json::Value {
    if let Some(obj) = value.as_object_mut() {
        for key in [
            "password",
            "pin",
            "secret",
            "token",
            "authorization",
            "cookie",
        ] {
            if obj.contains_key(key) {
                obj.insert(
                    key.to_string(),
                    serde_json::Value::String("[REDACTED]".to_string()),
                );
            }
        }
    }
    value
}

fn parse_request_info(method: &Method, path: &str) -> (String, String, Option<String>) {
    let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

    if parts.is_empty() {
        return (method.to_string(), "unknown".to_string(), None);
    }

    let resource = parts.first().unwrap_or(&"unknown").to_string();

    let resource_id = if parts.len() > 1 {
        let potential_id = parts[1];
        if !is_subresource(potential_id) {
            Some(potential_id.to_string())
        } else {
            None
        }
    } else {
        None
    };

    let action = match *method {
        Method::POST => format!("create_{}", resource),
        Method::PUT | Method::PATCH => format!("update_{}", resource),
        Method::DELETE => format!("delete_{}", resource),
        _ => method.to_string(),
    };

    (action, resource, resource_id)
}

fn is_subresource(segment: &str) -> bool {
    matches!(
        segment,
        "status" | "wallet" | "activity" | "stats" | "health" | "qr" | "nfc" | "read"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Method;

    #[test]
    fn test_parse_post() {
        let (action, resource, id) = parse_request_info(&Method::POST, "/payments");
        assert_eq!(action, "create_payments");
        assert_eq!(resource, "payments");
        assert_eq!(id, None);
    }

    #[test]
    fn test_parse_put_with_id() {
        let (action, resource, id) = parse_request_info(&Method::PUT, "/payments/123-456");
        assert_eq!(action, "update_payments");
        assert_eq!(resource, "payments");
        assert_eq!(id, Some("123-456".to_string()));
    }

    #[test]
    fn test_parse_patch() {
        let (action, resource, id) = parse_request_info(&Method::PATCH, "/profiles/user1");
        assert_eq!(action, "update_profiles");
        assert_eq!(resource, "profiles");
        assert_eq!(id, Some("user1".to_string()));
    }

    #[test]
    fn test_parse_delete() {
        let (action, resource, id) = parse_request_info(&Method::DELETE, "/files/abc");
        assert_eq!(action, "delete_files");
        assert_eq!(resource, "files");
        assert_eq!(id, Some("abc".to_string()));
    }

    #[test]
    fn test_parse_subresource() {
        let (action, resource, id) = parse_request_info(&Method::PATCH, "/notifications/123/read");
        assert_eq!(action, "update_notifications");
        assert_eq!(resource, "notifications");
        assert_eq!(id, Some("123".to_string()));
    }

    #[test]
    fn test_parse_empty_path() {
        let (action, resource, id) = parse_request_info(&Method::POST, "/");
        assert_eq!(action, "POST");
        assert_eq!(resource, "unknown");
        assert_eq!(id, None);
    }

    #[test]
    fn test_redact_sensitive_fields() {
        let input = serde_json::json!({
            "username": "alice",
            "password": "s3cret",
            "token": "abc123"
        });
        let redacted = redact_sensitive_fields(input);
        assert_eq!(redacted["username"], "alice");
        assert_eq!(redacted["password"], "[REDACTED]");
        assert_eq!(redacted["token"], "[REDACTED]");
    }

    #[test]
    fn test_body_snippet_empty() {
        assert_eq!(body_snippet(&[]), None);
    }

    #[test]
    fn test_body_snippet_json() {
        let data = serde_json::json!({"amount": 100, "asset": "XLM"});
        let bytes = serde_json::to_vec(&data).unwrap();
        let snippet = body_snippet(&bytes).unwrap();
        assert_eq!(snippet["amount"], 100);
        assert_eq!(snippet["asset"], "XLM");
    }

    #[test]
    fn test_body_snippet_with_sensitive_data() {
        let data = serde_json::json!({"username": "bob", "password": "hunter2"});
        let bytes = serde_json::to_vec(&data).unwrap();
        let snippet = body_snippet(&bytes).unwrap();
        assert_eq!(snippet["username"], "bob");
        assert_eq!(snippet["password"], "[REDACTED]");
    }

    #[test]
    fn test_body_snippet_plain_text() {
        let text = b"not valid json";
        let snippet = body_snippet(text).unwrap();
        assert_eq!(snippet, "not valid json");
    }
}
