//! #950: Live Soroban ledger event stream.
//!
//! [`SorobanEventStreamer`] polls Soroban RPC `getEvents`, decodes each event
//! with the same parser the indexer uses, and publishes it on a
//! `tokio::sync::broadcast` channel. [`ws_handler`] upgrades HTTP clients to
//! WebSockets and forwards every broadcast event to them as JSON text frames.
//!
//! This is a best-effort *live* feed, separate from the durable indexer in
//! `worker.rs`: it keeps no checkpoint in Postgres, and after a restart or a
//! cursor that has fallen out of the RPC retention window it resumes from the
//! latest ledger rather than replaying history.
//!
//! Slow consumers never block the producer: `broadcast::Sender::send` is
//! non-blocking and a receiver that falls more than the channel capacity
//! behind gets `RecvError::Lagged` and skips ahead. A WebSocket client that
//! can't accept a frame within `WS_SEND_TIMEOUT` is disconnected.

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::Response,
};
use serde::Serialize;
use serde_json::{json, Value};
use std::{env, error::Error, sync::Arc, time::Duration};
use tokio::sync::broadcast::{self, error::RecvError};

use super::parser::{
    extract_event_topic, extract_tx_hash, find_nested_i64, find_nested_string, parse_zaps_event,
    ZapsEvent,
};
use crate::services::stellar::StellarClient;

/// Events buffered per receiver before a slow receiver starts lagging.
pub const DEFAULT_CHANNEL_CAPACITY: usize = 1024;
const DEFAULT_POLL_INTERVAL: Duration = Duration::from_secs(5);
const INITIAL_BACKOFF: Duration = Duration::from_secs(1);
const MAX_BACKOFF: Duration = Duration::from_secs(30);
/// Page size requested from `getEvents`. A full page means more are waiting,
/// so the next page is fetched without sleeping.
const PAGE_LIMIT: usize = 100;
/// `getEvents` accepts at most 5 contract IDs per filter.
const MAX_CONTRACT_IDS_PER_FILTER: usize = 5;
const WS_SEND_TIMEOUT: Duration = Duration::from_secs(10);

/// Contracts whose events are streamed, read from the same env vars the
/// indexer uses.
const CONTRACT_ID_ENV_VARS: &[&str] = &[
    "SOCIAL_PAYMENT_CONTRACT_ID",
    "SOCIAL_GRAPH_CONTRACT_ID",
    "USER_REGISTRY_CONTRACT_ID",
    "YIELD_VAULT_CONTRACT_ID",
];

/// A decoded Soroban contract event as sent to WebSocket clients.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct LedgerEvent {
    pub id: Option<String>,
    pub ledger: Option<u64>,
    pub ledger_closed_at: Option<String>,
    pub contract_id: Option<String>,
    pub tx_hash: Option<String>,
    /// First symbol topic, e.g. `"UserRegistered"`.
    pub event_type: Option<String>,
    /// Decoded fields for known Zaps events; `null` for unrecognized ones.
    pub data: Value,
    /// The event exactly as returned by Soroban RPC.
    pub raw: Value,
}

impl LedgerEvent {
    /// Decode one entry of a `getEvents` response's `events` array.
    pub fn decode(raw: Value) -> Self {
        let event_type = extract_event_topic(&raw)
            .or_else(|| find_nested_string(&raw, "topic_symbol"))
            .or_else(|| find_nested_string(&raw, "event_type"));
        let data = event_type
            .as_deref()
            .map(|topic| decode_event_data(topic, &raw))
            .unwrap_or(Value::Null);
        let str_field = |key: &str| raw.get(key).and_then(Value::as_str).map(str::to_string);

        Self {
            id: str_field("id"),
            ledger: raw.get("ledger").and_then(Value::as_u64),
            ledger_closed_at: str_field("ledgerClosedAt"),
            contract_id: str_field("contractId"),
            tx_hash: str_field("txHash"),
            event_type,
            data,
            raw,
        }
    }
}

