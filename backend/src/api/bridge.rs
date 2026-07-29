use crate::services::allbridge::{
    AllbridgeClient, AllbridgeQuoteRequest, BridgeStatusKind, BridgeTransferStatus,
};
use axum::{
    extract::{Multipart, Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::sync::Arc;
use std::time::Duration;

/// Shared state for the bridge routes: DB pool + Allbridge API client.
#[derive(Clone)]
pub struct BridgeState {
    pub pool: sqlx::PgPool,
    pub allbridge: Arc<AllbridgeClient>,
}

impl BridgeState {
    pub fn new(pool: sqlx::PgPool, allbridge_api_url: String) -> Self {
        Self {
            pool,
            allbridge: Arc::new(AllbridgeClient::new(allbridge_api_url)),
        }
    }
}

#[derive(Deserialize)]
pub struct BridgeQuoteRequest {
    pub source_chain: String,
    pub source_token: String,
    pub amount: String,
    pub destination_chain: String,
    pub destination_token: String,
    pub destination_address: String,
}

#[derive(Serialize)]
pub struct BridgeQuoteResponse {
    pub fee: String,
    pub receive_amount: String,
    pub bridge_tx_data: String, // Payload details to construct user-side wallet signature
}

#[derive(Deserialize)]
pub struct SubmitBridgeTxRequest {
    pub source_tx_hash: String,
    /// Allbridge chain symbol of the deposit (defaults to Stellar).
    #[serde(default = "default_source_chain")]
    pub source_chain: String,
    pub destination_chain: Option<String>,
    pub destination_address: Option<String>,
    pub amount: Option<String>,
}

fn default_source_chain() -> String {
    "STLR".to_string()
}

#[derive(Serialize)]
pub struct BridgeStatusResponse {
    pub source_tx_hash: String,
    pub source_chain: String,
    pub destination_chain: Option<String>,
    pub status: String, // PENDING, SUCCESS, FAILED
    pub confirmations: i32,
    pub updated_at: String,
}

pub async fn get_quote(
    State(state): State<BridgeState>,
    Json(payload): Json<BridgeQuoteRequest>,
) -> impl IntoResponse {
    if payload.source_chain.trim().is_empty()
        || payload.destination_chain.trim().is_empty()
        || payload.amount.trim().is_empty()
        || payload.source_token.trim().is_empty()
        || payload.destination_token.trim().is_empty()
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "source_chain, destination_chain, amount, source_token and destination_token are required" })),
        )
            .into_response();
    }

    let amount_value = payload.amount.parse::<u64>();
    if amount_value.is_err() || amount_value.unwrap_or_default() == 0 {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "amount must be a positive integer" })),
        )
            .into_response();
    }

    let quote_request = AllbridgeQuoteRequest {
        source_chain: payload.source_chain,
        source_token: payload.source_token,
        amount: payload.amount,
        destination_chain: payload.destination_chain,
        destination_token: payload.destination_token,
        destination_address: payload.destination_address,
    };

    match state.allbridge.get_price_quote(&quote_request).await {
        Ok(quote) => Json(BridgeQuoteResponse {
            fee: quote.fee,
            receive_amount: quote.receive_amount,
            bridge_tx_data: quote.bridge_tx_data,
        })
        .into_response(),
        Err(err) => (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({ "error": err.message })),
        )
            .into_response(),
    }
}

/// BE-017: Record a submitted cross-chain deposit so its status can be tracked/polled.
pub async fn submit_bridge_tx(
    State(state): State<BridgeState>,
    Json(payload): Json<SubmitBridgeTxRequest>,
) -> impl IntoResponse {
    if payload.source_tx_hash.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "source_tx_hash is required" })),
        )
            .into_response();
    }

    // Insert the deposit in PENDING state. Re-submitting the same hash is idempotent.
    let result = sqlx::query(
        r#"
        INSERT INTO bridge_transactions
            (source_tx_hash, source_chain, destination_chain, destination_address, amount, status)
        VALUES ($1, $2, $3, $4, $5, 'PENDING')
        ON CONFLICT (source_tx_hash) DO UPDATE SET updated_at = NOW()
        RETURNING id, status
        "#,
    )
    .bind(&payload.source_tx_hash)
    .bind(&payload.source_chain)
    .bind(&payload.destination_chain)
    .bind(&payload.destination_address)
    .bind(&payload.amount)
    .fetch_one(&state.pool)
    .await;

    match result {
        Ok(row) => {
            let id: uuid::Uuid = row.get("id");
            let status: String = row.get("status");
            Json(serde_json::json!({
                "id": id.to_string(),
                "source_tx_hash": payload.source_tx_hash,
                "status": status,
            }))
            .into_response()
        }
        Err(e) => {
            tracing::error!("Failed to record bridge transaction: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to record bridge transaction" })),
            )
                .into_response()
        }
    }
}

