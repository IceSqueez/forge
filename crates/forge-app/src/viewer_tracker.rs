use std::sync::Arc;

use forge_events::{EventSource, EventsError};
use forge_runtime::EventBus;
use forge_storage::{ViewerPlatform, ViewerRepo};

/// Spawns a tokio task that listens for `chat.message` events from chat platforms
/// and upserts a row in the `viewers` table per message.
pub fn spawn(bus: Arc<EventBus>, repo: Arc<dyn ViewerRepo>) -> tokio::task::JoinHandle<()> {
    let mut sub = bus.subscribe();
    tokio::spawn(async move {
        loop {
            match sub.recv().await {
                Ok(event) => {
                    if event.kind != "chat.message" {
                        continue;
                    }
                    let Some(platform) = map_source_to_platform(event.source) else {
                        continue;
                    };
                    let user = match event.payload.get("user") {
                        Some(u) => u,
                        None => continue,
                    };
                    let viewer_id = user.get("id").and_then(|v| v.as_str()).unwrap_or_default();
                    let username = user
                        .get("login")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default();
                    if viewer_id.is_empty() || username.is_empty() {
                        continue;
                    }
                    if let Err(e) = repo.record_message(platform, viewer_id, username).await {
                        tracing::warn!(error = %e, "viewer tracker record_message failed");
                    }
                }
                Err(EventsError::BusClosed) => break,
                Err(EventsError::LaggingReceiver) => {
                    tracing::warn!("viewer tracker event subscriber lagged");
                }
                Err(EventsError::ReplayMiss(_)) => continue,
            }
        }
    })
}

fn map_source_to_platform(source: EventSource) -> Option<ViewerPlatform> {
    match source {
        EventSource::Twitch => Some(ViewerPlatform::Twitch),
        EventSource::YouTube => Some(ViewerPlatform::YouTube),
        EventSource::Kick => Some(ViewerPlatform::Kick),
        EventSource::Trovo => Some(ViewerPlatform::Trovo),
        _ => None,
    }
}
