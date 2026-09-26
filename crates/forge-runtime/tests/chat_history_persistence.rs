#![allow(clippy::unwrap_used)]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use forge_events::{Event, EventSource};
use forge_runtime::{EventBus, NullEventLogRepo, spawn_chat_history_persistence};
use forge_storage::chat_history::MockChatHistoryRepo;
use forge_storage::settings::MockSettingsRepo;
use forge_storage::{ChatHistoryRepo, SettingsRepo, StorageError};
use forge_types::{
    ChatModerationAction, ChatModerationPayload, ChatPayload, ChatSource, ModerationMarks,
};
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
        serde_json::json!({ (ChatPayload::KEY): serde_json::to_value(&payload).unwrap() }),
    )
}

fn mod_event(source: EventSource, action: ChatModerationAction) -> Event {
    let payload = ChatModerationPayload { action };
    Event::new(
        source,
        "chat.moderation",
        serde_json::json!({ (ChatModerationPayload::KEY): serde_json::to_value(&payload).unwrap() }),
    )
}

fn no_settings() -> Arc<dyn SettingsRepo> {
    Arc::new(MockSettingsRepo::new())
}

#[tokio::test]
async fn a_failed_batch_is_counted_unwritten_and_the_next_batch_is_still_written() {
    let bus = bus();
    let (tx, mut rx) = mpsc::unbounded_channel::<Vec<String>>();
    let calls = Arc::new(AtomicUsize::new(0));

    let mut repo = MockChatHistoryRepo::new();
    repo.expect_append_batch().times(2).returning(move |rows| {
        let nth = calls.fetch_add(1, Ordering::SeqCst);
        let _ = tx.send(rows.iter().map(|row| row.id.clone()).collect());
        if nth == 0 {
            Err(StorageError::NotFound {
                key: "boom".to_string(),
            })
        } else {
            Ok(())
        }
    });

    spawn_chat_history_persistence(Arc::clone(&bus), Arc::new(repo), no_settings());

    bus.publish(chat_event("first"));
    let first = timeout(RECV_TIMEOUT, rx.recv()).await.unwrap().unwrap();
    bus.publish(chat_event("second"));
    let second = timeout(RECV_TIMEOUT, rx.recv()).await.unwrap().unwrap();

    let unwritten: u64 = bus
        .loss_report()
        .iter()
        .map(|entry| entry.loss.unwritten)
        .sum();
    assert_eq!(
        (first, second, unwritten),
        (vec!["first".to_string()], vec!["second".to_string()], 1)
    );
}

#[tokio::test]
async fn prune_runs_on_cadence_with_a_freshly_read_store_limit() {
    let bus = bus();
    let (prune_tx, mut prune_rx) = mpsc::unbounded_channel::<usize>();

    let mut repo = MockChatHistoryRepo::new();
    repo.expect_append_batch().returning(|_| Ok(()));
    repo.expect_prune_to_limit()
        .times(1..)
        .returning(move |max_rows| {
            let _ = prune_tx.send(max_rows);
            Ok(0)
        });

    let mut settings = MockSettingsRepo::new();
    settings
        .expect_get_string()
        .returning(|_| Ok(Some("1234".to_string())));

    spawn_chat_history_persistence(Arc::clone(&bus), Arc::new(repo), Arc::new(settings));

    for i in 0..256 {
        bus.publish(chat_event(&format!("msg-{i}")));
    }

    let pruned_to = timeout(RECV_TIMEOUT, prune_rx.recv())
        .await
        .unwrap()
        .unwrap();

    assert_eq!(pruned_to, 1234);
}

#[derive(Debug, PartialEq)]
enum Call {
    Delete(String),
    User(ChatSource, String, bool),
    Clear(ChatSource),
}