/// BE-017: Return the live status of a bridged deposit.
///
/// The path segment is the source-chain transaction hash. If the stored status is
/// not yet terminal, Allbridge is polled for a fresh status and the row is updated.
pub async fn get_bridge_status(
    State(state): State<BridgeState>,
    Path(tx_id): Path<String>,
) -> impl IntoResponse {
    // Load the tracked deposit.
    let row = match sqlx::query(
        r#"
        SELECT source_tx_hash, source_chain, destination_chain, status, confirmations
        FROM bridge_transactions
        WHERE source_tx_hash = $1
        "#,
    )
    .bind(&tx_id)
    .fetch_optional(&state.pool)
    .await
    {
        Ok(Some(row)) => row,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "Bridge transaction not found" })),
            )
                .into_response();
        }
        Err(e) => {
            tracing::error!("Failed to load bridge transaction: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to load bridge transaction" })),
            )
                .into_response();
        }
    };

    let source_chain: String = row.get("source_chain");
    let destination_chain: Option<String> = row.get("destination_chain");
    let stored_status: String = row.get("status");
    let mut confirmations: i32 = row.get("confirmations");
    let mut status = stored_status.clone();

    // Only poll the external API while the transfer is still in flight.
    if !is_terminal(&stored_status) {
        match state
            .allbridge
            .poll_transaction_status(&source_chain, &tx_id)
            .await
        {
            Ok(BridgeTransferStatus {
                status: kind,
                confirmations: fresh_conf,
            }) => {
                status = kind.as_str().to_string();
                confirmations = fresh_conf.max(confirmations);
                update_status(&state.pool, &tx_id, &status, confirmations).await;
            }
            Err(e) => {
                // Network/API hiccup: fall back to the last known status instead of failing.
                tracing::warn!("Allbridge status poll failed for {}: {:?}", tx_id, e);
            }
        }
    }

    let updated_at = fetch_updated_at(&state.pool, &tx_id).await;

    Json(BridgeStatusResponse {
        source_tx_hash: tx_id,
        source_chain,
        destination_chain,
        status,
        confirmations,
        updated_at,
    })
    .into_response()
}

fn is_terminal(status: &str) -> bool {
    matches!(status, "SUCCESS" | "FAILED")
}

async fn update_status(pool: &sqlx::PgPool, tx_hash: &str, status: &str, confirmations: i32) {
    if let Err(e) = sqlx::query(
        r#"
        UPDATE bridge_transactions
        SET status = $2, confirmations = $3, updated_at = NOW()
        WHERE source_tx_hash = $1
        "#,
    )
    .bind(tx_hash)
    .bind(status)
    .bind(confirmations)
    .execute(pool)
    .await
    {
        tracing::error!("Failed to persist bridge status for {}: {:?}", tx_hash, e);
    }
}

async fn fetch_updated_at(pool: &sqlx::PgPool, tx_hash: &str) -> String {
    sqlx::query("SELECT updated_at FROM bridge_transactions WHERE source_tx_hash = $1")
        .bind(tx_hash)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .map(|row| {
            let ts: chrono::NaiveDateTime = row.get("updated_at");
            ts.and_utc().to_rfc3339()
        })
        .unwrap_or_default()
}

/// Background task: periodically refresh all non-terminal bridge transactions.
///
/// This gives the dashboard up-to-date statuses even if no client is actively
/// hitting the status endpoint.
pub async fn run_status_poller(state: BridgeState) {
    const POLL_INTERVAL: Duration = Duration::from_secs(30);
    let mut interval = tokio::time::interval(POLL_INTERVAL);

    loop {
        interval.tick().await;

        let pending = sqlx::query(
            r#"
            SELECT source_tx_hash, source_chain, confirmations
            FROM bridge_transactions
            WHERE status NOT IN ('SUCCESS', 'FAILED')
            ORDER BY created_at ASC
            LIMIT 100
            "#,
        )
        .fetch_all(&state.pool)
        .await;

        let rows = match pending {
            Ok(rows) => rows,
            Err(e) => {
                tracing::error!("Bridge poller failed to load pending transactions: {:?}", e);
                continue;
            }
        };

        for row in rows {
            let tx_hash: String = row.get("source_tx_hash");
            let source_chain: String = row.get("source_chain");
            let known_conf: i32 = row.get("confirmations");

            match state
                .allbridge
                .poll_transaction_status(&source_chain, &tx_hash)
                .await
            {
                Ok(status) => {
                    let new_conf = status.confirmations.max(known_conf);
                    // Only write when something actually changed.
                    if status.status != BridgeStatusKind::Pending || new_conf != known_conf {
                        update_status(&state.pool, &tx_hash, status.status.as_str(), new_conf)
                            .await;
                    }
                }
                Err(e) => {
                    tracing::warn!("Bridge poller: status poll failed for {}: {:?}", tx_hash, e);
                }
            }
        }
    }
}

