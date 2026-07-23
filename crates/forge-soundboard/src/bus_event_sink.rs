use std::sync::Arc;

use forge_audio::{AudioEvent, AudioEventSink};
use forge_events::{Event, EventSource};
use forge_runtime::EventBus;
use serde_json::json;

pub struct BusAudioEventSink {
    bus: Arc<EventBus>,
}

impl BusAudioEventSink {
    pub fn new(bus: Arc<EventBus>) -> Self {
        Self { bus }
    }
}

impl AudioEventSink for BusAudioEventSink {
    fn emit(&self, event: AudioEvent) {
        let bus_event = match event {
            AudioEvent::PlaybackStarted {
                clip_id,
                clip_label,
                device,
                duration_secs,
                looped,
            } => Event::new(
                EventSource::Audio,
                "playback.started",
                json!({
                    "clip_id": clip_id.map(|id| id.to_string()),
                    "clip_label": clip_label,
                    "device": device,
                    "duration_secs": duration_secs,
                    "looped": looped,
                }),
            ),
            AudioEvent::PlaybackFinished {
                clip_id,
                clip_label,
            } => Event::new(
                EventSource::Audio,
                "playback.finished",
                json!({
                    "clip_id": clip_id.map(|id| id.to_string()),
                    "clip_label": clip_label,
                }),
            ),
            AudioEvent::PlaybackFailed {
                clip_id,
                clip_label,
                error,
            } => Event::new(
                EventSource::Audio,
                "playback.failed",
                json!({
                    "clip_id": clip_id.map(|id| id.to_string()),
                    "clip_label": clip_label,
                    "error": error,
                }),
            ),
        };
        self.bus.publish(bus_event);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::time::Duration;

    use forge_runtime::NullEventLogRepo;
    use forge_types::ClipId;

    use super::*;

    #[tokio::test]
    async fn playback_started_publishes_audio_event_to_bus() {
        let bus = Arc::new(EventBus::new(Arc::new(NullEventLogRepo)));
        let mut sub = bus.subscribe();
        let sink = BusAudioEventSink::new(Arc::clone(&bus));
        let clip_id = ClipId::new();

        sink.emit(AudioEvent::PlaybackStarted {
            clip_id: Some(clip_id),
            clip_label: Some("Air Horn".to_string()),
            device: "default".to_string(),
            duration_secs: Some(1.5),
            looped: false,
        });

        let event = tokio::time::timeout(Duration::from_millis(100), sub.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(event.kind, "playback.started");
        assert_eq!(event.source, EventSource::Audio);
        assert_eq!(
            event.payload["clip_id"].as_str(),
            Some(clip_id.to_string().as_str())
        );
        assert_eq!(event.payload["clip_label"].as_str(), Some("Air Horn"));
        assert_eq!(event.payload["device"].as_str(), Some("default"));
    }

    #[tokio::test]
    async fn playback_finished_publishes_audio_event_to_bus() {
        let bus = Arc::new(EventBus::new(Arc::new(NullEventLogRepo)));
        let mut sub = bus.subscribe();
        let sink = BusAudioEventSink::new(Arc::clone(&bus));
        let clip_id = ClipId::new();

        sink.emit(AudioEvent::PlaybackFinished {
            clip_id: Some(clip_id),
            clip_label: None,
        });

        let event = tokio::time::timeout(Duration::from_millis(100), sub.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(event.kind, "playback.finished");
        assert_eq!(event.source, EventSource::Audio);
    }

    #[tokio::test]
    async fn playback_failed_publishes_error_payload() {
        let bus = Arc::new(EventBus::new(Arc::new(NullEventLogRepo)));
        let mut sub = bus.subscribe();
        let sink = BusAudioEventSink::new(Arc::clone(&bus));

        sink.emit(AudioEvent::PlaybackFailed {
            clip_id: None,
            clip_label: None,
            error: "device not found".to_string(),
        });

        let event = tokio::time::timeout(Duration::from_millis(100), sub.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(event.kind, "playback.failed");
        assert_eq!(event.payload["error"].as_str(), Some("device not found"));
        assert!(event.payload["clip_id"].is_null());
    }
}