/// Map a parsed event to JSON. `SocialPaymentEvent` isn't a `ZapsEvent`
/// variant (the indexer extracts it separately), so it's decoded here with
/// the same field lookups.
fn decode_event_data(topic: &str, raw: &Value) -> Value {
    match parse_zaps_event(topic, raw) {
        ZapsEvent::YieldDeposited(e) => {
            json!({ "address": e.address, "amount": e.amount, "tx_hash": e.tx_hash })
        }
        ZapsEvent::YieldWithdrawn(e) => {
            json!({ "address": e.address, "amount": e.amount, "tx_hash": e.tx_hash })
        }
        ZapsEvent::YieldRateUpdated(e) => json!({ "apy": e.apy, "tx_hash": e.tx_hash }),
        ZapsEvent::YieldAccrued(e) => json!({
            "elapsed_ledgers": e.elapsed_ledgers,
            "added_yield": e.added_yield,
            "new_index": e.new_index,
            "tx_hash": e.tx_hash,
        }),
        ZapsEvent::TokenSalvaged(e) => json!({
            "salvager": e.salvager,
            "token": e.token,
            "recipient": e.recipient,
            "amount": e.amount,
            "tx_hash": e.tx_hash,
        }),
        ZapsEvent::UserRegistered(e) => {
            json!({ "address": e.address, "username": e.username, "tx_hash": e.tx_hash })
        }
        ZapsEvent::FriendAdded(e) => {
            json!({ "requester": e.requester, "friend": e.friend, "tx_hash": e.tx_hash })
        }
        ZapsEvent::FriendRemoved(e) => {
            json!({ "user": e.user, "friend": e.friend, "tx_hash": e.tx_hash })
        }
        ZapsEvent::Unknown if topic == "SocialPaymentEvent" => json!({
            "sender": find_nested_string(raw, "sender"),
            "receiver": find_nested_string(raw, "receiver"),
            "amount": find_nested_i64(raw, "amount"),
            "memo": find_nested_string(raw, "memo"),
            "visibility": find_nested_string(raw, "visibility")
                .unwrap_or_else(|| "PUBLIC".to_string()),
            "tx_hash": extract_tx_hash(raw),
        }),
        ZapsEvent::Unknown => Value::Null,
    }
}

/// Fan-out point between the streamer and WebSocket sessions. Cheap to clone.
#[derive(Clone)]
pub struct EventBroadcaster {
    sender: broadcast::Sender<Arc<LedgerEvent>>,
}

impl EventBroadcaster {
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity.max(1));
        Self { sender }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Arc<LedgerEvent>> {
        self.sender.subscribe()
    }

    /// Publish to every current subscriber without blocking. Returns how many
    /// subscribers will see it; zero (nobody connected) is not an error.
    pub fn publish(&self, event: LedgerEvent) -> usize {
        self.sender.send(Arc::new(event)).unwrap_or(0)
    }

    pub fn subscriber_count(&self) -> usize {
        self.sender.receiver_count()
    }
}

impl Default for EventBroadcaster {
    fn default() -> Self {
        Self::new(DEFAULT_CHANNEL_CAPACITY)
    }
}

/// One page of `getEvents` results.
#[derive(Debug, Default, PartialEq)]
struct EventsPage {
    events: Vec<Value>,
    latest_ledger: u64,
    /// Cursor to resume after the last event in this page.
    cursor: Option<String>,
}

/// Where the next `getEvents` call starts.
#[derive(Debug, Clone, PartialEq)]
enum StreamPosition {
    /// Need to (re)seed from the network's latest ledger.
    Unseeded,
    StartLedger(u64),
    Cursor(String),
}

pub struct SorobanEventStreamer {
    client: Arc<StellarClient>,
    broadcaster: EventBroadcaster,
    contract_ids: Vec<String>,
    poll_interval: Duration,
}

impl SorobanEventStreamer {
    pub fn new(client: Arc<StellarClient>, broadcaster: EventBroadcaster) -> Self {
        let contract_ids = CONTRACT_ID_ENV_VARS
            .iter()
            .filter_map(|var| env::var(var).ok())
            .map(|id| id.trim().to_string())
            .filter(|id| !id.is_empty())
            .collect();
        Self {
            client,
            broadcaster,
            contract_ids,
            poll_interval: DEFAULT_POLL_INTERVAL,
        }
    }

    pub fn with_contract_ids(mut self, contract_ids: Vec<String>) -> Self {
        self.contract_ids = contract_ids;
        self
    }

    pub fn with_poll_interval(mut self, poll_interval: Duration) -> Self {
        self.poll_interval = poll_interval;
        self
    }

