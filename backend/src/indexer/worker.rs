use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use std::{env, error::Error, time::Duration};
use uuid::Uuid;

const INDEXER_CURSOR_KEY: &str = "stellar_event_cursor";
const DEFAULT_POLL_INTERVAL: Duration = Duration::from_secs(3);
const INITIAL_BACKOFF: Duration = Duration::from_secs(1);
const MAX_BACKOFF: Duration = Duration::from_secs(30);
const RPC_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Deserialize, Debug)]
pub struct SocialPaymentEvent {
    pub sender: String,
    pub receiver: String,
    pub amount: i64,
    pub memo: String,
    pub visibility: String,
    pub tx_hash: String,
}

pub async fn run(
    pool: PgPool,
    rpc_url: String,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing::info!("Starting Stellar event indexer background worker...");

    let mut cursor = load_or_initialize_cursor(&pool).await?;
    let mut backoff_attempt = 0usize;

    loop {
        match poll_soroban_events(&rpc_url, cursor).await {
            Ok((events, latest_ledger)) => {
                backoff_attempt = 0;
                let mut next_cursor = cursor;

                for event in events {
                    if let Some(payment_event) = extract_social_payment_event(&event) {
                        if let Err(err) = process_social_payment_event(payment_event, &pool).await {
                            tracing::warn!("Failed to process Stellar payment event: {err}");
                        }
                    }

                    if let Some(ledger) = event.get("ledger").and_then(Value::as_u64) {
                        next_cursor = next_cursor.max(ledger as i64);
                    }
                }

                if latest_ledger > 0 {
                    next_cursor = next_cursor.max(latest_ledger as i64);
                }

                if next_cursor > cursor {
                    cursor = next_cursor;
                    persist_cursor(&pool, cursor).await?;
                    tracing::debug!("Indexer cursor advanced to ledger {cursor}");
                } else {
                    cursor += 1;
                    persist_cursor(&pool, cursor).await?;
                }

                tokio::time::sleep(DEFAULT_POLL_INTERVAL).await;
            }
            Err(err) => {
                let delay = compute_backoff_delay(backoff_attempt);
                backoff_attempt += 1;
                tracing::warn!("Soroban RPC polling failed, retrying in {:?}: {err}", delay);
                tokio::time::sleep(delay).await;
            }
        }
    }
}

pub async fn process_social_payment_event(
    event: SocialPaymentEvent,
    pool: &PgPool,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let sender_id = get_or_create_user_id(&event.sender, pool).await?;
    let receiver_id = get_or_create_user_id(&event.receiver, pool).await?;

    sqlx::query(
        r#"
        INSERT INTO payments (tx_hash, sender_id, receiver_id, amount, currency, memo, visibility)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        ON CONFLICT (tx_hash) DO NOTHING
        "#,
    )
    .bind(&event.tx_hash)
    .bind(sender_id)
    .bind(receiver_id)
    .bind(event.amount)
    .bind("NGN")
    .bind(&event.memo)
    .bind(event.visibility.to_uppercase())
    .execute(pool)
    .await?;

    Ok(())
}

async fn poll_soroban_events(
    rpc_url: &str,
    start_ledger: i64,
) -> Result<(Vec<Value>, u64), Box<dyn Error + Send + Sync>> {
    let contract_id = env::var("SOCIAL_PAYMENT_CONTRACT_ID").ok();
    let payload = build_get_events_payload(start_ledger, contract_id.as_deref());

    let response = reqwest::Client::new()
        .post(rpc_url)
        .timeout(RPC_TIMEOUT)
        .json(&payload)
        .send()
        .await?;

    let status = response.status();
    if !status.is_success() {
        return Err(format!("Soroban RPC returned HTTP {status}").into());
    }

    let body: Value = response.json().await?;
    let result = body
        .get("result")
        .ok_or("Soroban RPC response did not include result")?;
    let events = result
        .get("events")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let latest_ledger = result
        .get("latestLedger")
        .and_then(Value::as_u64)
        .unwrap_or(0);

    Ok((events, latest_ledger))
}

fn build_get_events_payload(start_ledger: i64, contract_id: Option<&str>) -> Value {
    let mut filters = vec![json!({
        "topics": [[{ "type": "symbol", "value": "SocialPaymentEvent" }]]
    })];

    if let Some(contract_id) = contract_id.filter(|value| !value.is_empty()) {
        let contract_filter = json!({
            "contractIds": [contract_id],
            "topics": [[{ "type": "symbol", "value": "SocialPaymentEvent" }]]
        });
        filters[0] = contract_filter;
    }

    json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "getEvents",
        "params": [{
            "startLedger": start_ledger,
            "filters": filters
        }]
    })
}

fn compute_backoff_delay(attempt: usize) -> Duration {
    let multiplier = 2usize.saturating_pow(attempt.min(5) as u32);
    let candidate = INITIAL_BACKOFF.saturating_mul(multiplier as u32);
    candidate.min(MAX_BACKOFF)
}

