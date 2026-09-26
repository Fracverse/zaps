use axum::{
    body::Bytes,
    extract::State,
    http::{header, HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use chrono::Utc;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::db::models::PayoutBatch;

/// HMAC-SHA256 construction used to verify SDP webhook signatures.
type HmacSha256 = Hmac<Sha256>;

/// GET /api/payouts/batches
/// Return paginated list of batch payout runs (from payout_batches table).
#[derive(Deserialize)]
pub struct ListBatchesQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Serialize)]
pub struct BatchListItem {
    pub id: String,
    pub status: String,
    pub currency: String,
    pub total_recipients: i32,
    pub total_amount: i64,
    pub succeeded_count: i32,
    pub failed_count: i32,
    pub created_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
}

#[derive(Serialize)]
pub struct ListBatchesResponse {
    pub batches: Vec<BatchListItem>,
    pub limit: i64,
    pub offset: i64,
    pub total: i64,
}

pub async fn list_batches(
    State(pool): State<PgPool>,
    query: axum::extract::Query<ListBatchesQuery>,
) -> impl IntoResponse {
    let limit = query.limit.unwrap_or(20).clamp(1, 100);
    let offset = query.offset.unwrap_or(0).max(0);

    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM payout_batches")
        .fetch_one(&pool)
        .await
        .unwrap_or(0);

    let rows = sqlx::query(
        r#"
        SELECT id, status, currency, total_recipients, total_amount,
               succeeded_count, failed_count, created_at, started_at, completed_at
        FROM payout_batches
        ORDER BY created_at DESC
        LIMIT $1 OFFSET $2
        "#,
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(&pool)
    .await;

    let batches: Vec<BatchListItem> = match rows {
        Ok(rows) => rows
            .iter()
            .map(|r| {
                let id: Uuid = r.get("id");
                let created_at: chrono::NaiveDateTime = r.get("created_at");
                BatchListItem {
                    id: id.to_string(),
                    status: r.get("status"),
                    currency: r.get("currency"),
                    total_recipients: r.get("total_recipients"),
                    total_amount: r.get("total_amount"),
                    succeeded_count: r.get("succeeded_count"),
                    failed_count: r.get("failed_count"),
                    created_at: created_at.to_string(),
                    started_at: r
                        .get::<Option<chrono::NaiveDateTime>, _>("started_at")
                        .map(|t| t.to_string()),
                    completed_at: r
                        .get::<Option<chrono::NaiveDateTime>, _>("completed_at")
                        .map(|t| t.to_string()),
                }
            })
            .collect(),
        Err(e) => {
            tracing::error!("Failed to query payout batches: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "Failed to retrieve payout batches"
                })),
            )
                .into_response();
        }
    };

    Json(ListBatchesResponse {
        batches,
        limit,
        offset,
        total,
    })
    .into_response()
}

/// GET /api/payouts/batch/:id
/// Return detailed information about a specific batch payout.
#[derive(Serialize)]
pub struct BatchDetailResponse {
    pub batch: BatchListItem,
    pub recipients: Vec<BatchRecipientSummary>,
}

#[derive(Serialize)]
pub struct BatchRecipientSummary {
    pub id: String,
    pub user_id: Option<String>,
    pub destination_address: Option<String>,
    pub amount: i64,
    pub status: String,
    pub tx_hash: Option<String>,
    pub attempt_count: i32,
    pub created_at: String,
}

