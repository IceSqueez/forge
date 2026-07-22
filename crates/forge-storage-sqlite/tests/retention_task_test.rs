#![allow(clippy::expect_used, clippy::unwrap_used)]

use forge_events::{Event, EventSource};
use forge_storage::{DataProvider, SettingsRepo};
use forge_storage_sqlite::SqliteBackend;
use forge_types::EventId;
use time::OffsetDateTime;

const TEST_KEY: [u8; 32] = [0xab; 32];

async fn setup_file_db(interval: std::time::Duration) -> (SqliteBackend, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("create tmpdir");
    let db_path = dir.path().join("test.db");
    let url = format!("sqlite:{}", db_path.display());
    let backend = SqliteBackend::open_with_key_and_interval(&url, TEST_KEY, interval)
        .await
        .expect("open");
    (backend, dir)
}

fn make_event(source: EventSource, kind: &str, timestamp: OffsetDateTime) -> Event {
    Event {
        id: EventId::new(),
        source,
        kind: kind.to_string(),
        timestamp,
        payload: serde_json::Value::Null,
        caused_by: None,
        replay: false,
    }
}

#[tokio::test]
async fn retention_task_prunes_old_events_and_spares_recent() {
    let (backend, _dir) = setup_file_db(std::time::Duration::from_millis(50)).await;

    let now = OffsetDateTime::now_utc();

    let old_event = make_event(
        EventSource::Core,
        "timer.tick",
        now - time::Duration::days(30),
    );
    let old_id = old_event.id;
    backend
        .event_log_repo()
        .insert(&old_event)
        .await
        .expect("insert old event");

    let recent_event = make_event(
        EventSource::Core,
        "action.start",
        now - time::Duration::days(1),
    );
    let recent_id = recent_event.id;
    backend
        .event_log_repo()
        .insert(&recent_event)
        .await
        .expect("insert recent event");

    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    assert!(
        backend
            .event_log_repo()
            .get(old_id)
            .await
            .expect("get old")
            .is_none(),
        "event 30 days old must be pruned by 7-day retention policy"
    );
    assert!(
        backend
            .event_log_repo()
            .get(recent_id)
            .await
            .expect("get recent")
            .is_some(),
        "event 1 day old must survive 7-day retention policy"
    );

    backend.shutdown_retention_pruner();
}

#[tokio::test]
async fn retention_task_respects_custom_retention_days_from_settings() {
    let (backend, _dir) = setup_file_db(std::time::Duration::from_millis(50)).await;

    backend
        .set_string("event_log_retention_days", "3")
        .await
        .expect("set retention days");

    let now = OffsetDateTime::now_utc();

    let event_5d = make_event(
        EventSource::Twitch,
        "chat.message",
        now - time::Duration::days(5),
    );
    let id_5d = event_5d.id;
    backend
        .event_log_repo()
        .insert(&event_5d)
        .await
        .expect("insert 5d event");

    let event_1d = make_event(
        EventSource::Twitch,
        "chat.message",
        now - time::Duration::days(1),
    );
    let id_1d = event_1d.id;
    backend
        .event_log_repo()
        .insert(&event_1d)
        .await
        .expect("insert 1d event");

    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    assert!(
        backend
            .event_log_repo()
            .get(id_5d)
            .await
            .expect("get 5d")
            .is_none(),
        "event 5 days old must be pruned under 3-day retention"
    );
    assert!(
        backend
            .event_log_repo()
            .get(id_1d)
            .await
            .expect("get 1d")
            .is_some(),
        "event 1 day old must survive 3-day retention"
    );

    backend.shutdown_retention_pruner();
}
