#![allow(clippy::unwrap_used)]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use forge_events::{Event, EventSource};
use forge_runtime::{EventBus, NullEventLogRepo, spawn_chat_history_persistence};
use forge_storage::chat_history::MockChatHistoryRepo;
use forge_storage::settings::MockSettingsRepo;
use forge_storage::{ChatHistoryRepo, SettingsRepo, StorageError};
use forge_types::{ChatPayload, ModerationMarks};
use tokio::sync::mpsc;
use tokio::time::{Duration, timeout};

const RECV_TIMEOUT: Duration = Duration::from_secs(5);

fn bus() -> Arc<EventBus> {
    EventBus::new(Arc::new(NullEventLogRepo))
}

fn chat_event(msg_id: &str) -> Event {
    let payload = ChatPayload {
        platform_msg_id: msg_id.to_string(),
        author: "user".to_string(),
        author_color: None,
        segments: vec![],
        badges: vec![],
        is_event: false,
        event_detail: None,
        moderation: ModerationMarks::default(),
    };
    Event::new(
        EventSource::Twitch,
        "chat.message",
        serde_json::json!({ "_chat": serde_json::to_value(&payload).unwrap() }),
    )
}

async fn wait_until_subscribed() {
    for _ in 0..16 {
        tokio::task::yield_now().await;
    }
}

#[tokio::test]
async fn append_error_does_not_terminate_persistence_loop() {
    let bus = bus();
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();
    let calls = Arc::new(AtomicUsize::new(0));

    let mut repo = MockChatHistoryRepo::new();
    repo.expect_append().times(2).returning(move |row| {
        let nth = calls.fetch_add(1, Ordering::SeqCst);
        let _ = tx.send(row.id.clone());
        if nth == 0 {
            Err(StorageError::NotFound {
                key: "boom".to_string(),
            })
        } else {
            Ok(())
        }
    });
    let repo: Arc<dyn ChatHistoryRepo> = Arc::new(repo);
    let settings: Arc<dyn SettingsRepo> = Arc::new(MockSettingsRepo::new());

    spawn_chat_history_persistence(Arc::clone(&bus), repo, settings);
    wait_until_subscribed().await;

    bus.publish(chat_event("first"));
    bus.publish(chat_event("second"));

    let first = timeout(RECV_TIMEOUT, rx.recv()).await.unwrap().unwrap();
    let second = timeout(RECV_TIMEOUT, rx.recv()).await.unwrap().unwrap();

    assert_eq!(first, "first");
    assert_eq!(second, "second");
}

#[tokio::test]
async fn prune_runs_on_cadence_with_a_freshly_read_store_limit() {
    let bus = bus();
    let (prune_tx, mut prune_rx) = mpsc::unbounded_channel::<usize>();

    let mut repo = MockChatHistoryRepo::new();
    repo.expect_append().returning(|_| Ok(()));
    repo.expect_prune_to_limit()
        .times(1..)
        .returning(move |max_rows| {
            let _ = prune_tx.send(max_rows);
            Ok(0)
        });
    let repo: Arc<dyn ChatHistoryRepo> = Arc::new(repo);

    let mut settings = MockSettingsRepo::new();
    settings
        .expect_get_string()
        .returning(|_| Ok(Some("1234".to_string())));
    let settings: Arc<dyn SettingsRepo> = Arc::new(settings);

    spawn_chat_history_persistence(Arc::clone(&bus), repo, settings);
    wait_until_subscribed().await;

    for i in 0..256 {
        bus.publish(chat_event(&format!("msg-{i}")));
    }

    let pruned_to = timeout(RECV_TIMEOUT, prune_rx.recv())
        .await
        .unwrap()
        .unwrap();

    assert_eq!(pruned_to, 1234);
}
