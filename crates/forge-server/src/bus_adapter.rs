use std::collections::HashSet;
use std::fmt;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use forge_events::{Event, EventSource, EventsError};
use forge_runtime::EventBus;
use forge_storage::OverlayId;
use forge_types::EventId;
use serde::Serialize;
use time::format_description::well_known::Rfc3339;
use tokio::sync::{RwLock, broadcast};

pub(crate) const CLIENT_CHANNEL_CAP: usize = 1024;
/// Below the general 1024 bound: append/transient overlay content gains nothing from a deep
/// backlog, and a stalled page should rejoin near-live rather than crawl through history.
pub(crate) const OVERLAY_CHANNEL_CAP: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ClientId(pub(crate) u64);

impl ClientId {
    pub(crate) fn next() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        Self(COUNTER.fetch_add(1, Ordering::Relaxed))
    }
}

impl fmt::Display for ClientId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ws_{:04x}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EventFilter {
    pub source: Option<EventSource>,
    pub kind: Option<String>,
}

impl EventFilter {
    pub fn new(source: Option<EventSource>, kind: Option<String>) -> Self {
        Self { source, kind }
    }

    fn matches(&self, event: &Event) -> bool {
        let source_ok = self.source.is_none_or(|s| s == event.source);
        let kind_ok = self
            .kind
            .as_deref()
            .is_none_or(|k| k == event.kind.as_str());
        source_ok && kind_ok
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientFilterSet {
    pub subscriptions: HashSet<EventFilter>,
}

impl ClientFilterSet {
    pub fn new(subscriptions: HashSet<EventFilter>) -> Self {
        Self { subscriptions }
    }

    pub fn matches(&self, event: &Event) -> bool {
        self.subscriptions.iter().any(|f| f.matches(event))
    }
}

#[derive(Debug, Clone)]
pub enum WsFrame {
    Text(String),
    Close,
}

pub struct ClientHandle {
    pub id: ClientId,
    pub drop_counter: Arc<AtomicU64>,
}

struct ConnectedClient {
    id: ClientId,
    sender: broadcast::Sender<WsFrame>,
    filters: ClientFilterSet,
    overlay_identity: Option<OverlayId>,
}

pub struct BusAdapter {
    bus: Arc<EventBus>,
    registry: Arc<RwLock<Vec<ConnectedClient>>>,
}

#[derive(Serialize)]
struct PushFrame<'a> {
    #[serde(rename = "timeStamp")]
    time_stamp: String,
    event: PushEventEnvelope<'a>,
    data: &'a serde_json::Value,
}

#[derive(Serialize)]
struct PushEventEnvelope<'a> {
    source: EventSource,
    #[serde(rename = "type")]
    kind: &'a str,
    id: EventId,
    #[serde(rename = "causedBy", skip_serializing_if = "Option::is_none")]
    caused_by: Option<EventId>,
}

fn serialize_push(event: &Event) -> Result<String, serde_json::Error> {
    let time_stamp = event
        .timestamp
        .format(&Rfc3339)
        .unwrap_or_else(|_| String::new());
    let frame = PushFrame {
        time_stamp,
        event: PushEventEnvelope {
            source: event.source,
            kind: &event.kind,
            id: event.id,
            caused_by: event.caused_by,
        },
        data: &event.payload,
    };
    serde_json::to_string(&frame)
}

#[derive(Serialize)]
struct ContentFrame<'a> {
    frame: &'static str,
    content: &'a serde_json::Value,
    #[serde(rename = "durationMs", skip_serializing_if = "Option::is_none")]
    duration_ms: Option<u64>,
}

const RELOAD_FRAME_JSON: &str = r#"{"frame":"reload"}"#;

fn serialize_content_frame(
    content: &serde_json::Value,
    duration_ms: Option<u64>,
) -> Option<String> {
    serde_json::to_string(&ContentFrame {
        frame: "content",
        content,
        duration_ms,
    })
    .ok()
}

