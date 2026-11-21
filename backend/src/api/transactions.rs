use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Extension, Json,
};
use chrono::{DateTime, NaiveDateTime};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use super::AuthenticatedUser;

#[derive(Debug, Deserialize, Default)]
pub struct TransactionQuery {
    pub cursor: Option<String>,
    pub r#type: Option<String>,
    pub yield_type: Option<String>,
    pub status: Option<String>,
    pub asset: Option<String>,
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub amount_min: Option<String>,
    pub amount_max: Option<String>,
    pub search: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TransactionResponse {
    pub id: String,
    pub r#type: String,
    pub status: String,
    pub amount: String,
    pub asset: String,
    pub fiat_value: String,
    pub fiat_currency: String,
    pub address: String,
    pub address_label: Option<String>,
    pub timestamp: String,
    pub stellar_tx_hash: Option<String>,
    pub memo: Option<String>,
    pub fee: Option<String>,
    pub fee_asset: Option<String>,
    pub network: String,
    pub yield_type: Option<String>,
    pub sender_name: Option<String>,
    pub sender_anchor: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TransactionListResponse {
    pub items: Vec<TransactionResponse>,
    pub next_cursor: Option<String>,
    pub total: usize,
}

fn micro_units(amount: i64) -> String {
    let whole = amount / 1_000_000;
    let fraction = (amount.abs() % 1_000_000) as i64;
    if fraction == 0 {
        whole.to_string()
    } else {
        format!(
            "{}.{}",
            whole,
            format!("{fraction:06}").trim_end_matches('0')
        )
    }
}

fn matches_text(item: &TransactionResponse, search: &str) -> bool {
    let needle = search.to_lowercase();
    item.address.to_lowercase().contains(&needle)
        || item
            .address_label
            .as_deref()
            .unwrap_or("")
            .to_lowercase()
            .contains(&needle)
        || item
            .memo
            .as_deref()
            .unwrap_or("")
            .to_lowercase()
            .contains(&needle)
        || item
            .stellar_tx_hash
            .as_deref()
            .unwrap_or("")
            .to_lowercase()
            .contains(&needle)
        || item.asset.to_lowercase().contains(&needle)
}

fn parse_cursor(cursor: &str) -> Option<NaiveDateTime> {
    DateTime::parse_from_rfc3339(cursor)
        .ok()
        .map(|dt| dt.naive_utc())
        .or_else(|| NaiveDateTime::parse_from_str(cursor, "%Y-%m-%d %H:%M:%S%.f").ok())
}

fn in_amount_range(amount: &str, min: Option<&str>, max: Option<&str>) -> bool {
    let value = amount.parse::<f64>().ok();
    let Some(value) = value else { return true };
    if let Some(min) = min.and_then(|v| v.parse::<f64>().ok()) {
        if value < min {
            return false;
        }
    }
    if let Some(max) = max.and_then(|v| v.parse::<f64>().ok()) {
        if value > max {
            return false;
        }
    }
    true
}

fn apply_filters(
    mut items: Vec<TransactionResponse>,
    query: &TransactionQuery,
) -> Vec<TransactionResponse> {
    items.retain(|item| {
        if let Some(kind) = &query.r#type {
            if kind != "all" && item.r#type != *kind {
                return false;
            }
        }
        if let Some(yield_type) = &query.yield_type {
            if item.yield_type.as_deref() != Some(yield_type.as_str()) {
                return false;
            }
        }
        if let Some(status) = &query.status {
            if status != "all" && item.status != *status {
                return false;
            }
        }
        if let Some(asset) = &query.asset {
            if !item.asset.eq_ignore_ascii_case(asset) {
                return false;
            }
        }
        if let Some(search) = &query.search {
            if !matches_text(item, search) {
                return false;
            }
        }
        if !in_amount_range(
            &item.amount,
            query.amount_min.as_deref(),
            query.amount_max.as_deref(),
        ) {
            return false;
        }
        if let Some(from) = &query.date_from {
            if let (Ok(date), Ok(from)) = (
                NaiveDateTime::parse_from_str(&item.timestamp, "%Y-%m-%dT%H:%M:%S%.fZ"),
                chrono::NaiveDate::parse_from_str(from, "%Y-%m-%d"),
            ) {
                if date.date() < from {
                    return false;
                }
            }
        }
        if let Some(to) = &query.date_to {
            if let (Ok(date), Ok(to)) = (
                NaiveDateTime::parse_from_str(&item.timestamp, "%Y-%m-%dT%H:%M:%S%.fZ"),
                chrono::NaiveDate::parse_from_str(to, "%Y-%m-%d"),
            ) {
                if date.date() > to {
                    return false;
                }
            }
        }
        true
    });
    items.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    items
}