pub async fn get_batch_detail(
    State(pool): State<PgPool>,
    axum::extract::Path(batch_id): axum::extract::Path<String>,
) -> impl IntoResponse {
    let batch_id: Uuid = match batch_id.parse() {
        Ok(id) => id,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "Invalid batch ID format"
                })),
            )
                .into_response();
        }
    };

    // Get batch info
    let batch_row = match sqlx::query(
        r#"
        SELECT id, status, currency, total_recipients, total_amount,
               succeeded_count, failed_count, created_at, started_at, completed_at
        FROM payout_batches
        WHERE id = $1
        "#,
    )
    .bind(batch_id)
    .fetch_optional(&pool)
    .await
    {
        Ok(Some(row)) => row,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({
                    "error": "Batch not found"
                })),
            )
                .into_response();
        }
        Err(e) => {
            tracing::error!("Failed to query batch {}: {:?}", batch_id, e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "Failed to retrieve batch details"
                })),
            )
                .into_response();
        }
    };

    let created_at: chrono::NaiveDateTime = batch_row.get("created_at");
    let batch = BatchListItem {
        id: batch_id.to_string(),
        status: batch_row.get("status"),
        currency: batch_row.get("currency"),
        total_recipients: batch_row.get("total_recipients"),
        total_amount: batch_row.get("total_amount"),
        succeeded_count: batch_row.get("succeeded_count"),
        failed_count: batch_row.get("failed_count"),
        created_at: created_at.to_string(),
        started_at: batch_row
            .get::<Option<chrono::NaiveDateTime>, _>("started_at")
            .map(|t| t.to_string()),
        completed_at: batch_row
            .get::<Option<chrono::NaiveDateTime>, _>("completed_at")
            .map(|t| t.to_string()),
    };

    // Get recipient list
    let recipients_rows = match sqlx::query(
        r#"
        SELECT id, user_id, destination_address, amount, status,
               tx_hash, attempt_count, created_at
        FROM batch_recipients
        WHERE batch_id = $1
        ORDER BY created_at ASC
        "#,
    )
    .bind(batch_id)
    .fetch_all(&pool)
    .await
    {
        Ok(rows) => rows,
        Err(e) => {
            tracing::error!("Failed to query batch recipients {}: {:?}", batch_id, e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "Failed to retrieve batch recipients"
                })),
            )
                .into_response();
        }
    };

    let recipients: Vec<BatchRecipientSummary> = recipients_rows
        .iter()
        .map(|r| {
            let id: Uuid = r.get("id");
            let created_at: chrono::NaiveDateTime = r.get("created_at");
            BatchRecipientSummary {
                id: id.to_string(),
                user_id: r.get::<Option<Uuid>, _>("user_id").map(|u| u.to_string()),
                destination_address: r.get("destination_address"),
                amount: r.get("amount"),
                status: r.get("status"),
                tx_hash: r.get("tx_hash"),
                attempt_count: r.get("attempt_count"),
                created_at: created_at.to_string(),
            }
        })
        .collect();

    Json(BatchDetailResponse { batch, recipients }).into_response()
}

/// GET /api/payouts/batch/:id/export
/// Export the recipient results as a CSV download.
pub async fn export_batch(
    State(pool): State<PgPool>,
    axum::extract::Path(batch_id): axum::extract::Path<String>,
) -> impl IntoResponse {
    let batch_id: Uuid = match batch_id.parse() {
        Ok(id) => id,
        Err(_) => return (StatusCode::BAD_REQUEST, "Invalid batch ID format").into_response(),
    };

    let rows = match sqlx::query(
        "SELECT destination_address, amount, status, tx_hash FROM batch_recipients WHERE batch_id = $1 ORDER BY created_at ASC",
    )
    .bind(batch_id)
    .fetch_all(&pool)
    .await
    {
        Ok(rows) => rows,
        Err(error) => {
            tracing::error!(%error, %batch_id, "Failed to export payout batch");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to export payout batch").into_response();
        }
    };

    let mut csv = String::from("recipient,amount,status,tx_hash\n");
    for row in rows {
        let recipient: Option<String> = row.try_get("destination_address").unwrap_or(None);
        let amount: i64 = row.try_get("amount").unwrap_or_default();
        let status: String = row.try_get("status").unwrap_or_default();
        let tx_hash: Option<String> = row.try_get("tx_hash").unwrap_or(None);
        csv.push_str(&format!(
            "{},{},{},{}\n",
            csv_field(recipient.as_deref().unwrap_or("")),
            amount,
            csv_field(&status),
            csv_field(tx_hash.as_deref().unwrap_or("")),
        ));
    }

    (
        [
            (header::CONTENT_TYPE, "text/csv; charset=utf-8"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=\"payout-results.csv\"",
            ),
        ],
        csv,
    )
        .into_response()
}

