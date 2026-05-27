#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;
use std::time::Duration;

use forge_events::{Event, EventSource};
use forge_runtime::EventBus;
use forge_storage::DataProvider;
use forge_storage_sqlite::SqliteBackend;

const TEST_KEY: [u8; 32] = [0xab; 32];

#[tokio::test]
async fn event_log_persists_published_events() {
    let backend = SqliteBackend::open_with_key(":memory:", TEST_KEY)
        .await
        .unwrap();
    let backend: Arc<dyn DataProvider> = Arc::new(backend);

    let event_log = backend.event_log_repo();
    let bus = EventBus::new(Arc::clone(&event_log));
    EventBus::spawn_flush_task(Arc::clone(&bus));

    let event = Event::new(EventSource::Core, "test.boot_wire", serde_json::Value::Null);
    bus.publish(event.clone());

    tokio::time::sleep(Duration::from_millis(50)).await;

    let recent = event_log.recent(10).await.unwrap();
    assert!(
        recent.iter().any(|e| e.id == event.id),
        "published event must appear in persisted event_log"
    );
}
