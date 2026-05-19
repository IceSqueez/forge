use std::collections::HashSet;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use forge_events::{Event, EventSource, EventsError};
use forge_runtime::EventBus;
use forge_types::EventId;
use serde::Serialize;
use time::format_description::well_known::Rfc3339;
use tokio::sync::{RwLock, broadcast};

pub(crate) const CLIENT_CHANNEL_CAP: usize = 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ClientId(u64);

impl ClientId {
    pub(crate) fn next() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        Self(COUNTER.fetch_add(1, Ordering::Relaxed))
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

    pub fn wildcard() -> Self {
        Self {
            source: None,
            kind: None,
        }
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
    pub sender: broadcast::Sender<WsFrame>,
    pub drop_counter: Arc<AtomicU64>,
}

struct ConnectedClient {
    id: ClientId,
    sender: broadcast::Sender<WsFrame>,
    filters: ClientFilterSet,
    drop_counter: Arc<AtomicU64>,
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

                let json = match serialize_push(&event) {
                    Ok(s) => s,
                    Err(_) => continue,
                };
                let frame = WsFrame::Text(json);

                let mut disconnected = Vec::new();
                {
                    let reg = registry.read().await;
                    for client in reg.iter() {
                        if client.filters.matches(&event)
                            && client.sender.send(frame.clone()).is_err()
                        {
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
        });
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
            sender: sender.clone(),
            filters,
            drop_counter: Arc::clone(&drop_counter),
        });
        let handle = ClientHandle {
            id,
            sender,
            drop_counter,
        };
        (handle, receiver)
    }

    pub async fn unregister_client(&self, id: ClientId) {
        self.registry.write().await.retain(|c| c.id != id);
    }

    pub async fn update_subscriptions(&self, id: ClientId, filters: ClientFilterSet) {
        let mut reg = self.registry.write().await;
        if let Some(client) = reg.iter_mut().find(|c| c.id == id) {
            client.filters = filters;
        }
    }

    pub async fn drop_count_for(&self, id: ClientId) -> Option<u64> {
        self.registry
            .read()
            .await
            .iter()
            .find(|c| c.id == id)
            .map(|c| c.drop_counter.load(Ordering::Relaxed))
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
        ClientFilterSet::new(HashSet::from([EventFilter::wildcard()]))
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