pub async fn list_transactions(
    State(pool): State<PgPool>,
    Extension(user): Extension<AuthenticatedUser>,
    Query(query): Query<TransactionQuery>,
) -> Response {
    let cursor = query.cursor.as_deref().and_then(parse_cursor);
    let mut items = Vec::new();

    let payment_rows = sqlx::query(
        r#"SELECT p.id, p.tx_hash, p.amount, p.currency, p.memo, p.created_at,
                  sender.address AS sender_address, sender.username AS sender_username,
                  receiver.address AS receiver_address, receiver.username AS receiver_username
           FROM payments p
           JOIN users sender ON p.sender_id = sender.id
           JOIN users receiver ON p.receiver_id = receiver.id
           WHERE (p.sender_id = $1 OR p.receiver_id = $1)
             AND ($2::timestamp IS NULL OR p.created_at < $2)"#,
    )
    .bind(user.id)
    .bind(cursor)
    .fetch_all(&pool)
    .await;

    if let Ok(rows) = payment_rows {
        for row in rows {
            let sender: String = row.get("sender_address");
            let receiver: String = row.get("receiver_address");
            let outgoing = sender == user.address;
            let address = if outgoing { receiver } else { sender };
            let address_label = if outgoing {
                row.get::<String, _>("receiver_username")
            } else {
                row.get::<String, _>("sender_username")
            };
            let amount: i64 = row.get("amount");
            let currency: String = row.get("currency");
            let created: NaiveDateTime = row.get("created_at");
            items.push(TransactionResponse {
                id: row.get::<Uuid, _>("id").to_string(),
                r#type: if outgoing { "sent" } else { "received" }.to_string(),
                status: "completed".to_string(),
                amount: micro_units(amount),
                asset: currency,
                fiat_value: micro_units(amount),
                fiat_currency: "USD".to_string(),
                address,
                address_label: Some(address_label),
                timestamp: DateTime::<chrono::Utc>::from_naive_utc_and_offset(created, chrono::Utc)
                    .to_rfc3339(),
                stellar_tx_hash: Some(row.get::<String, _>("tx_hash")),
                memo: Some(row.get::<String, _>("memo")),
                fee: None,
                fee_asset: None,
                network: "Stellar".to_string(),
                yield_type: None,
                sender_name: None,
                sender_anchor: None,
            });
        }
    }

    let yield_rows = sqlx::query(
        r#"SELECT id, tx_hash, type, amount, created_at
           FROM yield_transactions
           WHERE user_id = $1
             AND ($2::timestamp IS NULL OR created_at < $2)"#,
    )
    .bind(user.id)
    .bind(cursor)
    .fetch_all(&pool)
    .await;

    if let Ok(rows) = yield_rows {
        for row in rows {
            let kind: String = row.get("type");
            let amount: i64 = row.get("amount");
            let created: NaiveDateTime = row.get("created_at");
            let yield_type = match kind.as_str() {
                "REWARD" => Some("reward"),
                "APY" => Some("apy"),
                "EARNED" => Some("interest"),
                _ => None,
            };
            items.push(TransactionResponse {
                id: row.get::<Uuid, _>("id").to_string(),
                r#type: "yield".to_string(),
                status: "completed".to_string(),
                amount: micro_units(amount),
                asset: "USDC".to_string(),
                fiat_value: micro_units(amount),
                fiat_currency: "USD".to_string(),
                address: "yield.zaps".to_string(),
                address_label: Some(
                    match kind.as_str() {
                        "DEPOSIT" => "Yield deposit",
                        "WITHDRAW" => "Yield withdrawal",
                        "REWARD" => "Yield reward",
                        "APY" => "APY payout",
                        _ => "Yield earning",
                    }
                    .to_string(),
                ),
                timestamp: DateTime::<chrono::Utc>::from_naive_utc_and_offset(created, chrono::Utc)
                    .to_rfc3339(),
                stellar_tx_hash: row.try_get("tx_hash").ok(),
                memo: Some(format!("{} yield transaction", kind)),
                fee: None,
                fee_asset: None,
                network: "Stellar".to_string(),
                yield_type: yield_type.map(str::to_string),
                sender_name: None,
                sender_anchor: None,
            });
        }
    }

    let payout_rows = sqlx::query(
        r#"SELECT r.id, r.amount, r.status, r.tx_hash, r.created_at,
                  r.destination_address, b.currency
           FROM batch_recipients r
           JOIN payout_batches b ON r.batch_id = b.id
           WHERE r.user_id = $1
             AND ($2::timestamp IS NULL OR r.created_at < $2)"#,
    )
    .bind(user.id)
    .bind(cursor)
    .fetch_all(&pool)
    .await;

    if let Ok(rows) = payout_rows {
        for row in rows {
            let amount: i64 = row.get("amount");
            let status: String = row.get("status");
            let currency: String = row.get("currency");
            let created: NaiveDateTime = row.get("created_at");
            items.push(TransactionResponse {
                id: row.get::<Uuid, _>("id").to_string(),
                r#type: "mass_payout".to_string(),
                status: match status.as_str() {
                    "FAILED" => "failed",
                    "CONFIRMED" => "completed",
                    _ => "pending",
                }
                .to_string(),
                amount: micro_units(amount),
                asset: currency,
                fiat_value: micro_units(amount),
                fiat_currency: "USD".to_string(),
                address: row
                    .try_get::<String, _>("destination_address")
                    .unwrap_or_else(|_| user.address.clone()),
                address_label: Some("SDP Disbursement".to_string()),
                timestamp: DateTime::<chrono::Utc>::from_naive_utc_and_offset(created, chrono::Utc)
                    .to_rfc3339(),
                stellar_tx_hash: row.try_get("tx_hash").ok(),
                memo: Some("Stellar Disbursement Platform payout".to_string()),
                fee: None,
                fee_asset: None,
                network: "Stellar".to_string(),
                yield_type: None,
                sender_name: Some("Stellar Disbursement Platform".to_string()),
                sender_anchor: None,
            });
        }
    }

    items = apply_filters(items, &query);
    items.truncate(20);
    let next_cursor = if items.len() == 20 {
        items
            .last()
            .and_then(|item| DateTime::parse_from_rfc3339(&item.timestamp).ok())
            .map(|dt| dt.to_rfc3339())
    } else {
        None
    };

    let total = items.len();
    Json(TransactionListResponse {
        items,
        next_cursor,
        total,
    })
    .into_response()
}