#[tokio::test]
async fn each_moderation_action_routes_to_its_repo_method_with_its_source() {
    let bus = bus();
    let (tx, mut rx) = mpsc::unbounded_channel::<Call>();

    let mut repo = MockChatHistoryRepo::new();
    let del_tx = tx.clone();
    repo.expect_mark_message_deleted()
        .times(1)
        .returning(move |id| {
            let _ = del_tx.send(Call::Delete(id.to_string()));
            Ok(1)
        });
    let user_tx = tx.clone();
    repo.expect_mark_user_messages_moderated()
        .times(1)
        .returning(move |source, author, timeout| {
            let _ = user_tx.send(Call::User(source, author.to_string(), timeout));
            Ok(1)
        });
    repo.expect_clear_platform()
        .times(1)
        .returning(move |source| {
            let _ = tx.send(Call::Clear(source));
            Ok(1)
        });
    let repo: Arc<dyn ChatHistoryRepo> = Arc::new(repo);

    spawn_chat_history_persistence(Arc::clone(&bus), repo, no_settings());

    bus.publish(mod_event(
        EventSource::Twitch,
        ChatModerationAction::DeleteMessage {
            message_id: "abc".to_string(),
        },
    ));
    bus.publish(mod_event(
        EventSource::Twitch,
        ChatModerationAction::RemoveUser {
            user_name: "bob".to_string(),
            timeout: true,
        },
    ));
    bus.publish(mod_event(
        EventSource::Kick,
        ChatModerationAction::ClearChat,
    ));

    let mut calls = Vec::new();
    for _ in 0..3 {
        calls.push(timeout(RECV_TIMEOUT, rx.recv()).await.unwrap().unwrap());
    }

    assert_eq!(
        calls,
        vec![
            Call::Delete("abc".to_string()),
            Call::User(ChatSource::Twitch, "bob".to_string(), true),
            Call::Clear(ChatSource::Kick),
        ]
    );
}

struct StuckEventLog;

#[async_trait::async_trait]
impl forge_storage::EventLogRepo for StuckEventLog {
    async fn insert(&self, _: &Event) -> Result<(), StorageError> {
        std::future::pending().await
    }

    async fn get(&self, _: forge_types::EventId) -> Result<Option<Event>, StorageError> {
        Ok(None)
    }

    async fn recent(&self, _: usize) -> Result<Vec<Event>, StorageError> {
        Ok(Vec::new())
    }

    async fn recent_since(
        &self,
        _: usize,
        _: Option<forge_types::EventId>,
    ) -> Result<Vec<Event>, StorageError> {
        Ok(Vec::new())
    }

    async fn prune_before(&self, _: time::OffsetDateTime) -> Result<u64, StorageError> {
        Ok(0)
    }
}

struct StuckChatHistory;

#[async_trait::async_trait]
impl ChatHistoryRepo for StuckChatHistory {
    async fn append(&self, _: &forge_types::UnifiedChatRow) -> Result<(), StorageError> {
        std::future::pending().await
    }

    async fn list_recent(
        &self,
        _: usize,
    ) -> Result<Vec<forge_types::UnifiedChatRow>, StorageError> {
        Ok(Vec::new())
    }

    async fn prune_to_limit(&self, _: usize) -> Result<u64, StorageError> {
        Ok(0)
    }

    async fn mark_message_deleted(&self, _: &str) -> Result<u64, StorageError> {
        Ok(0)
    }

    async fn mark_user_messages_moderated(
        &self,
        _: ChatSource,
        _: &str,
        _: bool,
    ) -> Result<u64, StorageError> {
        Ok(0)
    }

    async fn clear_platform(&self, _: ChatSource) -> Result<u64, StorageError> {
        Ok(0)
    }
}

#[tokio::test]
async fn abandoning_the_flush_returns_the_rows_every_stuck_writer_gave_up_in_total() {
    let bus = EventBus::new(Arc::new(StuckEventLog));
    EventBus::spawn_flush_task(Arc::clone(&bus));
    spawn_chat_history_persistence(Arc::clone(&bus), Arc::new(StuckChatHistory), no_settings());
    for id in ["a", "b", "c"] {
        bus.publish(chat_event(id));
    }

    bus.shutdown();
    let given_up = timeout(RECV_TIMEOUT, bus.abandon_flush()).await.unwrap();

    assert_eq!(given_up, 6, "3 event_log rows + 3 chat rows");
}