fn csv_field(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_owned()
    }
}

/// POST /api/payouts/batch and /api/payouts/batch/csv (#935) — accepting the
/// actual JSON/CSV payout records and validating them — live in
/// `api::bridge` (`batch_upload` / `batch_upload_csv`), alongside the shared
/// `persist_batch` helper. They're wired in `api::payout_routes`.

/// POST /api/payouts/sdp/webhook
///
/// Stellar Disbursement Platform (SDP) reconciliation webhook receiver.
///
/// 1. Validates the `X-SDP-Signature` (or `X-Payload-Signature` / `X-Stellar-Signature`)
///    header as an HMAC-SHA256 of the raw request body keyed by `SDP_WEBHOOK_SECRET`
///    (using constant-time comparison to prevent timing attacks).
/// 2. Normalizes the incoming disbursement status to match the database constraint
///    ('PENDING', 'SUBMITTED', 'CONFIRMED', 'FAILED').
/// 3. Updates the recipient state in `batch_recipients` by recipient ID or `sdp_payment_id`.
/// 4. Records an audit entry in `dispatch_logs`.
/// 5. Automatically updates parent batch totals and terminal status in `payout_batches`.
pub async fn sdp_reconciliation_webhook(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    // ── 1. Resolve the webhook secret ────────────────────────────────────────
    let secret = match std::env::var("SDP_WEBHOOK_SECRET")
        .or_else(|_| std::env::var("ANCHOR_WEBHOOK_SECRET"))
        .or_else(|_| std::env::var("PLATFORM_SECRET_KEY"))
    {
        Ok(s) if !s.is_empty() => s,
        _ => {
            tracing::error!("SDP webhook secret not configured");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Webhook secret not configured" })),
            )
                .into_response();
        }
    };

    // ── 2. Read the supplied signature ───────────────────────────────────────
    let signature = headers
        .get("X-SDP-Signature")
        .or_else(|| headers.get("x-sdp-signature"))
        .or_else(|| headers.get("X-Payload-Signature"))
        .or_else(|| headers.get("x-payload-signature"))
        .or_else(|| headers.get("X-Stellar-Signature"))
        .or_else(|| headers.get("x-stellar-signature"))
        .and_then(|v| v.to_str().ok());

    let signature = match signature {
        Some(s) => s.trim(),
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "error": "Missing signature header" })),
            )
                .into_response();
        }
    };

    // ── 3. Compute HMAC-SHA256 of the raw body and compare ───────────────────
    let mut mac = match HmacSha256::new_from_slice(secret.as_bytes()) {
        Ok(m) => m,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Invalid webhook secret" })),
            )
                .into_response();
        }
    };
    mac.update(&body);
    let computed = hex::encode(mac.finalize().into_bytes());

    if !constant_time_eq(computed.as_bytes(), signature.as_bytes()) {
        tracing::warn!("SDP webhook signature verification failed");
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "error": "Invalid signature" })),
        )
            .into_response();
    }

    // ── 4. Parse the reconciliation payload ─────────────────────────────────
    let payload: SdpReconciliationPayload = match serde_json::from_slice(&body) {
        Ok(p) => p,
        Err(e) => {
            tracing::error!("SDP webhook payload parse error: {e}");
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "Invalid payload" })),
            )
                .into_response();
        }
    };

    // Determine recipient UUID from external_id (our recipient_id) or id
    let recipient_id = payload
        .external_id
        .as_deref()
        .or(payload.recipient_id.as_deref())
        .or(payload.id.as_deref())
        .and_then(|id_str| Uuid::parse_str(id_str).ok());

    let sdp_payment_id = payload.sdp_payment_id.as_deref().or(payload.id.as_deref());

    if recipient_id.is_none() && sdp_payment_id.is_none() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "Missing valid recipient or payment identifier" })),
        )
            .into_response();
    }

    // Normalize status according to database CHECK constraints
    let normalized_status = match payload.status.to_uppercase().as_str() {
        "SUCCESS" | "SUCCESSFUL" | "COMPLETED" | "CONFIRMED" => "CONFIRMED",
        "FAILED" | "CANCELLED" | "CANCELED" | "ERROR" => "FAILED",
        "SUBMITTED" => "SUBMITTED",
        "PENDING" => "PENDING",
        _ => "FAILED",
    };

    // ── 5. Update the batch item state ──────────────────────────────────────
    let updated_row = match (recipient_id, sdp_payment_id) {
        (Some(rec_id), Some(sdp_id)) => {
            sqlx::query(
                r#"
                UPDATE batch_recipients
                SET status = $1,
                    tx_hash = COALESCE($2, tx_hash),
                    sdp_payment_id = COALESCE($3, sdp_payment_id),
                    last_error = COALESCE($4, last_error),
                    updated_at = NOW()
                WHERE id = $5 OR (sdp_payment_id IS NOT NULL AND sdp_payment_id = $3)
                RETURNING id, batch_id, status
                "#,
            )
            .bind(normalized_status)
            .bind(&payload.tx_hash)
            .bind(sdp_id)
            .bind(&payload.error_message)
            .bind(rec_id)
            .fetch_optional(&pool)
            .await
        }
        (Some(rec_id), None) => {
            sqlx::query(
                r#"
                UPDATE batch_recipients
                SET status = $1,
                    tx_hash = COALESCE($2, tx_hash),
                    last_error = COALESCE($3, last_error),
                    updated_at = NOW()
                WHERE id = $4
                RETURNING id, batch_id, status
                "#,
            )
            .bind(normalized_status)
            .bind(&payload.tx_hash)
            .bind(&payload.error_message)
            .bind(rec_id)
            .fetch_optional(&pool)
            .await
        }
        (None, Some(sdp_id)) => {
            sqlx::query(
                r#"
                UPDATE batch_recipients
                SET status = $1,
                    tx_hash = COALESCE($2, tx_hash),
                    last_error = COALESCE($3, last_error),
                    updated_at = NOW()
                WHERE sdp_payment_id = $4
                RETURNING id, batch_id, status
                "#,
            )
            .bind(normalized_status)
            .bind(&payload.tx_hash)
            .bind(&payload.error_message)
            .bind(sdp_id)
            .fetch_optional(&pool)
            .await
        }
        (None, None) => unreachable!(),
    };

    let (rec_id, batch_id) = match updated_row {
        Ok(Some(row)) => {
            let r_id: Uuid = row.get("id");
            let b_id: Uuid = row.get("batch_id");
            (r_id, b_id)
        }
        Ok(None) => {
            tracing::warn!("SDP webhook received for non-existent recipient");
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "Recipient record not found" })),
            )
                .into_response();
        }
        Err(e) => {
            tracing::error!("Failed to update batch recipient from webhook: {e}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Database update failed" })),
            )
                .into_response();
        }
    };

    // ── 6. Record audit log in dispatch_logs ────────────────────────────────
    let log_event = match normalized_status {
        "CONFIRMED" => "CONFIRMED",
        "FAILED" => "FAILED",
        "SUBMITTED" => "SUBMITTED",
        _ => "CONFIRMED",
    };
    let detail_msg = payload
        .error_message
        .as_deref()
        .unwrap_or("Reconciled from SDP webhook");

    let _ = sqlx::query(
        r#"
        INSERT INTO dispatch_logs (batch_id, recipient_id, attempt, event, detail)
        VALUES ($1, $2, 1, $3, $4)
        "#,
    )
    .bind(batch_id)
    .bind(rec_id)
    .bind(log_event)
    .bind(detail_msg)
    .execute(&pool)
    .await;

    // ── 7. Reconcile parent batch status ────────────────────────────────────
    let _ = sqlx::query(
        r#"
        UPDATE payout_batches
        SET succeeded_count = (SELECT COUNT(*) FROM batch_recipients WHERE batch_id = $1 AND status = 'CONFIRMED'),
            failed_count = (SELECT COUNT(*) FROM batch_recipients WHERE batch_id = $1 AND status = 'FAILED'),
            updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(batch_id)
    .execute(&pool)
    .await;

    let _ = sqlx::query(
        r#"
        UPDATE payout_batches
        SET status = CASE
                WHEN failed_count = 0 THEN 'COMPLETED'
                WHEN succeeded_count = 0 THEN 'FAILED'
                ELSE 'PARTIALLY_FAILED'
            END,
            completed_at = NOW(),
            updated_at = NOW()
        WHERE id = $1
          AND status IN ('PENDING', 'PROCESSING')
          AND (succeeded_count + failed_count) >= total_recipients
          AND total_recipients > 0
        "#,
    )
    .bind(batch_id)
    .execute(&pool)
    .await;

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "ok",
            "recipient_id": rec_id,
            "reconciled_status": normalized_status
        })),
    )
        .into_response()
}

