use axum::{
    extract::{Extension, Query, State},
    response::IntoResponse,
    Json,
};
use chrono::{NaiveDate, NaiveDateTime};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use super::auth_middleware::AuthenticatedUser;

#[derive(Deserialize)]
pub struct HistoryQuery {
    pub cursor: Option<String>,
    pub limit: Option<usize>,
    #[serde(rename = "type")]
    pub tx_type: Option<String>,
    pub status: Option<String>,
    pub asset: Option<String>,
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub amount_min: Option<f64>,
    pub amount_max: Option<f64>,
    pub search: Option<String>,
    pub yield_type: Option<String>,
}

#[derive(Serialize)]
pub struct HistoryItem {
    pub id: String,
    #[serde(rename = "type")]
    pub tx_type: String,
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
    pub yield_type: Option<String>,
    pub sender_name: Option<String>,
    pub sender_anchor: Option<String>,
}

#[derive(Serialize)]
pub struct HistoryResponse {
    pub items: Vec<HistoryItem>,
    pub next_cursor: Option<String>,
    pub total: usize,
}

struct RowItem {
    id: Uuid,
    tx_type: String,
    status: String,
    amount: i64,
    asset: String,
    address: String,
    address_label: Option<String>,
    created_at: NaiveDateTime,
    tx_hash: Option<String>,
    memo: Option<String>,
    yield_type: Option<String>,
    sender_name: Option<String>,
    sender_anchor: Option<String>,
}

fn date_bound(value: Option<&str>, end: bool) -> Option<NaiveDateTime> {
    let value = value?;
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|date| date.naive_utc())
        .or_else(|| {
            NaiveDate::parse_from_str(value, "%Y-%m-%d")
                .ok()
                .map(|date| {
                    if end {
                        date.and_hms_opt(23, 59, 59).unwrap()
                    } else {
                        date.and_hms_opt(0, 0, 0).unwrap()
                    }
                })
        })
}

fn amount_string(amount: i64) -> String {
    let value = amount as f64 / 1_000_000.0;
    format!("{value:.6}").trim_end_matches('0').trim_end_matches('.').to_owned()
}

fn map_status(value: &str) -> &'static str {
    match value.to_ascii_uppercase().as_str() {
        "FAILED" => "failed",
        "PENDING" | "SUBMITTED" | "PROCESSING" => "pending",
        _ => "completed",
    }
}

fn matches_filters(row: &RowItem, query: &HistoryQuery) -> bool {
    let tx_type = query.tx_type.as_deref().filter(|v| *v != "all");
    let status = query.status.as_deref().filter(|v| *v != "all");
    if tx_type.is_some_and(|value| row.tx_type != value) {
        return false;
    }
    if status.is_some_and(|value| row.status != value) {
        return false;
    }
    if query.yield_type.as_deref().is_some_and(|value| {
        row.tx_type != "yield" || row.yield_type.as_deref() != Some(value)
    }) {
        return false;
    }
    if query.asset.as_deref().is_some_and(|value| {
        !row.asset.eq_ignore_ascii_case(value)
    }) {
        return false;
    }
    if date_bound(query.date_from.as_deref(), false).is_some_and(|value| row.created_at < value) {
        return false;
    }
    if date_bound(query.date_to.as_deref(), true).is_some_and(|value| row.created_at > value) {
        return false;
    }
    let amount = row.amount as f64 / 1_000_000.0;
    if query.amount_min.is_some_and(|value| amount < value)
        || query.amount_max.is_some_and(|value| amount > value)
    {
        return false;
    }
    if let Some(search) = query.search.as_deref().filter(|value| !value.is_empty()) {
        let search = search.to_lowercase();
        let fields = [
            row.address.as_str(),
            row.address_label.as_deref().unwrap_or_default(),
            row.memo.as_deref().unwrap_or_default(),
            row.tx_hash.as_deref().unwrap_or_default(),
            row.asset.as_str(),
            row.sender_name.as_deref().unwrap_or_default(),
        ];
        if !fields.iter().any(|field| field.to_lowercase().contains(&search)) {
            return false;
        }
    }
    true
}

