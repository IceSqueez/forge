use std::sync::Arc;
use std::time::Duration;

use forge_storage::{ChatHistoryRepo, SettingsRepo, chat_history_store_limit};
use futures_util::{StreamExt, pin_mut};

use crate::bus::EventBus;
use crate::chat_stream::chat_stream;

const PERSIST_OP_TIMEOUT: Duration = Duration::from_secs(5);
const PRUNE_EVERY_APPENDS: u64 = 256;

pub fn spawn_chat_history_persistence(
    bus: Arc<EventBus>,
    repo: Arc<dyn ChatHistoryRepo>,
    settings: Arc<dyn SettingsRepo>,
) {
    tokio::spawn(run(bus, repo, settings));
}

async fn run(bus: Arc<EventBus>, repo: Arc<dyn ChatHistoryRepo>, settings: Arc<dyn SettingsRepo>) {
    let stream = chat_stream(bus);
    pin_mut!(stream);

    let mut appended: u64 = 0;
    while let Some(row) = stream.next().await {
        match tokio::time::timeout(PERSIST_OP_TIMEOUT, repo.append(&row)).await {
            Ok(Ok(())) => appended += 1,
            Ok(Err(e)) => {
                tracing::warn!(error = %e, "chat history append failed");
                continue;
            }
            Err(_) => {
                tracing::warn!("chat history append timed out");
                continue;
            }
        }

        if appended.is_multiple_of(PRUNE_EVERY_APPENDS) {
            prune(repo.as_ref(), settings.as_ref()).await;
        }
    }
}

async fn prune(repo: &dyn ChatHistoryRepo, settings: &dyn SettingsRepo) {
    let limit = match chat_history_store_limit(settings).await {
        Ok(limit) => limit as usize,
        Err(e) => {
            tracing::warn!(error = %e, "reading chat history store limit failed; skipping prune");
            return;
        }
    };
    match tokio::time::timeout(PERSIST_OP_TIMEOUT, repo.prune_to_limit(limit)).await {
        Ok(Ok(_)) => {}
        Ok(Err(e)) => tracing::warn!(error = %e, "chat history prune failed"),
        Err(_) => tracing::warn!("chat history prune timed out"),
    }
}
