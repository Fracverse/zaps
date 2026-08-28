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
    .bind(&batch_id)
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
    .bind(&batch_id)
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

    Json(BatchDetailResponse { batch, recipients })
    .into_response()
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
        [(header::CONTENT_TYPE, "text/csv; charset=utf-8"), (header::CONTENT_DISPOSITION, "attachment; filename=\"payout-results.csv\"")],
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

/// POST /api/payouts/batch
/// Create a new batch payout.
#[derive(Deserialize)]
pub struct CreateBatchRequest {
    pub idempotency_key: String,
    pub currency: String,
    pub total_recipients: i32,
    pub total_amount: i64,
    pub created_by: String, // UUID as string
}

#[derive(Serialize)]
pub struct CreateBatchResponse {
    pub batch_id: String,
    pub status: String,
}

pub async fn create_batch(
    State(pool): State<PgPool>,
    Json(payload): Json<CreateBatchRequest>,
) -> impl IntoResponse {
    let created_by: Uuid = match payload.created_by.parse() {
        Ok(id) => id,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "Invalid user ID format"
                })),
            )
                .into_response();
        }
    };

    let batch_id = match sqlx::query(
        r#"
        INSERT INTO payout_batches (idempotency_key, created_by, currency, total_recipients, total_amount, status)
        VALUES ($1, $2, $3, $4, $5, 'PENDING')
        RETURNING id, status
        "#,
    )
    .bind(&payload.idempotency_key)
    .bind(&created_by)
    .bind(&payload.currency)
    .bind(&payload.total_recipients)
    .bind(&payload.total_amount)
    .fetch_one(&pool)
    .await
    {
        Ok(row) => row,
        Err(e) => {
            tracing::error!("Failed to create payout batch: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "Failed to create payout batch"
                })),
            )
                .into_response();
        }
    };

    let id: Uuid = batch_id.get("id");
    let status: String = batch_id.get("status");

    Json(CreateBatchResponse {
        batch_id: id.to_string(),
        status,
    })
    .into_response()
}

/// POST /api/payouts/sdp/webhook
///
/// Stellar Disbursement Platform (SDP) reconciliation webhook receiver.
///
/// 1. Validates the `X-SDP-Signature` header as an HMAC-SHA256 of the raw
///    request body keyed by `SDP_WEBHOOK_SECRET` (constant-time compare).
/// 2. On success, updates the referenced batch recipient's state in the
///    database (status + tx_hash).  See issue #727.
pub async fn sdp_reconciliation_webhook(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    // ── 1. Resolve the webhook secret ────────────────────────────────────────
    let secret = match std::env::var("SDP_WEBHOOK_SECRET") {
        Ok(s) if !s.is_empty() => s,
        _ => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Webhook secret not configured" })),
            )
                .into_response();
        }
    };

    // ── 2. Read the supplied signature ───────────────────────────────────────
    let signature = match headers
        .get("X-SDP-Signature")
        .and_then(|v| v.to_str().ok())
    {
        Some(s) => s,
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

    let recipient_id: Uuid = match payload.id.parse() {
        Ok(id) => id,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "Invalid recipient id" })),
            )
                .into_response();
        }
    };

    // ── 5. Update the batch item state ──────────────────────────────────────
    match sqlx::query(
        "UPDATE batch_recipients SET status = $1, tx_hash = COALESCE($2, tx_hash) WHERE id = $3",
    )
    .bind(&payload.status)
    .bind(&payload.tx_hash)
    .bind(recipient_id)
    .execute(&pool)
    .await
    {
        Ok(_) => (StatusCode::OK, Json(serde_json::json!({ "status": "ok" }))).into_response(),
        Err(e) => {
            tracing::error!("Failed to update batch recipient {}: {e}", payload.id);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to update batch item" })),
            )
                .into_response()
        }
    }
}

/// SDP reconciliation webhook payload.
#[derive(Debug, Deserialize)]
struct SdpReconciliationPayload {
    /// Batch recipient id (UUID string).
    pub id: String,
    /// New disbursement status reported by SDP (e.g. "SUCCESS", "FAILED").
    pub status: String,
    /// Transaction hash, when available.
    #[serde(default)]
    pub tx_hash: Option<String>,
}

/// Constant-time comparison to avoid leaking the signature via timing.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}
