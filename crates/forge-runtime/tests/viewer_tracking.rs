#![allow(clippy::unwrap_used)]

use std::sync::Arc;

use forge_events::{Event, EventSource};
use forge_runtime::{EventBus, NullEventLogRepo, spawn_viewer_tracker};
use forge_storage::ViewerPlatform;
use forge_storage::viewer::MockViewerRepo;
use tokio::sync::mpsc;
use tokio::time::{Duration, timeout};

const RECV_TIMEOUT: Duration = Duration::from_secs(5);

type Recorded = (ViewerPlatform, String, String);

fn bus() -> Arc<EventBus> {
    EventBus::new(Arc::new(NullEventLogRepo))
}

fn chat_event(source: EventSource, kind: &str, payload: serde_json::Value) -> Event {
    Event::new(source, kind, payload)
}

fn user_message(source: EventSource, id: &str, login: &str) -> Event {
    chat_event(
        source,
        "chat.message",
        serde_json::json!({ "user": { "id": id, "login": login } }),
    )
}

async fn wait_until_subscribed() {
    for _ in 0..16 {
        tokio::task::yield_now().await;
    }
}

fn tracker_recording_into(tx: mpsc::UnboundedSender<Recorded>) -> MockViewerRepo {
    let mut repo = MockViewerRepo::new();
    repo.expect_record_message()
        .returning(move |platform, id, login| {
            let _ = tx.send((platform, id.to_string(), login.to_string()));
            Ok(())
        });
    repo
}

#[tokio::test]
async fn records_one_message_per_platform_source() {
    let bus = bus();
    let (tx, mut rx) = mpsc::unbounded_channel::<Recorded>();

    spawn_viewer_tracker(Arc::clone(&bus), Arc::new(tracker_recording_into(tx)));
    wait_until_subscribed().await;

    bus.publish(user_message(EventSource::Twitch, "t1", "alice"));
    bus.publish(user_message(EventSource::YouTube, "y1", "bob"));
    bus.publish(user_message(EventSource::Kick, "k1", "carol"));

    let mut got = Vec::new();
    for _ in 0..3 {
        got.push(timeout(RECV_TIMEOUT, rx.recv()).await.unwrap().unwrap());
    }

    assert_eq!(
        got,
        vec![
            (
                ViewerPlatform::Twitch,
                "t1".to_string(),
                "alice".to_string()
            ),
            (ViewerPlatform::YouTube, "y1".to_string(), "bob".to_string()),
            (ViewerPlatform::Kick, "k1".to_string(), "carol".to_string()),
        ]
    );
}

#[tokio::test]
async fn skips_events_that_are_not_recordable_chat_messages() {
    let bus = bus();
    let (tx, mut rx) = mpsc::unbounded_channel::<Recorded>();

    spawn_viewer_tracker(Arc::clone(&bus), Arc::new(tracker_recording_into(tx)));
    wait_until_subscribed().await;

    let skipped = [
        user_message(EventSource::Core, "c1", "core-user"),
        chat_event(
            EventSource::Twitch,
            "chat.whisper",
            serde_json::json!({ "user": { "id": "w1", "login": "whisper" } }),
        ),
        chat_event(
            EventSource::Twitch,
            "chat.message",
            serde_json::json!({ "text": "hi" }),
        ),
        user_message(EventSource::Twitch, "", "loginonly"),
        user_message(EventSource::Twitch, "idonly", ""),
        chat_event(
            EventSource::Twitch,
            "chat.message",
            serde_json::json!({ "user": { "login": "no-id" } }),
        ),
        chat_event(
            EventSource::Twitch,
            "chat.message",
            serde_json::json!({ "user": { "id": "no-login" } }),
        ),
    ];
    for event in skipped {
        bus.publish(event);
    }

    bus.publish(user_message(EventSource::Twitch, "ok", "sentinel"));

    let recorded = timeout(RECV_TIMEOUT, rx.recv()).await.unwrap().unwrap();
    assert_eq!(
        recorded,
        (
            ViewerPlatform::Twitch,
            "ok".to_string(),
            "sentinel".to_string()
        )
    );
}