// ── #553 Batch Payout Upload ──────────────────────────────────────────────────

/// A single disbursement record accepted in both JSON-array and CSV upload modes.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PayoutRecord {
    /// Destination Stellar address or registered username.
    pub destination: String,
    /// Amount in stroops (1 XLM = 10_000_000 stroops) or as a decimal string.
    pub amount: String,
    /// Optional human-readable note attached to the payment.
    #[serde(default)]
    pub memo: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct BatchJsonPayload {
    pub payouts: Vec<PayoutRecord>,
}

#[derive(Serialize)]
pub struct BatchUploadResponse {
    pub accepted: usize,
    pub rejected: usize,
    pub errors: Vec<String>,
    pub batch_id: Option<String>,
}

/// Validate a single payout record, returning an error description if invalid.
fn validate_record(idx: usize, record: &PayoutRecord) -> Option<String> {
    if record.destination.trim().is_empty() {
        return Some(format!("row {}: destination is required", idx + 1));
    }
    let amount_str = record.amount.trim().replace(',', "");
    match amount_str.parse::<f64>() {
        Ok(v) if v <= 0.0 => Some(format!("row {}: amount must be positive", idx + 1)),
        Err(_) => Some(format!(
            "row {}: amount '{}' is not a valid number",
            idx + 1,
            record.amount
        )),
        Ok(_) => None,
    }
}

/// Parse a CSV byte slice into a list of payout records.
/// Expected CSV columns (header row required): destination, amount, memo (optional).
fn parse_csv(data: &[u8]) -> Result<Vec<PayoutRecord>, String> {
    let text = std::str::from_utf8(data).map_err(|_| "CSV is not valid UTF-8".to_string())?;
    let mut lines = text.lines();

    // Parse header row
    let header_line = lines
        .next()
        .ok_or_else(|| "CSV file is empty".to_string())?;
    let headers: Vec<&str> = header_line.split(',').map(|h| h.trim()).collect();

    let dest_col = headers
        .iter()
        .position(|h| h.to_lowercase() == "destination")
        .ok_or_else(|| "CSV missing required column: destination".to_string())?;
    let amount_col = headers
        .iter()
        .position(|h| h.to_lowercase() == "amount")
        .ok_or_else(|| "CSV missing required column: amount".to_string())?;
    let memo_col = headers
        .iter()
        .position(|h| h.to_lowercase() == "memo");

    let mut records = Vec::new();
    for line in lines {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let cols: Vec<&str> = line.splitn(headers.len(), ',').collect();
        let destination = cols.get(dest_col).copied().unwrap_or("").trim().to_string();
        let amount = cols.get(amount_col).copied().unwrap_or("").trim().to_string();
        let memo = memo_col
            .and_then(|i| cols.get(i).copied())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        records.push(PayoutRecord {
            destination,
            amount,
            memo,
        });
    }

    Ok(records)
}

/// Validate all records and split them into accepted/rejected sets.
fn split_valid(records: Vec<PayoutRecord>) -> (Vec<PayoutRecord>, Vec<String>) {
    let mut accepted = Vec::new();
    let mut errors = Vec::new();
    for (i, record) in records.into_iter().enumerate() {
        if let Some(err) = validate_record(i, &record) {
            errors.push(err);
        } else {
            accepted.push(record);
        }
    }
    (accepted, errors)
}

/// Persist accepted payouts as a new batch and return the generated batch ID.
async fn persist_batch(
    pool: &sqlx::PgPool,
    records: &[PayoutRecord],
) -> Result<String, sqlx::Error> {
    let total_amount: f64 = records
        .iter()
        .map(|r| r.amount.trim().replace(',', "").parse::<f64>().unwrap_or(0.0))
        .sum();
    let total_amount_i64 = (total_amount * 1_000_000.0).round() as i64;

    let batch_row = sqlx::query(
        r#"
        INSERT INTO payout_batches
            (currency, total_recipients, total_amount, status)
        VALUES ('XLM', $1, $2, 'PENDING')
        RETURNING id
        "#,
    )
    .bind(records.len() as i32)
    .bind(total_amount_i64)
    .fetch_one(pool)
    .await?;

    let batch_id: uuid::Uuid = batch_row.get("id");

    for record in records {
        let amount_i64 =
            (record.amount.trim().replace(',', "").parse::<f64>().unwrap_or(0.0) * 1_000_000.0)
                .round() as i64;
        sqlx::query(
            r#"
            INSERT INTO batch_recipients
                (batch_id, destination_address, amount, status)
            VALUES ($1, $2, $3, 'PENDING')
            "#,
        )
        .bind(&batch_id)
        .bind(&record.destination)
        .bind(amount_i64)
        .execute(pool)
        .await?;
    }

    Ok(batch_id.to_string())
}