/// Overlay-class connections never reach here: they carry no filters and receive nothing from
/// the bus, only from directed delivery.
async fn fan_out(registry: &RwLock<Vec<ConnectedClient>>, event: &Event) {
    let Ok(json) = serialize_push(event) else {
        return;
    };
    let frame = WsFrame::Text(json);

    let mut disconnected = Vec::new();
    {
        let reg = registry.read().await;
        for client in reg.iter() {
            if client.overlay_identity.is_some() {
                continue;
            }
            if client.filters.matches(event) && client.sender.send(frame.clone()).is_err() {
                disconnected.push(client.id);
            }
        }
    }

    if !disconnected.is_empty() {
        registry
            .write()
            .await
            .retain(|c| !disconnected.contains(&c.id));
    }
}

/// `identity: None` addresses every overlay-class connection; a non-overlay client never
/// matches, since its `overlay_identity` is `None`.
async fn send_to_overlay(
    registry: &RwLock<Vec<ConnectedClient>>,
    identity: Option<&OverlayId>,
    frame: WsFrame,
) {
    let reg = registry.read().await;
    for client in reg.iter() {
        let targeted = match (&client.overlay_identity, identity) {
            (Some(client_identity), Some(target)) => client_identity == target,
            (Some(_), None) => true,
            (None, _) => false,
        };
        if targeted {
            let _ = client.sender.send(frame.clone());
        }
    }
}

impl BusAdapter {
    pub fn new(bus: Arc<EventBus>) -> Arc<Self> {
        Arc::new(Self {
            bus,
            registry: Arc::new(RwLock::new(Vec::new())),
        })
    }

    pub fn spawn(self: &Arc<Self>) {
        let registry = Arc::clone(&self.registry);
        let subscription = self.bus.subscribe();
        tokio::spawn(async move {
            let mut subscription = subscription;
            loop {
                let event = match subscription.recv().await {
                    Ok(e) => e,
                    Err(EventsError::BusClosed) => break,
                    Err(EventsError::LaggingReceiver | EventsError::ReplayMiss(_)) => continue,
                };

                fan_out(&registry, &event).await;
            }
        });
    }

    /// Reaches subscribers without touching the bus, so nothing downstream of the bus observes it.
    pub async fn deliver(&self, event: &Event) {
        fan_out(&self.registry, event).await;
    }

    /// Addressed at the connections belonging to one overlay identity.
    pub async fn deliver_overlay_content(
        &self,
        identity: &OverlayId,
        content: &serde_json::Value,
        duration_ms: Option<u64>,
    ) {
        let Some(json) = serialize_content_frame(content, duration_ms) else {
            return;
        };
        send_to_overlay(&self.registry, Some(identity), WsFrame::Text(json)).await;
    }

    /// `identity: None` reloads every overlay-class connection.
    pub async fn deliver_overlay_reload(&self, identity: Option<&OverlayId>) {
        send_to_overlay(
            &self.registry,
            identity,
            WsFrame::Text(RELOAD_FRAME_JSON.to_owned()),
        )
        .await;
    }

    pub async fn register_client(
        &self,
        filters: ClientFilterSet,
    ) -> (ClientHandle, broadcast::Receiver<WsFrame>) {
        let id = ClientId::next();
        let drop_counter = Arc::new(AtomicU64::new(0));
        let (sender, receiver) = broadcast::channel(CLIENT_CHANNEL_CAP);
        self.registry.write().await.push(ConnectedClient {
            id,
            sender,
            filters,
            overlay_identity: None,
        });
        let handle = ClientHandle { id, drop_counter };
        (handle, receiver)
    }

