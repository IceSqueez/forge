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

fn user_message(source: EventSource, kind: &str, id: &str, login: &str) -> Event {
    chat_event(
        source,
        kind,
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
async fn records_chat_activity_for_each_live_platforms_real_chat_kind() {
    let bus = bus();
    let (tx, mut rx) = mpsc::unbounded_channel::<Recorded>();

    spawn_viewer_tracker(Arc::clone(&bus), Arc::new(tracker_recording_into(tx)));
    wait_until_subscribed().await;

    let cases: [(EventSource, &str, ViewerPlatform, &str, &str); 3] = [
        (
            EventSource::Twitch,
            "twitch.channel.chat.message",
            ViewerPlatform::Twitch,
            "t1",
            "alice",
        ),
        (
            EventSource::YouTube,
            "youtube.chat.message",
            ViewerPlatform::YouTube,
            "y1",
            "bob",
        ),
        (
            EventSource::Kick,
            "kick.chat.message.sent",
            ViewerPlatform::Kick,
            "k1",
            "carol",
        ),
    ];

    let mut expected: Vec<Recorded> = Vec::new();
    for (source, kind, platform, id, login) in cases {
        bus.publish(user_message(source, kind, id, login));
        expected.push((platform, id.to_string(), login.to_string()));
    }

    let mut got = Vec::new();
    for _ in 0..expected.len() {
        got.push(timeout(RECV_TIMEOUT, rx.recv()).await.unwrap().unwrap());
    }

    assert_eq!(got, expected);
}

#[tokio::test]
async fn skips_events_that_are_not_recordable_chat_messages() {
    let bus = bus();
    let (tx, mut rx) = mpsc::unbounded_channel::<Recorded>();

    spawn_viewer_tracker(Arc::clone(&bus), Arc::new(tracker_recording_into(tx)));
    wait_until_subscribed().await;

    let skipped = [
        user_message(
            EventSource::Core,
            "twitch.channel.chat.message",
            "c1",
            "core-user",
        ),
        user_message(EventSource::Twitch, "twitch.chat.whisper", "w1", "whisper"),
        chat_event(
            EventSource::Twitch,
            "twitch.channel.chat.message",
            serde_json::json!({ "text": "hi" }),
        ),
        user_message(
            EventSource::Twitch,
            "twitch.channel.chat.message",
            "",
            "loginonly",
        ),
        user_message(
            EventSource::Twitch,
            "twitch.channel.chat.message",
            "idonly",
            "",
        ),
        chat_event(
            EventSource::Twitch,
            "twitch.channel.chat.message",
            serde_json::json!({ "user": { "login": "no-id" } }),
        ),
        chat_event(
            EventSource::Twitch,
            "twitch.channel.chat.message",
            serde_json::json!({ "user": { "id": "no-login" } }),
        ),
    ];
    for event in skipped {
        bus.publish(event);
    }

    bus.publish(user_message(
        EventSource::Twitch,
        "twitch.channel.chat.message",
        "ok",
        "sentinel",
    ));

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
