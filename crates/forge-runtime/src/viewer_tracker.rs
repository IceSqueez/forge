use std::sync::Arc;

use forge_events::EventSource;
use forge_storage::{ViewerPlatform, ViewerRepo};
use tokio::sync::broadcast;

use crate::bus::EventBus;

pub fn spawn_viewer_tracker(bus: Arc<EventBus>, repo: Arc<dyn ViewerRepo>) {
    tokio::spawn(run(bus, repo));
}

async fn run(bus: Arc<EventBus>, repo: Arc<dyn ViewerRepo>) {
    let mut rx = bus.subscribe().into_receiver();
    loop {
        match rx.recv().await {
            Ok(event) => {
                if !matches!(
                    event.kind.as_str(),
                    "twitch.channel.chat.message"
                        | "youtube.chat.message"
                        | "kick.chat.message.sent"
                ) {
                    continue;
                }
                let Some(platform) = map_source_to_platform(event.source) else {
                    continue;
                };
                let Some(user) = event.payload.get("user") else {
                    continue;
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
            Err(broadcast::error::RecvError::Lagged(n)) => {
                tracing::warn!(missed = n, "viewer tracker event subscriber lagged");
            }
            Err(broadcast::error::RecvError::Closed) => break,
        }
    }
}

fn map_source_to_platform(source: EventSource) -> Option<ViewerPlatform> {
    match source {
        EventSource::Twitch => Some(ViewerPlatform::Twitch),
        EventSource::YouTube => Some(ViewerPlatform::YouTube),
        EventSource::Kick => Some(ViewerPlatform::Kick),
        EventSource::Core
        | EventSource::Rhai
        | EventSource::Http
        | EventSource::Obs
        | EventSource::VTube
        | EventSource::Discord
        | EventSource::Midi
        | EventSource::Hotkey
        | EventSource::Timer
        | EventSource::Server
        | EventSource::Audio => None,
    }
}