async fn user_rows(pool: &PgPool, user: &AuthenticatedUser) -> Result<Vec<RowItem>, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT p.id,
               CASE WHEN p.sender_id = $1 THEN 'sent' ELSE 'received' END AS tx_type,
               'completed'::text AS status,
               p.amount,
               p.currency AS asset,
               CASE WHEN p.sender_id = $1 THEN receiver.address ELSE sender.address END AS address,
               CASE WHEN p.sender_id = $1 THEN receiver.username ELSE sender.username END AS address_label,
               p.created_at,
               p.tx_hash,
               NULLIF(p.memo, '') AS memo,
               NULL::text AS yield_type,
               NULL::text AS sender_name,
               NULL::text AS sender_anchor
        FROM payments p
        JOIN users sender ON sender.id = p.sender_id
        JOIN users receiver ON receiver.id = p.receiver_id
        WHERE p.sender_id = $1 OR p.receiver_id = $1
        UNION ALL
        SELECT y.id,
               'yield'::text AS tx_type,
               'completed'::text AS status,
               y.amount,
               'USDC'::text AS asset,
               'yield.zaps'::text AS address,
               'Zaps Yield'::text AS address_label,
               y.created_at,
               y.tx_hash,
               y.type AS memo,
               CASE UPPER(y.type)
                   WHEN 'EARNED' THEN 'interest'
                   WHEN 'REWARD' THEN 'reward'
                   ELSE NULL
               END::text AS yield_type,
               NULL::text AS sender_name,
               NULL::text AS sender_anchor
        FROM yield_transactions y
        WHERE y.user_id = $1
        UNION ALL
        SELECT r.id,
               'mass_payout'::text AS tx_type,
               r.status,
               r.amount,
               b.currency AS asset,
               COALESCE(r.destination_address, recipient.address, '') AS address,
               recipient.username AS address_label,
               r.created_at,
               r.tx_hash,
               NULL::text AS memo,
               NULL::text AS yield_type,
               'Batch payout'::text AS sender_name,
               NULL::text AS sender_anchor
        FROM batch_recipients r
        JOIN payout_batches b ON b.id = r.batch_id
        LEFT JOIN users recipient ON recipient.id = r.user_id
        WHERE b.created_by = $1
        "#,
    )
    .bind(user.id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|row| {
            let amount: i64 = row.get("amount");
            let asset: String = row.get("asset");
            let raw_status: String = row.get("status");
            RowItem {
                id: row.get("id"),
                tx_type: row.get("tx_type"),
                status: map_status(&raw_status).to_owned(),
                amount,
                asset,
                address: row.get("address"),
                address_label: row.get("address_label"),
                created_at: row.get("created_at"),
                tx_hash: row.get("tx_hash"),
                memo: row.get("memo"),
                yield_type: row.get("yield_type"),
                sender_name: row.get("sender_name"),
                sender_anchor: row.get("sender_anchor"),
            }
        })
        .collect())
}

fn into_history_item(row: RowItem) -> HistoryItem {
    let amount = amount_string(row.amount);
    let fiat_currency = if row.asset == "USDC" {
        "USD".to_owned()
    } else {
        row.asset.clone()
    };
    HistoryItem {
        id: row.id.to_string(),
        tx_type: row.tx_type,
        status: row.status,
        fiat_value: amount.clone(),
        fiat_currency,
        amount,
        asset: row.asset,
        address: row.address,
        address_label: row.address_label,
        timestamp: row.created_at.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        stellar_tx_hash: row.tx_hash,
        memo: row.memo,
        yield_type: row.yield_type,
        sender_name: row.sender_name,
        sender_anchor: row.sender_anchor,
    }
}

pub async fn get_history(
    State(pool): State<PgPool>,
    Extension(user): Extension<AuthenticatedUser>,
    Query(query): Query<HistoryQuery>,
) -> impl IntoResponse {
    let mut rows = match user_rows(&pool, &user).await {
        Ok(rows) => rows,
        Err(error) => {
            tracing::error!(%error, "Failed to fetch transaction history");
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to retrieve transaction history" })),
            )
                .into_response();
        }
    };
    rows.retain(|row| matches_filters(row, &query));
    rows.sort_by(|a, b| b.created_at.cmp(&a.created_at).then_with(|| b.id.cmp(&a.id)));
    let total = rows.len();
    let start = query
        .cursor
        .as_deref()
        .and_then(|cursor| rows.iter().position(|row| row.id.to_string() == cursor))
        .map_or(0, |index| index + 1);
    let limit = query.limit.unwrap_or(20).clamp(1, 100);
    let has_more = start.saturating_add(limit) < total;
    let page: Vec<_> = rows.into_iter().skip(start).take(limit).collect();
    let next_cursor = if has_more {
        page.last().map(|row| row.id.to_string())
    } else {
        None
    };
    Json(HistoryResponse {
        items: page.into_iter().map(into_history_item).collect(),
        next_cursor,
        total,
    })
    .into_response()
}

pub async fn get_history_detail(
    State(pool): State<PgPool>,
    Extension(user): Extension<AuthenticatedUser>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> impl IntoResponse {
    let query = HistoryQuery {
        cursor: None,
        limit: Some(100),
        tx_type: None,
        status: None,
        asset: None,
        date_from: None,
        date_to: None,
        amount_min: None,
        amount_max: None,
        search: None,
        yield_type: None,
    };
    let id = match Uuid::parse_str(&id) {
        Ok(id) => id,
        Err(_) => {
            return (
                axum::http::StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "Transaction not found" })),
            )
                .into_response();
        }
    };
    match user_rows(&pool, &user).await {
        Ok(rows) => rows
            .into_iter()
            .find(|row| row.id == id && matches_filters(row, &query))
            .map(|row| Json(into_history_item(row)).into_response())
            .unwrap_or_else(|| {
                (
                    axum::http::StatusCode::NOT_FOUND,
                    Json(serde_json::json!({ "error": "Transaction not found" })),
                )
                    .into_response()
            }),
        Err(error) => {
            tracing::error!(%error, "Failed to fetch transaction detail");
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to retrieve transaction" })),
            )
                .into_response()
        }
    }
}
