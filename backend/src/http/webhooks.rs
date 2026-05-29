use axum::{
    extract::{Path, State},
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::{
    api_error::ApiError,
    service::{ServiceContainer, webhook_service::RegisterWebhookRequest, WebhookDelivery, WebhookEndpoint},
};

#[derive(Debug, Serialize)]
pub struct WebhookEndpointResponse {
    pub id: String,
    pub merchant_id: String,
    pub url: String,
    pub events: Vec<String>,
    pub is_active: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize)]
pub struct WebhookDeliveryResponse {
    pub id: String,
    pub endpoint_id: String,
    pub event_type: String,
    pub payload: serde_json::Value,
    pub status: String,
    pub attempt_count: i32,
    pub next_retry_at: Option<chrono::DateTime<chrono::Utc>>,
    pub response_status: Option<i32>,
    pub error_message: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize)]
pub struct WebhookDashboardStats {
    pub total_endpoints: i64,
    pub active_endpoints: i64,
    pub total_deliveries: i64,
    pub successful_deliveries: i64,
    pub failed_deliveries: i64,
    pub pending_deliveries: i64,
}

// Register a new webhook endpoint
pub async fn register_webhook(
    State(services): State<Arc<ServiceContainer>>,
    Json(request): Json<RegisterWebhookRequest>,
) -> Result<Json<WebhookEndpointResponse>, ApiError> {
    let endpoint = services.webhook.register_endpoint(request).await?;

    Ok(Json(WebhookEndpointResponse {
        id: endpoint.id,
        merchant_id: endpoint.merchant_id,
        url: endpoint.url,
        events: endpoint.events,
        is_active: endpoint.is_active,
        created_at: endpoint.created_at,
    }))
}

// List all webhook endpoints for a merchant
pub async fn list_webhooks(
    State(services): State<Arc<ServiceContainer>>,
    Path(merchant_id): Path<String>,
) -> Result<Json<Vec<WebhookEndpointResponse>>, ApiError> {
    let endpoints = services.webhook.list_endpoints(&merchant_id).await?;

    let response = endpoints
        .into_iter()
        .map(|e| WebhookEndpointResponse {
            id: e.id,
            merchant_id: e.merchant_id,
            url: e.url,
            events: e.events,
            is_active: e.is_active,
            created_at: e.created_at,
        })
        .collect();

    Ok(Json(response))
}

// Delete a webhook endpoint
pub async fn delete_webhook(
    State(services): State<Arc<ServiceContainer>>,
    Path((merchant_id, endpoint_id)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let endpoint_uuid = Uuid::parse_str(&endpoint_id)
        .map_err(|_| ApiError::Validation("Invalid endpoint ID".to_string()))?;

    services
        .webhook
        .delete_endpoint(endpoint_uuid, &merchant_id)
        .await?;

    Ok(Json(serde_json::json!({
        "message": "Webhook endpoint deleted successfully"
    })))
}

// Get delivery history for a webhook endpoint
pub async fn get_webhook_deliveries(
    State(services): State<Arc<ServiceContainer>>,
    Path(endpoint_id): Path<String>,
) -> Result<Json<Vec<WebhookDeliveryResponse>>, ApiError> {
    let endpoint_uuid = Uuid::parse_str(&endpoint_id)
        .map_err(|_| ApiError::Validation("Invalid endpoint ID".to_string()))?;

    let deliveries = services.webhook.get_deliveries(endpoint_uuid).await?;

    let response = deliveries
        .into_iter()
        .map(|d| WebhookDeliveryResponse {
            id: d.id,
            endpoint_id: d.endpoint_id,
            event_type: d.event_type,
            payload: d.payload,
            status: d.status,
            attempt_count: d.attempt_count,
            next_retry_at: d.next_retry_at,
            response_status: d.response_status,
            error_message: d.error_message,
            created_at: d.created_at,
        })
        .collect();

    Ok(Json(response))
}

// Get webhook dashboard statistics
pub async fn get_webhook_dashboard(
    State(services): State<Arc<ServiceContainer>>,
    Path(merchant_id): Path<String>,
) -> Result<Json<WebhookDashboardStats>, ApiError> {
    let client = services.db_pool.get().await?;

    // Get total endpoints
    let total_endpoints: i64 = client
        .query_one(
            "SELECT COUNT(*) FROM webhook_endpoints WHERE merchant_id = $1",
            &[&merchant_id],
        )
        .await?
        .get(0);

    // Get active endpoints
    let active_endpoints: i64 = client
        .query_one(
            "SELECT COUNT(*) FROM webhook_endpoints WHERE merchant_id = $1 AND is_active = true",
            &[&merchant_id],
        )
        .await?
        .get(0);

    // Get total deliveries for this merchant's endpoints
    let total_deliveries: i64 = client
        .query_one(
            r#"
            SELECT COUNT(*) 
            FROM webhook_deliveries d
            JOIN webhook_endpoints e ON e.id = d.endpoint_id
            WHERE e.merchant_id = $1
            "#,
            &[&merchant_id],
        )
        .await?
        .get(0);

    // Get successful deliveries
    let successful_deliveries: i64 = client
        .query_one(
            r#"
            SELECT COUNT(*) 
            FROM webhook_deliveries d
            JOIN webhook_endpoints e ON e.id = d.endpoint_id
            WHERE e.merchant_id = $1 AND d.status = 'delivered'
            "#,
            &[&merchant_id],
        )
        .await?
        .get(0);

    // Get failed deliveries
    let failed_deliveries: i64 = client
        .query_one(
            r#"
            SELECT COUNT(*) 
            FROM webhook_deliveries d
            JOIN webhook_endpoints e ON e.id = d.endpoint_id
            WHERE e.merchant_id = $1 AND d.status = 'exhausted'
            "#,
            &[&merchant_id],
        )
        .await?
        .get(0);

    // Get pending deliveries
    let pending_deliveries: i64 = client
        .query_one(
            r#"
            SELECT COUNT(*) 
            FROM webhook_deliveries d
            JOIN webhook_endpoints e ON e.id = d.endpoint_id
            WHERE e.merchant_id = $1 AND d.status = 'pending'
            "#,
            &[&merchant_id],
        )
        .await?
        .get(0);

    Ok(Json(WebhookDashboardStats {
        total_endpoints,
        active_endpoints,
        total_deliveries,
        successful_deliveries,
        failed_deliveries,
        pending_deliveries,
    }))
}

// Trigger a test webhook event (for testing purposes)
#[derive(Debug, Deserialize)]
pub struct TestWebhookRequest {
    pub event_type: String,
    pub payload: serde_json::Value,
}

pub async fn test_webhook(
    State(services): State<Arc<ServiceContainer>>,
    Path(merchant_id): Path<String>,
    Json(request): Json<TestWebhookRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    services
        .webhook
        .dispatch_event(&request.event_type, request.payload)
        .await?;

    Ok(Json(serde_json::json!({
        "message": "Webhook event dispatched successfully",
        "event_type": request.event_type
    })))
}