/// SDP reconciliation webhook payload format supporting standard and custom SDP callbacks.
#[derive(Debug, Deserialize)]
pub struct SdpReconciliationPayload {
    /// Batch recipient id or SDP payment id (UUID or string).
    #[serde(default)]
    pub id: Option<String>,
    /// External recipient id provided during disbursement creation.
    #[serde(default)]
    pub external_id: Option<String>,
    /// Optional recipient id field alias.
    #[serde(default)]
    pub recipient_id: Option<String>,
    /// SDP internal payment id.
    #[serde(default)]
    pub sdp_payment_id: Option<String>,
    /// New disbursement status reported by SDP (e.g. "SUCCESS", "COMPLETED", "CONFIRMED", "FAILED").
    pub status: String,
    /// Transaction hash, when available.
    #[serde(default)]
    pub tx_hash: Option<String>,
    /// Optional error or failure message.
    #[serde(default)]
    pub error_message: Option<String>,
}

/// Constant-time comparison to avoid leaking the signature via timing attacks.
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constant_time_eq_matching() {
        let a = b"5d41402abc4b2a76b9719d911017c592";
        let b = b"5d41402abc4b2a76b9719d911017c592";
        assert!(constant_time_eq(a, b));
    }

    #[test]
    fn test_constant_time_eq_mismatch() {
        let a = b"5d41402abc4b2a76b9719d911017c592";
        let b = b"5d41402abc4b2a76b9719d911017c593";
        assert!(!constant_time_eq(a, b));
    }

    #[test]
    fn test_constant_time_eq_different_lengths() {
        let a = b"5d41402abc";
        let b = b"5d41402abc4b2a76b9719d911017c592";
        assert!(!constant_time_eq(a, b));
    }

    #[test]
    fn test_hmac_sha256_computation() {
        let secret = "test_webhook_secret_key";
        let body = br#"{"id":"00000000-0000-0000-0000-000000000001","status":"SUCCESS","tx_hash":"0xabc123"}"#;

        let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(body);
        let signature = hex::encode(mac.finalize().into_bytes());

        assert_eq!(signature.len(), 64);

        // Verify that same input produces identical signature
        let mut mac2 = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
        mac2.update(body);
        let signature2 = hex::encode(mac2.finalize().into_bytes());
        assert!(constant_time_eq(
            signature.as_bytes(),
            signature2.as_bytes()
        ));

        // Verify that different body produces different signature
        let mut mac3 = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
        mac3.update(b"tampered body");
        let signature3 = hex::encode(mac3.finalize().into_bytes());
        assert!(!constant_time_eq(
            signature.as_bytes(),
            signature3.as_bytes()
        ));
    }

    #[test]
    fn test_sdp_payload_deserialization() {
        let valid_json = r#"{
            "id": "11111111-2222-3333-4444-555555555555",
            "status": "COMPLETED",
            "tx_hash": "tx-stellar-hash-123"
        }"#;

        let payload: SdpReconciliationPayload = serde_json::from_str(valid_json).unwrap();
        assert_eq!(
            payload.id,
            Some("11111111-2222-3333-4444-555555555555".to_string())
        );
        assert_eq!(payload.status, "COMPLETED");
        assert_eq!(payload.tx_hash, Some("tx-stellar-hash-123".to_string()));
    }

    #[test]
    fn test_sdp_payload_with_external_id() {
        let json = r#"{
            "id": "sdp-disbursement-999",
            "external_id": "11111111-2222-3333-4444-555555555555",
            "status": "SUCCESS",
            "tx_hash": "tx-hash-456",
            "error_message": null
        }"#;

        let payload: SdpReconciliationPayload = serde_json::from_str(json).unwrap();
        assert_eq!(payload.id, Some("sdp-disbursement-999".to_string()));
        assert_eq!(
            payload.external_id,
            Some("11111111-2222-3333-4444-555555555555".to_string())
        );
        assert_eq!(payload.status, "SUCCESS");
    }

    #[test]
    fn test_sdp_status_normalization_logic() {
        let normalize = |s: &str| match s.to_uppercase().as_str() {
            "SUCCESS" | "SUCCESSFUL" | "COMPLETED" | "CONFIRMED" => "CONFIRMED",
            "FAILED" | "CANCELLED" | "CANCELED" | "ERROR" => "FAILED",
            "SUBMITTED" => "SUBMITTED",
            "PENDING" => "PENDING",
            _ => "FAILED",
        };

        assert_eq!(normalize("SUCCESS"), "CONFIRMED");
        assert_eq!(normalize("completed"), "CONFIRMED");
        assert_eq!(normalize("CONFIRMED"), "CONFIRMED");
        assert_eq!(normalize("FAILED"), "FAILED");
        assert_eq!(normalize("cancelled"), "FAILED");
        assert_eq!(normalize("error"), "FAILED");
        assert_eq!(normalize("SUBMITTED"), "SUBMITTED");
        assert_eq!(normalize("pending"), "PENDING");
        assert_eq!(normalize("unknown_status"), "FAILED");
    }

    #[tokio::test]
    async fn test_sdp_webhook_rejects_missing_signature() {
        use axum::body::Body;
        use axum::http::Request;
        use axum::Router;
        use tower::ServiceExt;

        std::env::set_var("SDP_WEBHOOK_SECRET", "test_secret_123");

        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://localhost/dummy")
            .unwrap();

        let app = Router::new()
            .route(
                "/sdp/webhook",
                axum::routing::post(sdp_reconciliation_webhook),
            )
            .with_state(pool);

        let req = Request::builder()
            .method("POST")
            .uri("/sdp/webhook")
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"id":"00000000-0000-0000-0000-000000000001","status":"SUCCESS"}"#,
            ))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_sdp_webhook_rejects_invalid_signature() {
        use axum::body::Body;
        use axum::http::Request;
        use axum::Router;
        use tower::ServiceExt;

        std::env::set_var("SDP_WEBHOOK_SECRET", "test_secret_123");

        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://localhost/dummy")
            .unwrap();

        let app = Router::new()
            .route(
                "/sdp/webhook",
                axum::routing::post(sdp_reconciliation_webhook),
            )
            .with_state(pool);

        let req = Request::builder()
            .method("POST")
            .uri("/sdp/webhook")
            .header("content-type", "application/json")
            .header(
                "X-SDP-Signature",
                "invalid_forged_signature_hex_digest_value_1234567890abcdef",
            )
            .body(Body::from(
                r#"{"id":"00000000-0000-0000-0000-000000000001","status":"SUCCESS"}"#,
            ))
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }
}