pub async fn get_transaction(
    State(pool): State<PgPool>,
    Extension(user): Extension<AuthenticatedUser>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Response {
    let Ok(uuid) = Uuid::parse_str(&id) else {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Transaction not found" })),
        )
            .into_response();
    };

    if let Ok(Some(row)) = sqlx::query(
        r#"SELECT p.id, p.tx_hash, p.amount, p.currency, p.memo, p.created_at,
                  sender.address AS sender_address, sender.username AS sender_username,
                  receiver.address AS receiver_address, receiver.username AS receiver_username
           FROM payments p
           JOIN users sender ON p.sender_id = sender.id
           JOIN users receiver ON p.receiver_id = receiver.id
           WHERE p.id = $1 AND (p.sender_id = $2 OR p.receiver_id = $2)"#,
    )
    .bind(uuid)
    .bind(user.id)
    .fetch_optional(&pool)
    .await
    {
        let sender: String = row.get("sender_address");
        let receiver: String = row.get("receiver_address");
        let outgoing = sender == user.address;
        let address = if outgoing { receiver } else { sender };
        let amount: i64 = row.get("amount");
        let currency: String = row.get("currency");
        let created: NaiveDateTime = row.get("created_at");
        return Json(serde_json::json!({ "transaction": TransactionResponse {
            id: row.get::<Uuid, _>("id").to_string(),
            r#type: if outgoing { "sent" } else { "received" }.to_string(),
            status: "completed".to_string(),
            amount: micro_units(amount),
            asset: currency.clone(),
            fiat_value: micro_units(amount),
            fiat_currency: "USD".to_string(),
            address,
            address_label: Some(if outgoing { row.get::<String, _>("receiver_username") } else { row.get::<String, _>("sender_username") }),
            timestamp: DateTime::<chrono::Utc>::from_naive_utc_and_offset(created, chrono::Utc).to_rfc3339(),
            stellar_tx_hash: Some(row.get::<String, _>("tx_hash")),
            memo: Some(row.get::<String, _>("memo")),
            fee: None, fee_asset: None, network: "Stellar".to_string(),
            yield_type: None, sender_name: None, sender_anchor: None,
        }})).into_response();
    }

    if let Ok(Some(row)) = sqlx::query(
        r#"SELECT id, tx_hash, type, amount, created_at FROM yield_transactions
           WHERE id = $1 AND user_id = $2"#,
    )
    .bind(uuid)
    .bind(user.id)
    .fetch_optional(&pool)
    .await
    {
        let kind: String = row.get("type");
        let amount: i64 = row.get("amount");
        let created: NaiveDateTime = row.get("created_at");
        let yield_type = match kind.as_str() {
            "REWARD" => "reward",
            "APY" => "apy",
            "EARNED" => "interest",
            _ => "",
        };
        return Json(serde_json::json!({ "transaction": TransactionResponse {
            id: row.get::<Uuid, _>("id").to_string(),
            r#type: "yield".to_string(),
            status: "completed".to_string(),
            amount: micro_units(amount),
            asset: "USDC".to_string(),
            fiat_value: micro_units(amount),
            fiat_currency: "USD".to_string(),
            address: "yield.zaps".to_string(),
            address_label: Some("Yield transaction".to_string()),
            timestamp: DateTime::<chrono::Utc>::from_naive_utc_and_offset(created, chrono::Utc).to_rfc3339(),
            stellar_tx_hash: row.try_get("tx_hash").ok(),
            memo: Some(format!("{} yield transaction", kind)),
            fee: None, fee_asset: None, network: "Stellar".to_string(),
            yield_type: if yield_type.is_empty() { None } else { Some(yield_type.to_string()) },
            sender_name: None, sender_anchor: None,
        }})).into_response();
    }

    if let Ok(Some(row)) = sqlx::query(
        r#"SELECT r.id, r.amount, r.status, r.tx_hash, r.created_at,
                  r.destination_address, b.currency
           FROM batch_recipients r
           JOIN payout_batches b ON r.batch_id = b.id
           WHERE r.id = $1 AND r.user_id = $2"#,
    )
    .bind(uuid)
    .bind(user.id)
    .fetch_optional(&pool)
    .await
    {
        let amount: i64 = row.get("amount");
        let status: String = row.get("status");
        let currency: String = row.get("currency");
        let created: NaiveDateTime = row.get("created_at");
        return Json(serde_json::json!({ "transaction": TransactionResponse {
            id: row.get::<Uuid, _>("id").to_string(),
            r#type: "mass_payout".to_string(),
            status: match status.as_str() {
                "FAILED" => "failed",
                "CONFIRMED" => "completed",
                _ => "pending",
            }.to_string(),
            amount: micro_units(amount),
            asset: currency,
            fiat_value: micro_units(amount),
            fiat_currency: "USD".to_string(),
            address: row.try_get::<String, _>("destination_address").unwrap_or_else(|_| user.address.clone()),
            address_label: Some("SDP Disbursement".to_string()),
            timestamp: DateTime::<chrono::Utc>::from_naive_utc_and_offset(created, chrono::Utc).to_rfc3339(),
            stellar_tx_hash: row.try_get("tx_hash").ok(),
            memo: Some("Stellar Disbursement Platform payout".to_string()),
            fee: None,
            fee_asset: None,
            network: "Stellar".to_string(),
            yield_type: None,
            sender_name: Some("Stellar Disbursement Platform".to_string()),
            sender_anchor: None,
        }})).into_response();
    }

    (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({ "error": "Transaction not found" })),
    )
        .into_response()
}