/// POST `/api/payouts/batch-upload`
///
/// Accepts disbursement parameters via:
/// - **JSON body**: `{ "payouts": [{ "destination": "...", "amount": "...", "memo": "..." }] }`
/// - **Multipart form**: field named `file` containing a CSV with columns `destination,amount,memo`
///
/// Validates every record. Rejects the entire batch if the format is wrong. If individual
/// records are invalid, they are reported in the `errors` array while valid ones proceed.
/// Returns 422 if the payload is empty after validation.
pub async fn batch_upload(
    State(state): State<BridgeState>,
    content_type: axum::http::HeaderMap,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    // Determine payload format from Content-Type header.
    let ct = content_type
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let raw_records: Result<Vec<PayoutRecord>, String> = if ct.contains("multipart/form-data") {
        // Re-build a Multipart from raw bytes is non-trivial without the extractor.
        // Instead, we handle this via the dedicated multipart handler below.
        // This branch should not be reached when using `batch_upload_multipart`.
        Err("Use the multipart endpoint for CSV uploads".to_string())
    } else {
        // Assume JSON body
        match serde_json::from_slice::<BatchJsonPayload>(&body) {
            Ok(payload) => {
                if payload.payouts.is_empty() {
                    Err("payouts array must not be empty".to_string())
                } else {
                    Ok(payload.payouts)
                }
            }
            Err(e) => Err(format!("Invalid JSON payload: {}", e)),
        }
    };

    let records = match raw_records {
        Ok(r) => r,
        Err(msg) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": msg })),
            )
                .into_response();
        }
    };

    let (accepted, errors) = split_valid(records);

    if accepted.is_empty() {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(BatchUploadResponse {
                accepted: 0,
                rejected: errors.len(),
                errors,
                batch_id: None,
            }),
        )
            .into_response();
    }

    match persist_batch(&state.pool, &accepted).await {
        Ok(batch_id) => Json(BatchUploadResponse {
            accepted: accepted.len(),
            rejected: errors.len(),
            errors,
            batch_id: Some(batch_id),
        })
        .into_response(),
        Err(e) => {
            tracing::error!("Failed to persist batch upload: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to store batch" })),
            )
                .into_response()
        }
    }
}

/// POST `/api/payouts/batch-upload/csv`
///
/// Multipart form upload variant accepting a CSV file in a field named `file`.
/// Columns required: `destination`, `amount`. Column `memo` is optional.
pub async fn batch_upload_csv(
    State(state): State<BridgeState>,
    mut multipart: Multipart,
) -> impl IntoResponse {
    let mut csv_bytes: Option<Vec<u8>> = None;

    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or("").to_string();
        if name == "file" {
            match field.bytes().await {
                Ok(bytes) => {
                    csv_bytes = Some(bytes.to_vec());
                    break;
                }
                Err(e) => {
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(serde_json::json!({ "error": format!("Failed to read uploaded file: {}", e) })),
                    )
                        .into_response();
                }
            }
        }
    }

    let csv_data = match csv_bytes {
        Some(b) if !b.is_empty() => b,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "Multipart field 'file' is required and must not be empty" })),
            )
                .into_response();
        }
    };

    let records = match parse_csv(&csv_data) {
        Ok(r) => r,
        Err(msg) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": msg })),
            )
                .into_response();
        }
    };

    if records.is_empty() {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({ "error": "CSV contains no data rows" })),
        )
            .into_response();
    }

    let (accepted, errors) = split_valid(records);

    if accepted.is_empty() {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(BatchUploadResponse {
                accepted: 0,
                rejected: errors.len(),
                errors,
                batch_id: None,
            }),
        )
            .into_response();
    }

    match persist_batch(&state.pool, &accepted).await {
        Ok(batch_id) => Json(BatchUploadResponse {
            accepted: accepted.len(),
            rejected: errors.len(),
            errors,
            batch_id: Some(batch_id),
        })
        .into_response(),
        Err(e) => {
            tracing::error!("Failed to persist CSV batch upload: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to store batch" })),
            )
                .into_response()
        }
    }
}
