#![allow(clippy::expect_used, clippy::unwrap_used)]

use forge_events::{Event, EventSource};
use forge_storage::DataProvider;
use forge_storage_sqlite::SqliteBackend;
use forge_types::EventId;
use time::OffsetDateTime;

const TEST_KEY: [u8; 32] = [0xab; 32];

async fn setup() -> SqliteBackend {
    SqliteBackend::open_with_key("sqlite::memory:", TEST_KEY)
        .await
        .expect("open")
}

fn make_event_at(source: EventSource, kind: &str, ts: OffsetDateTime) -> Event {
    Event {
        id: EventId::new(),
        source,
        kind: kind.to_string(),
        timestamp: ts,
        payload: serde_json::Value::Null,
        caused_by: None,
        replay: false,
    }
}

#[tokio::test]
async fn insert_and_get_roundtrip() {
    let backend = setup().await;
    let ts = OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap();
    let event = make_event_at(EventSource::Twitch, "chat.message", ts);
    let id = event.id;

    backend
        .event_log_repo()
        .insert(&event)
        .await
        .expect("insert");

    let fetched = backend
        .event_log_repo()
        .get(id)
        .await
        .expect("get")
        .expect("present");

    assert_eq!(fetched.id, event.id);
    assert_eq!(fetched.source, event.source);
    assert_eq!(fetched.kind, event.kind);
    assert_eq!(fetched.timestamp.unix_timestamp(), ts.unix_timestamp());
    assert_eq!(fetched.payload, event.payload);
    assert_eq!(fetched.caused_by, event.caused_by);
    assert_eq!(fetched.replay, event.replay);
}

#[tokio::test]
async fn get_missing_returns_none() {
    let backend = setup().await;
    let result = backend
        .event_log_repo()
        .get(EventId::new())
        .await
        .expect("get");
    assert!(result.is_none());
}

#[tokio::test]
async fn recent_returns_desc_timestamp_order_with_limit() {
    let backend = setup().await;
    let base = 1_700_000_000_i64;

    for i in 0..4_i64 {
        let ts = OffsetDateTime::from_unix_timestamp(base + i).unwrap();
        let ev = make_event_at(EventSource::Core, "timer.tick", ts);
        backend.event_log_repo().insert(&ev).await.expect("insert");
    }

    let recent = backend.event_log_repo().recent(3).await.expect("recent");

    assert_eq!(recent.len(), 3);

    let timestamps: Vec<i64> = recent
        .iter()
        .map(|e| e.timestamp.unix_timestamp())
        .collect();
    assert!(
        timestamps.windows(2).all(|w| w[0] >= w[1]),
        "events must be ordered newest-first: {timestamps:?}"
    );
    assert_eq!(timestamps[0], base + 3);
    assert_eq!(timestamps[1], base + 2);
    assert_eq!(timestamps[2], base + 1);
}

#[tokio::test]
async fn prune_before_removes_old_events_and_returns_count() {
    let backend = setup().await;
    let base = 1_700_000_000_i64;

    let mut ids = Vec::new();
    for i in 0..5_i64 {
        let ts = OffsetDateTime::from_unix_timestamp(base + i).unwrap();
        let ev = make_event_at(EventSource::Core, "action.start", ts);
        ids.push(ev.id);
        backend.event_log_repo().insert(&ev).await.expect("insert");
    }

    let cutoff = OffsetDateTime::from_unix_timestamp(base + 3).unwrap();
    let deleted = backend
        .event_log_repo()
        .prune_before(cutoff)
        .await
        .expect("prune_before");

    assert_eq!(deleted, 3, "should delete events at base+0, base+1, base+2");

    assert!(
        backend
            .event_log_repo()
            .get(ids[0])
            .await
            .expect("get")
            .is_none()
    );
    assert!(
        backend
            .event_log_repo()
            .get(ids[1])
            .await
            .expect("get")
            .is_none()
    );
    assert!(
        backend
            .event_log_repo()
            .get(ids[2])
            .await
            .expect("get")
            .is_none()
    );
    assert!(
        backend
            .event_log_repo()
            .get(ids[3])
            .await
            .expect("get")
            .is_some()
    );
    assert!(
        backend
            .event_log_repo()
            .get(ids[4])
            .await
            .expect("get")
            .is_some()
    );
}

#[tokio::test]
async fn json_payload_roundtrip_with_caused_by_and_replay() {
    let backend = setup().await;
    let ts = OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap();
    let parent_id = EventId::new();

    let event = Event {
        id: EventId::new(),
        source: EventSource::Obs,
        kind: "scene.changed".to_string(),
        timestamp: ts,
        payload: serde_json::json!({"from": "Menu", "to": "Gameplay", "nested": {"x": 1}}),
        caused_by: Some(parent_id),
        replay: true,
    };
    let id = event.id;

    backend
        .event_log_repo()
        .insert(&event)
        .await
        .expect("insert");

    let fetched = backend
        .event_log_repo()
        .get(id)
        .await
        .expect("get")
        .expect("present");

    assert_eq!(fetched.source, EventSource::Obs);
    assert_eq!(fetched.kind, "scene.changed");
    assert_eq!(fetched.payload["from"], "Menu");
    assert_eq!(fetched.payload["to"], "Gameplay");
    assert_eq!(fetched.payload["nested"]["x"], 1);
    assert_eq!(fetched.caused_by, Some(parent_id));
    assert!(fetched.replay);
}