    /// Derives the connection's identity from a validated credential, never from a client
    /// claim, and shrinks its outbound queue to the overlay bound. `None` means the client id
    /// was not found (already disconnected).
    pub async fn promote_to_overlay(
        &self,
        id: ClientId,
        identity: OverlayId,
    ) -> Option<broadcast::Receiver<WsFrame>> {
        let mut reg = self.registry.write().await;
        let client = reg.iter_mut().find(|c| c.id == id)?;
        let (sender, receiver) = broadcast::channel(OVERLAY_CHANNEL_CAP);
        client.sender = sender;
        client.overlay_identity = Some(identity);
        client.filters = ClientFilterSet::new(HashSet::new());
        Some(receiver)
    }

    pub async fn unregister_client(&self, id: ClientId) {
        self.registry.write().await.retain(|c| c.id != id);
    }

    pub async fn kick_client(&self, id: ClientId) -> bool {
        let mut reg = self.registry.write().await;
        let Some(pos) = reg.iter().position(|c| c.id == id) else {
            return false;
        };
        let client = reg.remove(pos);
        let _ = client.sender.send(WsFrame::Close);
        true
    }

    pub async fn update_subscriptions(&self, id: ClientId, filters: ClientFilterSet) {
        let mut reg = self.registry.write().await;
        if let Some(client) = reg.iter_mut().find(|c| c.id == id) {
            client.filters = filters;
        }
    }

    pub async fn current_subscriptions(&self, id: ClientId) -> HashSet<EventFilter> {
        self.registry
            .read()
            .await
            .iter()
            .find(|c| c.id == id)
            .map(|c| c.filters.subscriptions.clone())
            .unwrap_or_default()
    }

