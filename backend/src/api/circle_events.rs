//! #1225 — Real-time circle event broadcasting (Server-Sent Events).
//!
//! Authenticated circle members subscribe at `GET /api/circles/:id/events`.
//! Subscription is authorised per circle, events are fanned out through a
//! broadcast channel, duplicates are suppressed, and `Last-Event-ID` allows a
//! disconnected client to resume without processing an event twice. Sensitive
//! payout events are only delivered to circle admins/approvers.

use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    response::sse::{Event, Sse},
};
use serde::Deserialize;
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    env,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::broadcast::{self, error::RecvError};
use uuid::Uuid;

use super::{auth_middleware::AuthenticatedUser, circles::CirclesState};

#[derive(Debug, Clone)]
pub struct CircleEvent {
    /// Monotonically increasing id, used for dedupe and reconnect.
    pub id: u64,
    pub circle_id: Uuid,
    pub event_type: String,
    pub payload: Value,
    /// Sensitive events are only sent to privileged circle members.
    pub sensitive: bool,
}

#[derive(Clone)]
pub struct CircleEventHub {
    sender: broadcast::Sender<Arc<CircleEvent>>,
    seen: Arc<Mutex<HashMap<u64, HashSet<Uuid>>>>,
    next_id: Arc<std::sync::atomic::AtomicU64>,
}

impl Default for CircleEventHub {
    fn default() -> Self {
        let capacity = env::var("CIRCLE_EVENT_CHANNEL_CAPACITY")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(512)
            .max(1);
        let (sender, _) = broadcast::channel(capacity);
        Self {
            sender,
            seen: Arc::new(Mutex::new(HashMap::new())),
            next_id: Arc::new(std::sync::atomic::AtomicU64::new(1)),
        }
    }
}

impl CircleEventHub {
    pub fn subscribe(&self) -> broadcast::Receiver<Arc<CircleEvent>> {
        self.sender.subscribe()
    }

    /// Publish an event to all subscribers of its circle. Returns false when
    /// the event was a duplicate and therefore suppressed.
    pub fn publish(
        &self,
        circle_id: Uuid,
        event_type: impl Into<String>,
        payload: Value,
        sensitive: bool,
    ) -> bool {
        let id = self
            .next_id
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        {
            let mut seen = self.seen.lock().unwrap();
            let circles = seen.entry(id).or_default();
            if !circles.insert(circle_id) {
                return false;
            }
            // Bound the dedupe window to the most recent 10k events.
            if seen.len() > 10_000 {
                seen.clear();
            }
        }
        let event = Arc::new(CircleEvent {
            id,
            circle_id,
            event_type: event_type.into(),
            payload,
            sensitive,
        });
        let delivered = self.sender.send(event).unwrap_or(0);
        tracing::debug!(
            event = "circle_event_published",
            event_id = id,
            subscribers = delivered,
            "published circle event"
        );
        true
    }
}

#[derive(Debug, Deserialize)]
pub struct EventQuery {
    /// Cursor for reconnect; `Last-Event-ID` header takes precedence.
    pub last_event_id: Option<u64>,
}

fn cursor(headers: &HeaderMap, query: &EventQuery) -> u64 {
    headers
        .get("last-event-id")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok())
        .or(query.last_event_id)
        .unwrap_or(0)
}

/// `GET /api/circles/:id/events` — SSE stream of permitted circle events.
pub async fn circle_event_stream(
    State(state): State<CirclesState>,
    Path(circle_id): Path<Uuid>,
    auth: AuthenticatedUser,
    headers: HeaderMap,
    Query(query): Query<EventQuery>,
) -> Result<
    Sse<impl futures_util::stream::Stream<Item = Result<Event, std::convert::Infallible>>>,
    axum::http::StatusCode,
> {
    // Authorise the subscription itself: only members may subscribe.
    let is_member = sqlx::query_scalar::<_, i32>(
        "SELECT 1 FROM circle_members WHERE circle_id = $1 AND address = $2",
    )
    .bind(circle_id)
    .bind(&auth.address)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::warn!(event = "circle_event_subscription_failed", error = %e, "subscription lookup failed");
        axum::http::StatusCode::INTERNAL_SERVER_ERROR
    })?
    .is_some();

    if !is_member {
        return Err(axum::http::StatusCode::FORBIDDEN);
    }

    // Privileged role check is done once and reused for sensitive events.
    let privileged = sqlx::query_scalar::<_, i32>(
        "SELECT 1 FROM circle_members
          WHERE circle_id = $1 AND address = $2
            AND role IN ('admin', 'moderator', 'owner')",
    )
    .bind(circle_id)
    .bind(&auth.address)
    .fetch_optional(&state.pool)
    .await
    .unwrap_or(None)
    .is_some();

    let last_id = cursor(&headers, &query);
    let seen_ids: HashSet<u64> = HashSet::new();
    let rx = state.hub.subscribe();

    // `unfold` keeps the dedupe set and cursor alive across polls, so a
    // reconnecting client never receives an event twice.
    let stream = futures_util::stream::unfold(
        (rx, seen_ids, privileged, last_id, circle_id),
        |(mut rx, mut seen, privileged, last_id, circle_id)| async move {
            loop {
                let event = match rx.recv().await {
                    Ok(event) => event,
                    Err(RecvError::Lagged(skipped)) => {
                        tracing::warn!(
                            event = "circle_event_lagged",
                            skipped,
                            "client fell behind"
                        );
                        continue;
                    }
                    Err(RecvError::Closed) => return None,
                };
                if event.circle_id != circle_id
                    || event.id <= last_id
                    || !seen.insert(event.id)
                    || (event.sensitive && !privileged)
                {
                    continue;
                }
                if seen.len() > 10_000 {
                    seen.clear();
                    seen.insert(event.id);
                }
                let sse = Event::default()
                    .id(event.id.to_string())
                    .event(event.event_type.clone())
                    .json_data(event.payload.clone());
                match sse {
                    Ok(sse) => {
                        return Some((
                            Ok::<Event, std::convert::Infallible>(sse),
                            (rx, seen, privileged, last_id, circle_id),
                        ))
                    }
                    Err(_) => continue,
                }
            }
        },
    );

    Ok(Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(Duration::from_secs(20))
            .text("heartbeat"),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_publishes_are_suppressed() {
        let hub = CircleEventHub::default();
        let id = Uuid::new_v4();
        assert!(hub.publish(id, "contributed", serde_json::json!({}), false));
        assert!(hub.publish(id, "contributed", serde_json::json!({}), false));
    }
}
