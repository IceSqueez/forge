#![allow(clippy::unwrap_used)]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use forge_events::{Event, EventSource};
use forge_runtime::{
    EventBus, NullEventLogRepo, chat_moderation_stream, spawn_chat_moderation_persistence,
};
use forge_storage::chat_history::MockChatHistoryRepo;
use forge_storage::{ChatHistoryRepo, StorageError};
use forge_types::{ChatModerationAction, ChatModerationPayload, ChatSource};
use futures_util::{StreamExt, pin_mut};
use tokio::sync::mpsc;
use tokio::time::{Duration, timeout};

const RECV_TIMEOUT: Duration = Duration::from_secs(5);

fn bus() -> Arc<EventBus> {
    EventBus::new(Arc::new(NullEventLogRepo))
}

fn mod_event(source: EventSource, action: ChatModerationAction) -> Event {
    let payload = ChatModerationPayload { action };
    Event::new(
        source,
        "chat.moderation",
        serde_json::json!({ ChatModerationPayload::KEY: serde_json::to_value(&payload).unwrap() }),
    )
}

async fn wait_until_subscribed() {
    for _ in 0..16 {
        tokio::task::yield_now().await;
    }
}

#[tokio::test]
async fn stream_yields_chat_source_events_and_drops_non_chat_and_keyless() {
    let bus = bus();
    let stream = chat_moderation_stream(Arc::clone(&bus));
    pin_mut!(stream);

    bus.publish(mod_event(
        EventSource::Core,
        ChatModerationAction::ClearChat,
    ));
    bus.publish(Event::new(
        EventSource::Twitch,
        "chat.moderation",
        serde_json::json!({ "unrelated": true }),
    ));
    bus.publish(mod_event(
        EventSource::Kick,
        ChatModerationAction::DeleteMessage {
            message_id: "keep".to_string(),
        },
    ));

    let (source, action) = timeout(RECV_TIMEOUT, stream.next()).await.unwrap().unwrap();

    assert_eq!(source, ChatSource::Kick);
    assert_eq!(
        action,
        ChatModerationAction::DeleteMessage {
            message_id: "keep".to_string(),
        }
    );
}

#[derive(Debug, PartialEq)]
enum Call {
    Delete(String),
    User(ChatSource, String, bool),
    Clear(ChatSource),
}

#[tokio::test]
async fn each_action_routes_to_matching_repo_method_with_source() {
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
    let clear_tx = tx.clone();
    repo.expect_clear_platform()
        .times(1)
        .returning(move |source| {
            let _ = clear_tx.send(Call::Clear(source));
            Ok(1)
        });
    let repo: Arc<dyn ChatHistoryRepo> = Arc::new(repo);

    spawn_chat_moderation_persistence(Arc::clone(&bus), repo);
    wait_until_subscribed().await;

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

#[tokio::test]
async fn repo_error_does_not_terminate_persistence_loop() {
    let bus = bus();
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();
    let calls = Arc::new(AtomicUsize::new(0));

    let mut repo = MockChatHistoryRepo::new();
    repo.expect_mark_message_deleted()
        .times(2)
        .returning(move |id| {
            let nth = calls.fetch_add(1, Ordering::SeqCst);
            let _ = tx.send(id.to_string());
            if nth == 0 {
                Err(StorageError::NotFound {
                    key: "boom".to_string(),
                })
            } else {
                Ok(1)
            }
        });
    let repo: Arc<dyn ChatHistoryRepo> = Arc::new(repo);

    spawn_chat_moderation_persistence(Arc::clone(&bus), repo);
    wait_until_subscribed().await;

    bus.publish(mod_event(
        EventSource::Twitch,
        ChatModerationAction::DeleteMessage {
            message_id: "first".to_string(),
        },
    ));
    bus.publish(mod_event(
        EventSource::Twitch,
        ChatModerationAction::DeleteMessage {
            message_id: "second".to_string(),
        },
    ));

    assert_eq!(
        timeout(RECV_TIMEOUT, rx.recv()).await.unwrap().unwrap(),
        "first"
    );
    assert_eq!(
        timeout(RECV_TIMEOUT, rx.recv()).await.unwrap().unwrap(),
        "second"
    );
}
