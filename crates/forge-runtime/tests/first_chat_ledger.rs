#![allow(clippy::unwrap_used)]

use std::sync::Arc;
use std::time::Duration;

use forge_events::{Event, EventSource};
use forge_runtime::{EventBus, FirstChatLedger, NullEventLogRepo, spawn_viewer_tracker};
use forge_storage::viewer::MockViewerRepo;
use forge_storage::{DataProvider, ViewerRepo};
use forge_storage_sqlite::SqliteBackend;
use forge_types::ChatViewer;
use tokio::time::timeout;

const TEST_KEY: [u8; 32] = [0xab; 32];
const RECV_TIMEOUT: Duration = Duration::from_secs(30);
const CONCURRENT_PUBLISHERS: usize = 32;

fn chat_line(viewer_id: &str) -> Event {
    let mut payload = serde_json::json!({ "message": "hi" });
    ChatViewer::new(viewer_id, viewer_id).attach(&mut payload);
    Event::new(EventSource::Twitch, "twitch.channel.chat.message", payload)
}

fn bus() -> Arc<EventBus> {
    EventBus::new(Arc::new(NullEventLogRepo))
}

async fn empty_history_ledger() -> FirstChatLedger {
    let mut repo = MockViewerRepo::new();
    repo.expect_list().return_once(|| Ok(Vec::new()));
    FirstChatLedger::load(&repo).await.unwrap()
}

async fn published_flags(bus: &EventBus, viewer_ids: &[&str]) -> Vec<bool> {
    let mut sub = bus.subscribe();
    for viewer_id in viewer_ids {
        bus.publish(chat_line(viewer_id));
    }
    let mut flags = Vec::with_capacity(viewer_ids.len());
    for _ in viewer_ids {
        let event = timeout(RECV_TIMEOUT, sub.recv()).await.unwrap().unwrap();
        flags.push(ChatViewer::read(&event.payload).unwrap().first_message);
    }
    flags
}

#[tokio::test]
async fn every_subscriber_and_the_recent_ring_see_the_stamp_made_at_publish() {
    let bus = bus();
    bus.track_first_chats(empty_history_ledger().await);
    let mut sub = bus.subscribe();
    let line = chat_line("alice");
    let id = line.id;
    bus.publish(line);

    let delivered = timeout(RECV_TIMEOUT, sub.recv()).await.unwrap().unwrap();
    let kept = bus.lookup(id).unwrap();
    for event in [delivered, kept] {
        assert!(ChatViewer::read(&event.payload).unwrap().first_message);
    }
}

#[tokio::test]
async fn a_bus_without_a_ledger_flags_nobody_as_a_first_chatter() {
    let bus = bus();
    assert_eq!(
        published_flags(&bus, &["alice", "alice"]).await,
        [false, false]
    );
}

#[tokio::test]
async fn back_to_back_lines_of_one_viewer_from_many_threads_yield_exactly_one_first() {
    let bus = bus();
    bus.track_first_chats(empty_history_ledger().await);
    let mut sub = bus.subscribe();
    let barrier = std::sync::Barrier::new(CONCURRENT_PUBLISHERS);
    std::thread::scope(|scope| {
        for _ in 0..CONCURRENT_PUBLISHERS {
            scope.spawn(|| {
                barrier.wait();
                bus.publish(chat_line("alice"));
            });
        }
    });

    let mut firsts = 0;
    for _ in 0..CONCURRENT_PUBLISHERS {
        let event = timeout(RECV_TIMEOUT, sub.recv()).await.unwrap().unwrap();
        if ChatViewer::read(&event.payload).unwrap().first_message {
            firsts += 1;
        }
    }
    assert_eq!(firsts, 1);
}

#[tokio::test]
async fn a_viewer_greeted_before_a_restart_is_not_first_again_after_it() {
    let media = tempfile::tempdir().unwrap();
    let backend =
        SqliteBackend::open_for_test(":memory:", TEST_KEY, media.path().join("media"), None)
            .await
            .unwrap();
    let viewers: Arc<dyn ViewerRepo> = backend.viewer_repo();

    let before = bus();
    before.track_first_chats(FirstChatLedger::load(viewers.as_ref()).await.unwrap());
    spawn_viewer_tracker(Arc::clone(&before), Arc::clone(&viewers));
    assert_eq!(published_flags(&before, &["alice"]).await, [true]);
    before.shutdown();
    before.await_flush().await;

    let after = bus();
    after.track_first_chats(FirstChatLedger::load(viewers.as_ref()).await.unwrap());
    assert_eq!(
        published_flags(&after, &["alice", "bob"]).await,
        [false, true]
    );
}