    /// Poll forever, publishing every event. RPC failures back off
    /// exponentially; a JSON-RPC error (e.g. a cursor older than the RPC's
    /// retention window) re-seeds from the latest ledger.
    pub async fn run(self) {
        if self.contract_ids.is_empty() {
            tracing::warn!(
                "No Zaps contract IDs configured; Soroban event stream will relay \
                 every contract event on the network"
            );
        }
        tracing::info!("Starting Soroban event streamer");

        let mut position = StreamPosition::Unseeded;
        let mut backoff_attempt = 0u32;

        loop {
            if position == StreamPosition::Unseeded {
                match self.latest_ledger().await {
                    Ok(ledger) => position = StreamPosition::StartLedger(ledger),
                    Err(err) => {
                        self.backoff(&mut backoff_attempt, &*err).await;
                        continue;
                    }
                }
            }

            match self.fetch_page(&position).await {
                Ok(page) => {
                    backoff_attempt = 0;
                    let full_page = page.events.len() >= PAGE_LIMIT;
                    position = next_position(&position, &page);

                    for raw in page.events {
                        self.broadcaster.publish(LedgerEvent::decode(raw));
                    }

                    if !full_page {
                        tokio::time::sleep(self.poll_interval).await;
                    }
                }
                Err(StreamError::Rpc(err)) => {
                    tracing::warn!("getEvents rejected at {position:?}, re-seeding: {err}");
                    position = StreamPosition::Unseeded;
                    tokio::time::sleep(self.poll_interval).await;
                }
                Err(StreamError::Transport(err)) => {
                    self.backoff(&mut backoff_attempt, &*err).await;
                }
            }
        }
    }

    async fn backoff(&self, attempt: &mut u32, err: &(dyn Error + Send + Sync)) {
        let delay = compute_backoff_delay(*attempt);
        *attempt = attempt.saturating_add(1);
        tracing::warn!("Soroban event stream RPC failed, retrying in {delay:?}: {err}");
        tokio::time::sleep(delay).await;
    }

    async fn latest_ledger(&self) -> Result<u64, Box<dyn Error + Send + Sync>> {
        let response = self
            .client
            .send_rpc_request("getLatestLedger", json!({}))
            .await?;
        response
            .get("result")
            .and_then(|r| r.get("sequence"))
            .and_then(Value::as_u64)
            .ok_or_else(|| format!("getLatestLedger returned no sequence: {response}").into())
    }

    async fn fetch_page(&self, position: &StreamPosition) -> Result<EventsPage, StreamError> {
        let params = build_get_events_params(position, &self.contract_ids);
        let response = self
            .client
            .send_rpc_request("getEvents", params)
            .await
            .map_err(StreamError::Transport)?;
        parse_events_response(&response)
    }
}

#[derive(Debug)]
enum StreamError {
    /// Network/HTTP failure — retry the same position with backoff.
    Transport(Box<dyn Error + Send + Sync>),
    /// The RPC understood the request and rejected it.
    Rpc(String),
}

fn build_get_events_params(position: &StreamPosition, contract_ids: &[String]) -> Value {
    let filters: Vec<Value> = if contract_ids.is_empty() {
        vec![json!({ "type": "contract" })]
    } else {
        contract_ids
            .chunks(MAX_CONTRACT_IDS_PER_FILTER)
            .map(|ids| json!({ "type": "contract", "contractIds": ids }))
            .collect()
    };

    // `startLedger` and `pagination.cursor` are mutually exclusive.
    match position {
        StreamPosition::Cursor(cursor) => json!({
            "filters": filters,
            "pagination": { "cursor": cursor, "limit": PAGE_LIMIT },
        }),
        StreamPosition::StartLedger(ledger) => json!({
            "startLedger": ledger,
            "filters": filters,
            "pagination": { "limit": PAGE_LIMIT },
        }),
        StreamPosition::Unseeded => unreachable!("seeded before fetching"),
    }
}

