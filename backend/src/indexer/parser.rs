use base64::{engine::general_purpose::STANDARD as B64, Engine};
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

/// BE-043: Decode a base64-encoded Soroban XDR ScVal and extract the symbol
/// string if the discriminant is SCV_SYMBOL (14).
///
/// XDR layout:
///   [4 bytes big-endian discriminant = 14] [4 bytes big-endian string length] [bytes] [padding]
pub fn decode_scval_symbol(xdr_b64: &str) -> Option<String> {
    let bytes = B64.decode(xdr_b64.trim()).ok()?;
    if bytes.len() < 8 {
        return None;
    }
    let discriminant = u32::from_be_bytes(bytes[0..4].try_into().ok()?);
    // SCV_SYMBOL = 14
    if discriminant != 14 {
        return None;
    }
    let len = u32::from_be_bytes(bytes[4..8].try_into().ok()?) as usize;
    if bytes.len() < 8 + len {
        return None;
    }
    String::from_utf8(bytes[8..8 + len].to_vec()).ok()
}

/// BE-043: Extract the first symbol topic from a Soroban RPC event's topics
/// array. Topics are base64-encoded XDR ScVal entries.
pub fn extract_event_topic(event: &Value) -> Option<String> {
    // Soroban RPC event structure: event["topic"] is an array of base64 XDR strings.
    let topics = event.get("topic").and_then(Value::as_array)?;
    for topic in topics {
        if let Some(xdr) = topic.as_str() {
            if let Some(sym) = decode_scval_symbol(xdr) {
                return Some(sym);
            }
        }
    }
    None
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_scval_symbol_yields_deposited() {
        // Encode "YieldDeposited" as SCV_SYMBOL XDR and verify round-trip decode.
        let name = b"YieldDeposited";
        let mut xdr = Vec::new();
        xdr.extend_from_slice(&14u32.to_be_bytes());
        xdr.extend_from_slice(&(name.len() as u32).to_be_bytes());
        xdr.extend_from_slice(name);
        let pad = (4 - name.len() % 4) % 4;
        xdr.resize(xdr.len() + pad, 0u8);
        let b64 = B64.encode(&xdr);
        assert_eq!(
            decode_scval_symbol(&b64),
            Some("YieldDeposited".to_string())
        );
    }

    #[test]
    fn decode_scval_symbol_rejects_non_symbol_type() {
        // Discriminant 6 = SCV_BOOL, not a symbol.
        let xdr = 6u32.to_be_bytes();
        let b64 = B64.encode(xdr);
        assert_eq!(decode_scval_symbol(&b64), None);
    }

    #[test]
    fn decode_scval_symbol_rejects_invalid_base64() {
        assert_eq!(decode_scval_symbol("!!!not_base64!!!"), None);
    }

    #[test]
    fn extract_event_topic_from_soroban_rpc_shape() {
        let name = b"YieldWithdrawn";
        let mut xdr = Vec::new();
        xdr.extend_from_slice(&14u32.to_be_bytes());
        xdr.extend_from_slice(&(name.len() as u32).to_be_bytes());
        xdr.extend_from_slice(name);
        let pad = (4 - name.len() % 4) % 4;
        xdr.resize(xdr.len() + pad, 0u8);
        let b64 = B64.encode(&xdr);

        let event = serde_json::json!({ "topic": [b64] });
        assert_eq!(
            extract_event_topic(&event),
            Some("YieldWithdrawn".to_string())
        );
    }

    #[test]
    fn extract_event_topic_returns_none_for_missing_topic() {
        let event = serde_json::json!({ "other_field": "value" });
        assert_eq!(extract_event_topic(&event), None);
    }
}
