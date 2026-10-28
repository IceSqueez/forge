use std::sync::Arc;

use forge_events::{Event, EventPublisher, EventSource};
use serde_json::json;

use crate::audio_event::{AudioEvent, AudioEventSink};

pub const PLAY_ID_KEY: &str = "play_id";

pub struct BusAudioEventSink {
    bus: Arc<dyn EventPublisher>,
}

impl BusAudioEventSink {
    pub fn new(bus: Arc<dyn EventPublisher>) -> Self {
        Self { bus }
    }
}

impl AudioEventSink for BusAudioEventSink {
    fn emit(&self, event: AudioEvent) {
        let bus_event = match event {
            AudioEvent::PlaybackStarted {
                play_id,
                clip_id,
                clip_label,
                device,
                duration_secs,
                looped,
            } => Event::new(
                EventSource::Audio,
                "playback.started",
                json!({
                    PLAY_ID_KEY: play_id,
                    "clip_id": clip_id.map(|id| id.to_string()),
                    "clip_label": clip_label,
                    "device": device,
                    "duration_secs": duration_secs,
                    "looped": looped,
                }),
            ),
            AudioEvent::PlaybackFinished {
                play_id,
                clip_id,
                clip_label,
            } => Event::new(
                EventSource::Audio,
                "playback.finished",
                json!({
                    PLAY_ID_KEY: play_id,
                    "clip_id": clip_id.map(|id| id.to_string()),
                    "clip_label": clip_label,
                }),
            ),
            AudioEvent::PlaybackFailed {
                play_id,
                clip_id,
                clip_label,
                error,
            } => Event::new(
                EventSource::Audio,
                "playback.failed",
                json!({
                    PLAY_ID_KEY: play_id,
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
mod tests {
    use std::sync::Mutex;

    use forge_types::ClipId;

    use super::*;

    #[derive(Default)]
    struct RecordingPublisher(Mutex<Vec<Event>>);

    impl EventPublisher for RecordingPublisher {
        fn publish(&self, event: Event) {
            self.0.lock().unwrap().push(event);
        }
    }

    #[test]
    fn each_playback_event_publishes_its_audio_kind_and_payload() {
        let clip_id = ClipId::new();
        let cases = [
            (
                AudioEvent::PlaybackStarted {
                    play_id: 7,
                    clip_id: Some(clip_id),
                    clip_label: Some("Air Horn".to_owned()),
                    device: "default".to_owned(),
                    duration_secs: Some(1.5),
                    looped: true,
                },
                "playback.started",
                json!({
                    "play_id": 7,
                    "clip_id": clip_id.to_string(),
                    "clip_label": "Air Horn",
                    "device": "default",
                    "duration_secs": 1.5,
                    "looped": true,
                }),
            ),
            (
                AudioEvent::PlaybackFinished {
                    play_id: 8,
                    clip_id: Some(clip_id),
                    clip_label: None,
                },
                "playback.finished",
                json!({ "play_id": 8, "clip_id": clip_id.to_string(), "clip_label": null }),
            ),
            (
                AudioEvent::PlaybackFailed {
                    play_id: 9,
                    clip_id: None,
                    clip_label: None,
                    error: "device not found".to_owned(),
                },
                "playback.failed",
                json!({ "play_id": 9, "clip_id": null, "clip_label": null, "error": "device not found" }),
            ),
        ];

        for (audio_event, kind, payload) in cases {
            let publisher = Arc::new(RecordingPublisher::default());
            let sink = BusAudioEventSink::new(Arc::clone(&publisher) as Arc<dyn EventPublisher>);

            sink.emit(audio_event);

            let published = publisher.0.lock().unwrap();
            let [event] = published.as_slice() else {
                panic!(
                    "{kind}: expected exactly one event, got {}",
                    published.len()
                );
            };
            assert_eq!(
                (&event.source, event.kind.as_str(), &event.payload),
                (&EventSource::Audio, kind, &payload),
                "{kind}"
            );
        }
    }
}