fn parse_events_response(response: &Value) -> Result<EventsPage, StreamError> {
    if let Some(error) = response.get("error") {
        return Err(StreamError::Rpc(error.to_string()));
    }
    let result = response
        .get("result")
        .ok_or_else(|| StreamError::Rpc(format!("no result in response: {response}")))?;

    let events = result
        .get("events")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    // Newer RPCs return a top-level cursor; older ones only a per-event
    // `pagingToken` (the event `id` is also a valid cursor).
    let cursor = result
        .get("cursor")
        .and_then(Value::as_str)
        .filter(|c| !c.is_empty())
        .map(str::to_string)
        .or_else(|| {
            events.last().and_then(|e| {
                e.get("pagingToken")
                    .or_else(|| e.get("id"))
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
        });

    Ok(EventsPage {
        latest_ledger: result
            .get("latestLedger")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        events,
        cursor,
    })
}

fn next_position(current: &StreamPosition, page: &EventsPage) -> StreamPosition {
    match &page.cursor {
        Some(cursor) => StreamPosition::Cursor(cursor.clone()),
        None => current.clone(),
    }
}

fn compute_backoff_delay(attempt: u32) -> Duration {
    INITIAL_BACKOFF
        .saturating_mul(2u32.saturating_pow(attempt.min(5)))
        .min(MAX_BACKOFF)
}

/// Spawn a streamer against `rpc_url` (plus `STELLAR_RPC_BACKUP_URLS`
/// failover) and return the broadcaster WebSocket routes subscribe to.
pub fn spawn(rpc_url: String) -> (EventBroadcaster, tokio::task::JoinHandle<()>) {
    let broadcaster = EventBroadcaster::default();
    let client = Arc::new(StellarClient::with_env_backups(rpc_url));
    let streamer = SorobanEventStreamer::new(client, broadcaster.clone());
    let handle = tokio::spawn(streamer.run());
    (broadcaster, handle)
}

// ── WebSocket delivery ────────────────────────────────────────────────────────

/// GET — upgrade to a WebSocket that receives every streamed ledger event as
/// a JSON text frame.
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(broadcaster): State<EventBroadcaster>,
) -> Response {
    let receiver = broadcaster.subscribe();
    ws.on_upgrade(move |socket| client_session(socket, receiver))
}

async fn client_session(mut socket: WebSocket, mut events: broadcast::Receiver<Arc<LedgerEvent>>) {
    loop {
        tokio::select! {
            received = events.recv() => {
                let frame = match received {
                    Ok(event) => match serde_json::to_string(&*event) {
                        Ok(text) => text,
                        Err(err) => {
                            tracing::error!("Failed to serialize ledger event: {err}");
                            continue;
                        }
                    },
                    // The client fell more than the channel capacity behind;
                    // tell it how much it missed and carry on from the newest.
                    Err(RecvError::Lagged(skipped)) => {
                        tracing::warn!("WebSocket client lagged, skipped {skipped} events");
                        json!({ "type": "lagged", "skipped": skipped }).to_string()
                    }
                    Err(RecvError::Closed) => break,
                };
                let sent = tokio::time::timeout(WS_SEND_TIMEOUT, socket.send(Message::Text(frame)));
                match sent.await {
                    Ok(Ok(())) => {}
                    Ok(Err(_)) => break,
                    Err(_) => {
                        tracing::warn!("Dropping WebSocket client that stopped reading");
                        break;
                    }
                }
            }
            incoming = socket.recv() => match incoming {
                // Pings are answered by the WebSocket layer; other client
                // messages are ignored — this is a one-way feed.
                Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                Some(Ok(_)) => {}
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// base64 XDR for ScVal::Symbol("UserRegistered").
    fn user_registered_topic() -> String {
        use base64::{engine::general_purpose::STANDARD, Engine};
        let symbol = b"UserRegistered";
        let mut bytes = 14u32.to_be_bytes().to_vec();
        bytes.extend((symbol.len() as u32).to_be_bytes());
        bytes.extend(symbol);
        bytes.resize(bytes.len().div_ceil(4) * 4, 0);
        STANDARD.encode(bytes)
    }

    #[test]
    fn decodes_known_event_with_metadata() {
        let raw = json!({
            "type": "contract",
            "ledger": 4242,
            "ledgerClosedAt": "2026-09-24T00:00:00Z",
            "contractId": "CCONTRACT",
            "id": "0000018219349135361-0000000001",
            "txHash": "abc123",
            "topic": [user_registered_topic()],
            "value": { "address": "GUSER", "username": "maryam" },
        });

        let event = LedgerEvent::decode(raw.clone());

        assert_eq!(event.event_type.as_deref(), Some("UserRegistered"));
        assert_eq!(event.ledger, Some(4242));
        assert_eq!(event.contract_id.as_deref(), Some("CCONTRACT"));
        assert_eq!(event.tx_hash.as_deref(), Some("abc123"));
        assert_eq!(event.data["address"], "GUSER");
        assert_eq!(event.data["username"], "maryam");
        assert_eq!(event.raw, raw);
    }

    #[test]
    fn unknown_event_keeps_raw_payload_with_null_data() {
        let raw = json!({ "ledger": 1, "topic": [] });
        let event = LedgerEvent::decode(raw.clone());
        assert_eq!(event.event_type, None);
        assert_eq!(event.data, Value::Null);
        assert_eq!(event.raw, raw);
    }

    #[test]
    fn decodes_social_payment_event() {
        let raw = json!({
            "event_type": "SocialPaymentEvent",
            "value": { "sender": "GA", "receiver": "GB", "amount": 500, "memo": "hi" },
        });
        let event = LedgerEvent::decode(raw);
        assert_eq!(event.data["sender"], "GA");
        assert_eq!(event.data["amount"], 500);
        assert_eq!(event.data["visibility"], "PUBLIC");
    }

    #[test]
    fn start_ledger_params_have_no_cursor() {
        let params =
            build_get_events_params(&StreamPosition::StartLedger(100), &["CA".to_string()]);
        assert_eq!(params["startLedger"], 100);
        assert!(params["pagination"].get("cursor").is_none());
        assert_eq!(params["filters"][0]["contractIds"], json!(["CA"]));
    }

    #[test]
    fn cursor_params_omit_start_ledger() {
        let params = build_get_events_params(&StreamPosition::Cursor("c-1".into()), &[]);
        assert!(params.get("startLedger").is_none());
        assert_eq!(params["pagination"]["cursor"], "c-1");
        assert_eq!(params["filters"], json!([{ "type": "contract" }]));
    }

    #[test]
    fn contract_ids_are_chunked_to_rpc_filter_limit() {
        let ids: Vec<String> = (0..7).map(|i| format!("C{i}")).collect();
        let params = build_get_events_params(&StreamPosition::StartLedger(1), &ids);
        let filters = params["filters"].as_array().unwrap();
        assert_eq!(filters.len(), 2);
        assert_eq!(filters[0]["contractIds"].as_array().unwrap().len(), 5);
        assert_eq!(filters[1]["contractIds"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn parses_page_and_prefers_top_level_cursor() {
        let response = json!({ "result": {
            "events": [{ "id": "e-1" }, { "id": "e-2" }],
            "latestLedger": 99,
            "cursor": "top-cursor",
        }});
        let page = parse_events_response(&response).unwrap();
        assert_eq!(page.events.len(), 2);
        assert_eq!(page.latest_ledger, 99);
        assert_eq!(page.cursor.as_deref(), Some("top-cursor"));
    }

    #[test]
    fn falls_back_to_last_event_paging_token() {
        let response = json!({ "result": {
            "events": [{ "id": "e-1", "pagingToken": "p-1" }],
            "latestLedger": 5,
        }});
        let page = parse_events_response(&response).unwrap();
        assert_eq!(page.cursor.as_deref(), Some("p-1"));
    }

    #[test]
    fn empty_page_without_cursor_keeps_position() {
        let page = EventsPage::default();
        let position = StreamPosition::StartLedger(7);
        assert_eq!(next_position(&position, &page), position);
    }

    #[test]
    fn json_rpc_error_is_reported_as_rpc_error() {
        let response = json!({ "error": { "code": -32600, "message": "startLedger too old" } });
        assert!(matches!(
            parse_events_response(&response),
            Err(StreamError::Rpc(_))
        ));
    }

    #[test]
    fn backoff_grows_and_caps() {
        assert_eq!(compute_backoff_delay(0), Duration::from_secs(1));
        assert_eq!(compute_backoff_delay(3), Duration::from_secs(8));
        assert_eq!(compute_backoff_delay(10), MAX_BACKOFF);
    }

    #[tokio::test]
    async fn publish_without_subscribers_does_not_fail() {
        let broadcaster = EventBroadcaster::new(4);
        assert_eq!(broadcaster.publish(LedgerEvent::decode(json!({}))), 0);
    }

    #[tokio::test]
    async fn every_subscriber_receives_each_event() {
        let broadcaster = EventBroadcaster::new(4);
        let mut a = broadcaster.subscribe();
        let mut b = broadcaster.subscribe();

        assert_eq!(
            broadcaster.publish(LedgerEvent::decode(json!({ "id": "x" }))),
            2
        );

        assert_eq!(a.recv().await.unwrap().id.as_deref(), Some("x"));
        assert_eq!(b.recv().await.unwrap().id.as_deref(), Some("x"));
    }

    #[tokio::test]
    async fn slow_subscriber_lags_instead_of_blocking_publisher() {
        let broadcaster = EventBroadcaster::new(2);
        let mut slow = broadcaster.subscribe();

        // Publishing past capacity must not block or fail.
        for i in 0..5 {
            broadcaster.publish(LedgerEvent::decode(json!({ "id": i.to_string() })));
        }

        assert!(matches!(slow.recv().await, Err(RecvError::Lagged(3))));
        assert_eq!(slow.recv().await.unwrap().id.as_deref(), Some("3"));
        assert_eq!(slow.recv().await.unwrap().id.as_deref(), Some("4"));
    }
}
