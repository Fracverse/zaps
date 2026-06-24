use crate::services::allbridge::{
    AllbridgeClient, AllbridgeQuoteRequest, BridgeStatusKind, BridgeTransferStatus,
};
use axum::{
    extract::{Path, State},
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
                        update_status(&state.pool, &tx_hash, status.status.as_str(), new_conf).await;
                    }
                }
                Err(e) => {
                    tracing::warn!("Bridge poller: status poll failed for {}: {:?}", tx_hash, e);
                }
            }
        }
    }
}