async fn load_or_initialize_cursor(pool: &PgPool) -> Result<i64, Box<dyn Error + Send + Sync>> {
    let existing = sqlx::query_scalar::<_, i64>(
        "SELECT last_ledger_sequence FROM indexer_state WHERE key = $1",
    )
    .bind(INDEXER_CURSOR_KEY)
    .fetch_optional(pool)
    .await?;

    if let Some(cursor) = existing {
        return Ok(cursor);
    }

    sqlx::query(
        "INSERT INTO indexer_state (key, last_ledger_sequence) VALUES ($1, $2) ON CONFLICT (key) DO NOTHING",
    )
    .bind(INDEXER_CURSOR_KEY)
    .bind(0_i64)
    .execute(pool)
    .await?;

    Ok(0)
}

async fn persist_cursor(pool: &PgPool, ledger: i64) -> Result<(), Box<dyn Error + Send + Sync>> {
    sqlx::query(
        "INSERT INTO indexer_state (key, last_ledger_sequence, updated_at) VALUES ($1, $2, NOW()) ON CONFLICT (key) DO UPDATE SET last_ledger_sequence = EXCLUDED.last_ledger_sequence, updated_at = NOW()",
    )
    .bind(INDEXER_CURSOR_KEY)
    .bind(ledger)
    .execute(pool)
    .await?;

    Ok(())
}

fn extract_social_payment_event(value: &Value) -> Option<SocialPaymentEvent> {
    let sender = find_nested_string(value, "sender")?;
    let receiver = find_nested_string(value, "receiver")?;
    let amount = find_nested_i64(value, "amount")?;
    let memo = find_nested_string(value, "memo").unwrap_or_else(|| "".to_string());
    let visibility = find_nested_string(value, "visibility").unwrap_or_else(|| "PUBLIC".to_string());
    let tx_hash = find_nested_string(value, "tx_hash")
        .or_else(|| find_nested_string(value, "txHash"))
        .or_else(|| find_nested_string(value, "transactionHash"))
        .unwrap_or_else(|| "unknown".to_string());

    Some(SocialPaymentEvent {
        sender,
        receiver,
        amount,
        memo,
        visibility,
        tx_hash,
    })
}

fn find_nested_string(value: &Value, key: &str) -> Option<String> {
    match value {
        Value::Object(map) => map.get(key).and_then(|item| match item {
            Value::String(text) => Some(text.clone()),
            Value::Number(number) => Some(number.to_string()),
            _ => None,
        }).or_else(|| map.values().find_map(|nested| find_nested_string(nested, key))),
        Value::Array(items) => items.iter().find_map(|item| find_nested_string(item, key)),
        _ => None,
    }
}

fn find_nested_i64(value: &Value, key: &str) -> Option<i64> {
    match value {
        Value::Object(map) => map.get(key).and_then(|item| match item {
            Value::Number(number) => number.as_i64(),
            Value::String(text) => text.parse::<i64>().ok(),
            _ => None,
        }).or_else(|| map.values().find_map(|nested| find_nested_i64(nested, key))),
        Value::Array(items) => items.iter().find_map(|item| find_nested_i64(item, key)),
        _ => None,
    }
}

async fn get_or_create_user_id(
    address: &str,
    pool: &PgPool,
) -> Result<Uuid, Box<dyn std::error::Error + Send + Sync>> {
    let username = slugify_address(address);
    let row = sqlx::query(
        r#"
        INSERT INTO users (address, username, display_name)
        VALUES ($1, $2, $3)
        ON CONFLICT (address)
        DO UPDATE SET username = COALESCE(users.username, EXCLUDED.username)
        RETURNING id
        "#,
    )
    .bind(address)
    .bind(&username)
    .bind(Some(&username))
    .fetch_one(pool)
    .await?;

    Ok(row.get("id"))
}

fn slugify_address(address: &str) -> String {
    let trimmed = address.trim();
    let snippet = trimmed.get(1..15).unwrap_or(trimmed);
    format!("u_{}", snippet.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_payload_with_contract_and_topic_filters() {
        let payload = build_get_events_payload(12, Some("CAKE"));
        let params = payload["params"].as_array().unwrap();
        let filter = &params[0]["filters"][0];

        assert_eq!(filter["contractIds"][0].as_str(), Some("CAKE"));
        assert_eq!(filter["topics"][0][0]["value"].as_str(), Some("SocialPaymentEvent"));
    }

    #[test]
    fn backoff_delay_grows_and_caps() {
        assert_eq!(compute_backoff_delay(0), INITIAL_BACKOFF);
        assert_eq!(compute_backoff_delay(1), Duration::from_secs(2));
        assert_eq!(compute_backoff_delay(6), MAX_BACKOFF);
    }

    #[test]
    fn extracts_payment_event_from_nested_payload() {
        let payload = json!({
            "body": {
                "v0": {
                    "data": {
                        "sender": "GABC",
                        "receiver": "GXYZ",
                        "amount": 2500,
                        "memo": "Lunch",
                        "visibility": "PUBLIC",
                        "tx_hash": "abc123"
                    }
                }
            }
        });

        let event = extract_social_payment_event(&payload).unwrap();
        assert_eq!(event.sender, "GABC");
        assert_eq!(event.receiver, "GXYZ");
        assert_eq!(event.amount, 2500);
        assert_eq!(event.memo, "Lunch");
    }
}
