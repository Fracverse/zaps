use serde_json::Value;

pub struct YieldDepositedEvent {
    pub address: String,
    pub amount: i64,
    pub tx_hash: String,
}

pub struct YieldWithdrawnEvent {
    pub address: String,
    pub amount: i64,
    pub tx_hash: String,
}

pub struct YieldRateUpdatedEvent {
    pub apy: i32,
    pub tx_hash: String,
}

pub enum ZapsEvent {
    YieldDeposited(YieldDepositedEvent),
    YieldWithdrawn(YieldWithdrawnEvent),
    YieldRateUpdated(YieldRateUpdatedEvent),
    Unknown,
}

pub fn parse_zaps_event(topic: &str, value: &Value) -> ZapsEvent {
    match topic {
        "YieldDeposited" => {
            let address = find_nested_string(value, "address").unwrap_or_default();
            let amount = find_nested_i64(value, "amount").unwrap_or_default();
            let tx_hash = extract_tx_hash(value);

            ZapsEvent::YieldDeposited(YieldDepositedEvent {
                address,
                amount,
                tx_hash,
            })
        }
        "YieldWithdrawn" => {
            let address = find_nested_string(value, "address").unwrap_or_default();
            let amount = find_nested_i64(value, "amount").unwrap_or_default();
            let tx_hash = extract_tx_hash(value);

            ZapsEvent::YieldWithdrawn(YieldWithdrawnEvent {
                address,
                amount,
                tx_hash,
            })
        }
        "YieldRateUpdated" => {
            let apy = find_nested_i64(value, "apy").unwrap_or_default() as i32;
            let tx_hash = extract_tx_hash(value);

            ZapsEvent::YieldRateUpdated(YieldRateUpdatedEvent { apy, tx_hash })
        }
        _ => ZapsEvent::Unknown,
    }
}

pub fn find_nested_string(value: &Value, key: &str) -> Option<String> {
    match value {
        Value::Object(map) => map
            .get(key)
            .and_then(|item| match item {
                Value::String(text) => Some(text.clone()),
                Value::Number(number) => Some(number.to_string()),
                _ => None,
            })
            .or_else(|| {
                map.values()
                    .find_map(|nested| find_nested_string(nested, key))
            }),
        Value::Array(items) => items.iter().find_map(|item| find_nested_string(item, key)),
        _ => None,
    }
}

pub fn find_nested_i64(value: &Value, key: &str) -> Option<i64> {
    match value {
        Value::Object(map) => map
            .get(key)
            .and_then(|item| match item {
                Value::Number(number) => number.as_i64(),
                Value::String(text) => text.parse::<i64>().ok(),
                _ => None,
            })
            .or_else(|| map.values().find_map(|nested| find_nested_i64(nested, key))),
        Value::Array(items) => items.iter().find_map(|item| find_nested_i64(item, key)),
        _ => None,
    }
}

pub fn extract_tx_hash(value: &Value) -> String {
    find_nested_string(value, "tx_hash")
        .or_else(|| find_nested_string(value, "txHash"))
        .or_else(|| find_nested_string(value, "transactionHash"))
        .unwrap_or_else(|| "unknown".to_string())
}
