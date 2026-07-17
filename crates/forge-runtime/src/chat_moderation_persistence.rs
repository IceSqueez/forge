use std::sync::Arc;
use std::time::Duration;

use forge_storage::ChatHistoryRepo;
use forge_types::{ChatModerationAction, ChatSource};
use futures_util::{StreamExt, pin_mut};

use crate::bus::EventBus;
use crate::chat_moderation_stream::chat_moderation_stream;

const MODERATION_OP_TIMEOUT: Duration = Duration::from_secs(5);

pub fn spawn_chat_moderation_persistence(bus: Arc<EventBus>, repo: Arc<dyn ChatHistoryRepo>) {
    tokio::spawn(run(bus, repo));
}

async fn run(bus: Arc<EventBus>, repo: Arc<dyn ChatHistoryRepo>) {
    let stream = chat_moderation_stream(bus);
    pin_mut!(stream);
    while let Some((source, action)) = stream.next().await {
        apply(repo.as_ref(), source, action).await;
    }
}

async fn apply(repo: &dyn ChatHistoryRepo, source: ChatSource, action: ChatModerationAction) {
    let outcome = match &action {
        ChatModerationAction::DeleteMessage { message_id } => {
            tokio::time::timeout(MODERATION_OP_TIMEOUT, repo.mark_message_deleted(message_id)).await
        }
        ChatModerationAction::RemoveUser { user_name, timeout } => {
            tokio::time::timeout(
                MODERATION_OP_TIMEOUT,
                repo.mark_user_messages_moderated(source, user_name, *timeout),
            )
            .await
        }
        ChatModerationAction::ClearChat => {
            tokio::time::timeout(MODERATION_OP_TIMEOUT, repo.clear_platform(source)).await
        }
    };
    match outcome {
        Ok(Ok(_)) => {}
        Ok(Err(e)) => tracing::warn!(error = %e, "chat moderation persistence failed"),
        Err(_) => tracing::warn!("chat moderation persistence timed out"),
    }
}
