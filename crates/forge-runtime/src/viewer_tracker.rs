use std::sync::Arc;

use forge_events::EventSource;
use forge_storage::{ViewerPlatform, ViewerRepo};

use crate::bus::EventBus;
use crate::delivery::VIEWER_TRACKER;

pub fn spawn_viewer_tracker(bus: Arc<EventBus>, repo: Arc<dyn ViewerRepo>) {
    let subscription = bus.subscribe_critical(VIEWER_TRACKER);
    tokio::spawn(async move {
        let mut subscription = subscription;
        while let Some(event) = subscription.recv().await {
            if !matches!(
                event.kind.as_str(),
                "twitch.channel.chat.message" | "youtube.chat.message" | "kick.chat.message.sent"
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
    });
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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::time::Duration;

    use forge_events::Event;
    use forge_storage::viewer::MockViewerRepo;
    use tokio::sync::mpsc;

    use super::*;
    use crate::{Config, NullEventLogRepo};

    const OBSERVER_CAPACITY: usize = 16;
    const BURST: usize = OBSERVER_CAPACITY * 4;
    const RECORD_TIMEOUT: Duration = Duration::from_millis(500);

    fn chat_message(login: &str) -> Event {
        Event::new(
            EventSource::Twitch,
            "twitch.channel.chat.message",
            serde_json::json!({ "user": { "id": login, "login": login } }),
        )
    }

    #[tokio::test]
    async fn the_tracker_records_every_message_of_a_burst_larger_than_the_observer_buffer() {
        let (recorded_tx, mut recorded) = mpsc::unbounded_channel();
        let mut repo = MockViewerRepo::new();
        repo.expect_record_message()
            .returning(move |_, _, username| {
                let _ = recorded_tx.send(username.to_owned());
                Ok(())
            });
        let config = Config {
            bus_observer_capacity: OBSERVER_CAPACITY,
            ..Config::default()
        };
        let bus = EventBus::with_config(Arc::new(NullEventLogRepo), &config);
        spawn_viewer_tracker(Arc::clone(&bus), Arc::new(repo));

        // Publishing never yields, so the whole burst lands before the tracker drains anything.
        let logins: Vec<String> = (0..BURST).map(|i| format!("viewer-{i}")).collect();
        for login in &logins {
            bus.publish(chat_message(login));
        }

        let mut seen = Vec::with_capacity(BURST);
        while seen.len() < BURST {
            match tokio::time::timeout(RECORD_TIMEOUT, recorded.recv()).await {
                Ok(Some(login)) => seen.push(login),
                _ => break,
            }
        }
        assert_eq!(
            seen, logins,
            "the tracker is a lossless consumer; a burst must not cost it any viewer"
        );
    }
}
