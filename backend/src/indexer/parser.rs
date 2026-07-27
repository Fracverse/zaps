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

/// SC-024 / BE-061: emitted when the vault compounds interest and publishes a
/// new yield index. Payload: `(elapsed_ledgers, added_yield, new_index)`.
pub struct YieldAccruedEvent {
    pub elapsed_ledgers: i64,
    pub added_yield: i64,
    pub new_index: i64,
    pub tx_hash: String,
}

/// BE-049: emitted when an admin salvages stranded tokens from a contract.
pub struct TokenSalvagedEvent {
    pub salvager: String,
    pub token: String,
    pub recipient: String,
    pub amount: i64,
    pub tx_hash: String,
}

/// #542: emitted by the user_registry contract's `register_user` when a
/// Stellar address claims a Zaps username on-chain.
pub struct UserRegisteredEvent {
    pub address: String,
    pub username: String,
    pub tx_hash: String,
}

/// BE-047: emitted by the social_graph contract when one user adds another
/// as a friend.
pub struct FriendAddedEvent {
    pub requester: String,
    pub friend: String,
    pub tx_hash: String,
}

/// BE-047: emitted by the social_graph contract when one user removes a
/// friend.
pub struct FriendRemovedEvent {
    pub user: String,
    pub friend: String,
    pub tx_hash: String,
}

pub enum ZapsEvent {
    YieldDeposited(YieldDepositedEvent),
    YieldWithdrawn(YieldWithdrawnEvent),
    YieldRateUpdated(YieldRateUpdatedEvent),
    YieldAccrued(YieldAccruedEvent),
    TokenSalvaged(TokenSalvagedEvent),
    UserRegistered(UserRegisteredEvent),
    FriendAdded(FriendAddedEvent),
    FriendRemoved(FriendRemovedEvent),
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
        "YieldAccrued" => {
            let elapsed_ledgers = find_nested_i64(value, "elapsed_ledgers")
                .or_else(|| find_nested_i64(value, "elapsed"))
                .unwrap_or_default();
            let added_yield = find_nested_i64(value, "added_yield").unwrap_or_default();
            let new_index = find_nested_i64(value, "new_index")
                .or_else(|| find_nested_i64(value, "index"))
                .unwrap_or_default();
            let tx_hash = extract_tx_hash(value);

            ZapsEvent::YieldAccrued(YieldAccruedEvent {
                elapsed_ledgers,
                added_yield,
                new_index,
                tx_hash,
            })
        }
        "TokenSalvaged" => {
            let salvager = find_nested_string(value, "salvager").unwrap_or_default();
            let token = find_nested_string(value, "token").unwrap_or_default();
            let recipient = find_nested_string(value, "recipient").unwrap_or_default();
            let amount = find_nested_i64(value, "amount").unwrap_or_default();
            let tx_hash = extract_tx_hash(value);

            ZapsEvent::TokenSalvaged(TokenSalvagedEvent {
                salvager,
                token,
                recipient,
                amount,
                tx_hash,
            })
        }
        // BE-047: Friendship events carry two addresses in a `data.vec` array.
        "FriendAdded" => {
            let (a1, a2) = extract_vec_address_pair(value);
            let tx_hash = extract_tx_hash(value);
            ZapsEvent::FriendAdded(FriendAddedEvent {
                requester: a1.unwrap_or_default(),
                friend: a2.unwrap_or_default(),
                tx_hash,
            })
        }
        "FriendRemoved" => {
            let (a1, a2) = extract_vec_address_pair(value);
            let tx_hash = extract_tx_hash(value);
            ZapsEvent::FriendRemoved(FriendRemovedEvent {
                user: a1.unwrap_or_default(),
                friend: a2.unwrap_or_default(),
                tx_hash,
            })
        }
        // #542: address + username come straight off the UserRegistered event
        // payload XDR (decoded to JSON upstream), same as every other event here.
        "UserRegistered" => {
            let address = find_nested_string(value, "address").unwrap_or_default();
            let username = find_nested_string(value, "username").unwrap_or_default();
            let tx_hash = extract_tx_hash(value);

            ZapsEvent::UserRegistered(UserRegisteredEvent {
                address,
                username,
                tx_hash,
            })
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

/// BE-047: Extract two addresses from a Soroban event's `body.v0.data.vec`
/// array where each element is `{ "address": "C..." }`. Returns `(None, None)`
/// when the structure does not match.
pub fn extract_vec_address_pair(value: &Value) -> (Option<String>, Option<String>) {
    let Some(vec) = value
        .get("body")
        .and_then(|body| body.get("v0"))
        .and_then(|v0| v0.get("data"))
        .and_then(|data| data.get("vec"))
        .and_then(Value::as_array)
    else {
        return (None, None);
    };

    let a1 = vec
        .first()
        .and_then(|v| v.get("address"))
        .and_then(Value::as_str)
        .map(String::from);

    let a2 = vec
        .get(1)
        .and_then(|v| v.get("address"))
        .and_then(Value::as_str)
        .map(String::from);

    (a1, a2)
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
    fn parses_yield_accrued_payload() {
        let payload = serde_json::json!({
            "value": {
                "elapsed_ledgers": 120,
                "added_yield": 45_000,
                "new_index": 1_004_500,
                "tx_hash": "accrue123"
            }
        });

        match parse_zaps_event("YieldAccrued", &payload) {
            ZapsEvent::YieldAccrued(event) => {
                assert_eq!(event.elapsed_ledgers, 120);
                assert_eq!(event.added_yield, 45_000);
                assert_eq!(event.new_index, 1_004_500);
                assert_eq!(event.tx_hash, "accrue123");
            }
            _ => panic!("expected a YieldAccrued event"),
        }
    }

    #[test]
    fn extract_event_topic_returns_none_for_missing_topic() {
        let event = serde_json::json!({ "other_field": "value" });
        assert_eq!(extract_event_topic(&event), None);
    }

    #[test]
    fn parses_friend_added_event() {
        let payload = serde_json::json!({
            "body": {
                "v0": {
                    "data": {
                        "vec": [
                            { "address": "CAFCT4" },
                            { "address": "CAHK3M" }
                        ]
                    }
                }
            },
            "txHash": "friend_tx_123"
        });

        match parse_zaps_event("FriendAdded", &payload) {
            ZapsEvent::FriendAdded(event) => {
                assert_eq!(event.requester, "CAFCT4");
                assert_eq!(event.friend, "CAHK3M");
                assert_eq!(event.tx_hash, "friend_tx_123");
            }
            _ => panic!("expected a FriendAdded event"),
        }
    }

    #[test]
    fn parses_friend_removed_event() {
        let payload = serde_json::json!({
            "body": {
                "v0": {
                    "data": {
                        "vec": [
                            { "address": "GUSER" },
                            { "address": "GFRIEND" }
                        ]
                    }
                }
            },
            "txHash": "remove_tx_456"
        });

        match parse_zaps_event("FriendRemoved", &payload) {
            ZapsEvent::FriendRemoved(event) => {
                assert_eq!(event.user, "GUSER");
                assert_eq!(event.friend, "GFRIEND");
                assert_eq!(event.tx_hash, "remove_tx_456");
            }
            _ => panic!("expected a FriendRemoved event"),
        }
    }

    #[test]
    fn extract_vec_address_pair_returns_none_for_missing_data() {
        let payload = serde_json::json!({ "no_body": true });
        let (a1, a2) = extract_vec_address_pair(&payload);
        assert!(a1.is_none());
        assert!(a2.is_none());
    }
}
