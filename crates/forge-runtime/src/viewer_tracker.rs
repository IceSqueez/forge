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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::time::Duration;

    use forge_events::Event;
    use forge_storage::viewer::MockViewerRepo;
    use tokio::sync::mpsc;

    use super::*;
    use crate::NullEventLogRepo;

    const LAG_CHANNEL_CAP: usize = 16;
    const LAG_RING_CAP: usize = 64;
    const LAG_BURST: usize = LAG_CHANNEL_CAP * 2;
    const WARMUP_LOGIN: &str = "before-the-gap";
    const AFTER_GAP_LOGIN: &str = "after-the-gap";
    const RECORD_TIMEOUT: Duration = Duration::from_millis(200);

    fn chat_message(login: &str) -> Event {
        Event::new(
            EventSource::Twitch,
            "twitch.channel.chat.message",
            serde_json::json!({ "user": { "id": login, "login": login } }),
        )
    }

    #[tokio::test]
    async fn the_tracker_keeps_recording_after_its_subscription_lags() {
        let (recorded_tx, mut recorded) = mpsc::unbounded_channel();
        let mut repo = MockViewerRepo::new();
        repo.expect_record_message()
            .returning(move |_, _, username| {
                let _ = recorded_tx.send(username.to_owned());
                Ok(())
            });

        let bus = EventBus::with_caps(Arc::new(NullEventLogRepo), LAG_CHANNEL_CAP, LAG_RING_CAP);
        spawn_viewer_tracker(Arc::clone(&bus), Arc::new(repo));

        // The task subscribes on its first poll, and publishing never yields; the warm-up's
        // arrival then proves it is subscribed and parked, so the burst that follows lands
        // with nobody draining it.
        tokio::task::yield_now().await;
        bus.publish(chat_message(WARMUP_LOGIN));
        assert_eq!(
            tokio::time::timeout(RECORD_TIMEOUT, recorded.recv())
                .await
                .unwrap(),
            Some(WARMUP_LOGIN.to_owned())
        );

        for i in 0..LAG_BURST {
            bus.publish(Event::new(
                EventSource::Core,
                "core.filler",
                serde_json::json!({ "i": i }),
            ));
        }
        bus.publish(chat_message(AFTER_GAP_LOGIN));

        assert_eq!(
            tokio::time::timeout(RECORD_TIMEOUT, recorded.recv())
                .await
                .unwrap(),
            Some(AFTER_GAP_LOGIN.to_owned()),
            "one lag must not retire viewer tracking for the rest of the process"
        );
    }
}