    pub async fn broadcast_close(&self) {
        let reg = self.registry.read().await;
        for client in reg.iter() {
            let _ = client.sender.send(WsFrame::Close);
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use forge_runtime::{EventBus, NullEventLogRepo};

    fn make_bus() -> Arc<EventBus> {
        EventBus::new(Arc::new(NullEventLogRepo))
    }

    fn twitch_filter() -> ClientFilterSet {
        ClientFilterSet::new(HashSet::from([EventFilter::new(
            Some(EventSource::Twitch),
            None,
        )]))
    }

    fn wildcard_filter() -> ClientFilterSet {
        ClientFilterSet::new(HashSet::from([EventFilter::new(None, None)]))
    }

    #[tokio::test]
    async fn matching_event_reaches_subscribed_client() {
        let bus = make_bus();
        let adapter = BusAdapter::new(Arc::clone(&bus));
        adapter.spawn();

        let (_, mut rx) = adapter.register_client(twitch_filter()).await;

        bus.publish(Event::new(
            EventSource::Twitch,
            "chat.message",
            serde_json::Value::Null,
        ));

        let frame = tokio::time::timeout(std::time::Duration::from_millis(200), rx.recv())
            .await
            .expect("timeout waiting for event")
            .expect("receiver error");

        assert!(matches!(frame, WsFrame::Text(_)));
    }

    #[tokio::test]
    async fn non_matching_event_does_not_reach_client() {
        let bus = make_bus();
        let adapter = BusAdapter::new(Arc::clone(&bus));
        adapter.spawn();

        let (_, mut filtered_rx) = adapter.register_client(twitch_filter()).await;
        let (_, mut probe_rx) = adapter.register_client(wildcard_filter()).await;

        bus.publish(Event::new(
            EventSource::Obs,
            "scene.changed",
            serde_json::Value::Null,
        ));

        tokio::time::timeout(std::time::Duration::from_millis(200), probe_rx.recv())
            .await
            .expect("timeout waiting for probe")
            .expect("probe receiver error");

        assert!(
            filtered_rx.try_recv().is_err(),
            "Twitch-filtered client must not receive OBS event"
        );
    }

    #[tokio::test]
    async fn unregistered_client_does_not_receive_events() {
        let bus = make_bus();
        let adapter = BusAdapter::new(Arc::clone(&bus));
        adapter.spawn();

        let (handle, mut rx) = adapter.register_client(twitch_filter()).await;
        adapter.unregister_client(handle.id).await;

        let (_, mut probe_rx) = adapter.register_client(wildcard_filter()).await;
        bus.publish(Event::new(
            EventSource::Twitch,
            "chat.message",
            serde_json::Value::Null,
        ));

        tokio::time::timeout(std::time::Duration::from_millis(200), probe_rx.recv())
            .await
            .expect("timeout waiting for probe")
            .expect("probe receiver error");

        assert!(
            rx.try_recv().is_err(),
            "unregistered client must not receive events"
        );
    }

    #[tokio::test]
    async fn kick_client_delivers_a_close_frame_to_the_kicked_client() {
        let adapter = BusAdapter::new(make_bus());
        let (kicked, mut kicked_rx) = adapter.register_client(wildcard_filter()).await;

        assert!(adapter.kick_client(kicked.id).await);

        assert!(matches!(
            kicked_rx.try_recv().expect("close frame"),
            WsFrame::Close
        ));
    }

    #[tokio::test]
    async fn kicked_client_stops_receiving_events_while_the_others_keep_theirs() {
        let bus = make_bus();
        let adapter = BusAdapter::new(Arc::clone(&bus));
        adapter.spawn();

        let (kicked, mut kicked_rx) = adapter.register_client(wildcard_filter()).await;
        let (_survivor, mut survivor_rx) = adapter.register_client(wildcard_filter()).await;

        adapter.kick_client(kicked.id).await;
        assert!(matches!(
            kicked_rx.try_recv().expect("close frame"),
            WsFrame::Close
        ));

        bus.publish(Event::new(
            EventSource::Twitch,
            "chat.message",
            serde_json::Value::Null,
        ));

        let frame = tokio::time::timeout(std::time::Duration::from_millis(200), survivor_rx.recv())
            .await
            .expect("timeout waiting for survivor")
            .expect("survivor receiver error");
        assert!(matches!(frame, WsFrame::Text(_)));

        assert!(
            kicked_rx.try_recv().is_err(),
            "kicked client must be off the registry, not merely closed"
        );
    }

    #[tokio::test]
    async fn kick_client_with_unknown_id_returns_false_and_leaves_the_registry_intact() {
        let bus = make_bus();
        let adapter = BusAdapter::new(Arc::clone(&bus));
        adapter.spawn();

        let (_client, mut rx) = adapter.register_client(wildcard_filter()).await;

        assert!(!adapter.kick_client(ClientId::next()).await);

        bus.publish(Event::new(
            EventSource::Twitch,
            "chat.message",
            serde_json::Value::Null,
        ));

        let frame = tokio::time::timeout(std::time::Duration::from_millis(200), rx.recv())
            .await
            .expect("timeout waiting for event")
            .expect("receiver error");
        assert!(matches!(frame, WsFrame::Text(_)));
    }

    #[tokio::test]
    async fn wildcard_source_filter_matches_all_twitch_kinds() {
        let bus = make_bus();
        let adapter = BusAdapter::new(Arc::clone(&bus));
        adapter.spawn();

        let (_, mut rx) = adapter.register_client(twitch_filter()).await;

        bus.publish(Event::new(
            EventSource::Twitch,
            "chat.message",
            serde_json::Value::Null,
        ));
        bus.publish(Event::new(
            EventSource::Twitch,
            "chat.send",
            serde_json::Value::Null,
        ));
        bus.publish(Event::new(
            EventSource::Obs,
            "scene.changed",
            serde_json::Value::Null,
        ));

        let first = tokio::time::timeout(std::time::Duration::from_millis(200), rx.recv())
            .await
            .expect("timeout first")
            .expect("recv error first");
        let second = tokio::time::timeout(std::time::Duration::from_millis(200), rx.recv())
            .await
            .expect("timeout second")
            .expect("recv error second");

        assert!(matches!(first, WsFrame::Text(_)));
        assert!(matches!(second, WsFrame::Text(_)));
        assert!(
            rx.try_recv().is_err(),
            "OBS event must not pass Twitch-source filter"
        );
    }
}
