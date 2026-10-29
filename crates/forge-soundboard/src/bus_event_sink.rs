use std::sync::Arc;

use forge_events::{EventPublisher, EventSource, TypedEvent};

use crate::audio_event::{AudioEvent, AudioEventSink};
use crate::vocabulary::{PlaybackFailed, PlaybackFinished, PlaybackStarted};

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
            } => PlaybackStarted {
                play_id,
                clip_id,
                clip_label,
                device,
                duration_secs,
                looped,
            }
            .into_event(EventSource::Audio, None),
            AudioEvent::PlaybackFinished {
                play_id,
                clip_id,
                clip_label,
            } => PlaybackFinished {
                play_id,
                clip_id,
                clip_label,
            }
            .into_event(EventSource::Audio, None),
            AudioEvent::PlaybackFailed {
                play_id,
                clip_id,
                clip_label,
                error,
            } => PlaybackFailed {
                play_id,
                clip_id,
                clip_label,
                error,
            }
            .into_event(EventSource::Audio, None),
        };
        self.bus.publish(bus_event);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use forge_events::Event;
    use forge_types::ClipId;
    use serde_json::json;

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
