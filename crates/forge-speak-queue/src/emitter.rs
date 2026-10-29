use forge_events::{EventPublisher, EventSource, TypedEvent};
use forge_speech_types::{EngineId, LanguageCode, VoiceId};
use tokio::sync::broadcast;

use crate::vocabulary::{
    FailureReason, HoldReason, RejectReason, SkipReason, SpeakCleared, SpeakEnqueued, SpeakFailed,
    SpeakFinished, SpeakPaused, SpeakRejected, SpeakRemoved, SpeakReordered, SpeakResumed,
    SpeakSkipped, SpeakStarted,
};
use crate::{Priority, RequestId, SpeakEvent, SpeakRequest};

const USER_PAUSED: &str = "user paused";

pub(crate) struct DetectedLanguage {
    pub(crate) code: LanguageCode,
    pub(crate) confidence: f64,
}

pub(crate) struct ResolvedPlayback<'a> {
    pub(crate) voice_id: &'a VoiceId,
    pub(crate) engine_id: &'a EngineId,
    pub(crate) duration_secs: u32,
    pub(crate) language: Option<&'a DetectedLanguage>,
}

pub(crate) struct EnqueuePreview {
    pub(crate) voice_preview: String,
    pub(crate) estimated_secs: u32,
}

pub(crate) enum Outcome {
    Finished,
    Skipped {
        reason: SkipReason,
        detail: Option<String>,
    },
    Failed {
        reason: FailureReason,
        error: String,
    },
    Removed,
    Rejected {
        reason: RejectReason,
        limit: usize,
        queue_len: usize,
    },
}

pub(crate) struct SpeakEmitter<'a> {
    feed: &'a broadcast::Sender<SpeakEvent>,
    bus: &'a dyn EventPublisher,
}

impl<'a> SpeakEmitter<'a> {
    pub(crate) fn new(
        feed: &'a broadcast::Sender<SpeakEvent>,
        bus: &'a dyn EventPublisher,
    ) -> Self {
        Self { feed, bus }
    }

    #[expect(
        clippy::let_underscore_must_use,
        reason = "having no subscribers is a normal state"
    )]
    pub(crate) fn feed(&self, event: SpeakEvent) {
        let _ = self.feed.send(event);
    }

    fn publish(&self, event: impl TypedEvent, caused_by: Option<forge_types::EventId>) {
        event.publish(self.bus, EventSource::Audio, caused_by);
    }

    pub(crate) fn outcome(&self, request: &SpeakRequest, outcome: Outcome) {
        let request_id = request.request_id.clone();
        let viewer_name = request.viewer_name.clone();
        let text = request.text.clone();
        let caused_by = request.source_event_id;
        match outcome {
            Outcome::Finished => {
                self.feed(SpeakEvent::Finished {
                    request_id: request_id.clone(),
                });
                self.publish(
                    SpeakFinished {
                        request_id,
                        viewer_name,
                        text,
                    },
                    caused_by,
                );
            }
            Outcome::Skipped { reason, detail } => {
                self.skipped(
                    request_id,
                    reason,
                    detail,
                    Some(viewer_name),
                    Some(text),
                    caused_by,
                );
            }
            Outcome::Failed { reason, error } => {
                self.feed(SpeakEvent::Failed {
                    request_id: request_id.clone(),
                    error: error.clone(),
                });
                self.publish(
                    SpeakFailed {
                        request_id,
                        reason,
                        error,
                        viewer_name,
                        text,
                    },
                    caused_by,
                );
            }
            Outcome::Removed => {
                self.feed(SpeakEvent::Removed {
                    request_id: request_id.clone(),
                });
                self.publish(
                    SpeakRemoved {
                        request_id,
                        viewer_name,
                        text,
                    },
                    caused_by,
                );
            }
            Outcome::Rejected {
                reason,
                limit,
                queue_len,
            } => {
                self.feed(SpeakEvent::Rejected {
                    request_id: request_id.clone(),
                    reason: reason.describe(limit),
                });
                self.publish(
                    SpeakRejected {
                        request_id,
                        reason,
                        limit,
                        queue_len,
                        viewer_name,
                        text,
                    },
                    caused_by,
                );
            }
        }
    }

    pub(crate) fn skipped_in_flight(&self, request_id: RequestId, reason: SkipReason) {
        self.skipped(request_id, reason, None, None, None, None);
    }

    fn skipped(
        &self,
        request_id: RequestId,
        reason: SkipReason,
        detail: Option<String>,
        viewer_name: Option<String>,
        text: Option<String>,
        caused_by: Option<forge_types::EventId>,
    ) {
        let described = match &detail {
            Some(detail) => format!("{}: {detail}", reason.describe()),
            None => reason.describe().to_owned(),
        };
        self.feed(SpeakEvent::Skipped {
            request_id: request_id.clone(),
            reason: described,
        });
        self.publish(
            SpeakSkipped {
                request_id,
                reason,
                detail,
                viewer_name,
                text,
            },
            caused_by,
        );
    }

    pub(crate) fn enqueued(
        &self,
        request: &SpeakRequest,
        queue_len: usize,
        preview: EnqueuePreview,
    ) {
        self.feed(SpeakEvent::Enqueued {
            request_id: request.request_id.clone(),
            queue_len,
            viewer_name: request.viewer_name.clone(),
            text: request.text.clone(),
            is_high_priority: request.priority == Priority::High,
            voice_preview: preview.voice_preview,
            estimated_secs: preview.estimated_secs,
        });
        self.publish(
            SpeakEnqueued {
                request_id: request.request_id.clone(),
                queue_len,
                viewer_name: request.viewer_name.clone(),
                text: request.text.clone(),
            },
            request.source_event_id,
        );
    }

    pub(crate) fn started(
        &self,
        request: &SpeakRequest,
        playback: Option<ResolvedPlayback<'_>>,
        queue_len: usize,
    ) {
        let language = playback.as_ref().and_then(|p| p.language);
        self.feed(SpeakEvent::Started {
            request_id: request.request_id.clone(),
            voice_id: playback
                .as_ref()
                .map_or_else(|| VoiceId(String::new()), |p| p.voice_id.clone()),
            engine_id: playback
                .as_ref()
                .map_or_else(|| EngineId(String::new()), |p| p.engine_id.clone()),
            viewer_name: request.viewer_name.clone(),
            text: request.text.clone(),
            duration_secs: playback.as_ref().map_or(0, |p| p.duration_secs),
        });
        self.publish(
            SpeakStarted {
                request_id: request.request_id.clone(),
                voice_id: playback.as_ref().map(|p| p.voice_id.clone()),
                engine_id: playback.as_ref().map(|p| p.engine_id.clone()),
                queue_len,
                viewer_name: request.viewer_name.clone(),
                text: request.text.clone(),
                detected_language: language.map(|l| l.code.to_string()),
                language_confidence: language.map(|l| l.confidence),
            },
            request.source_event_id,
        );
    }

    pub(crate) fn reordered(&self, request: &SpeakRequest, before: Option<RequestId>) {
        self.publish(
            SpeakReordered {
                request_id: request.request_id.clone(),
                before,
                viewer_name: request.viewer_name.clone(),
                text: request.text.clone(),
            },
            request.source_event_id,
        );
    }

    pub(crate) fn cleared(&self, keep_current: bool) {
        if !keep_current {
            self.feed(SpeakEvent::Cleared);
        }
        self.publish(SpeakCleared { keep_current }, None);
    }

    pub(crate) fn paused(&self, reason: HoldReason) {
        self.feed(match reason {
            HoldReason::User => SpeakEvent::Paused {
                reason: USER_PAUSED.to_owned(),
            },
            HoldReason::VoiceGate => SpeakEvent::VoiceGateHeld,
        });
        self.publish(SpeakPaused { reason }, None);
    }

    pub(crate) fn resumed(&self, reason: HoldReason) {
        self.feed(match reason {
            HoldReason::User => SpeakEvent::Resumed,
            HoldReason::VoiceGate => SpeakEvent::VoiceGateReleased,
        });
        self.publish(SpeakResumed { reason }, None);
    }
}

pub(crate) fn pipeline_skip(
    reason: &forge_tts_pipeline::SkipReason,
) -> (SkipReason, Option<String>) {
    use forge_tts_pipeline::SkipReason as PipelineSkip;
    match reason {
        PipelineSkip::MatchedSkipRule(name) => {
            (SkipReason::SkipRuleMatched, Some((*name).to_owned()))
        }
        PipelineSkip::BlockedByWordFilter => (SkipReason::BlockedByWordFilter, None),
        PipelineSkip::EmptyAfterProcessing => (SkipReason::EmptyAfterProcessing, None),
    }
}

pub(crate) fn resolver_skip(reason: &str) -> SkipReason {
    match reason {
        "blocked by alias" => SkipReason::BlockedByAlias,
        "no voices available" => SkipReason::NoVoicesAvailable,
        VOICE_OVERRIDE_NOT_FOUND => SkipReason::VoiceOverrideNotFound,
        _ => SkipReason::VoiceResolutionFailed,
    }
}

pub(crate) const VOICE_OVERRIDE_NOT_FOUND: &str = "voice override not found in catalog";

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use forge_events::{Event, EventPublisher, EventSource};
    use forge_speech_types::{EngineId, LanguageCode, VoiceId};
    use forge_types::EventId;
    use serde_json::{Value, json};
    use tokio::sync::broadcast;

    use super::*;
    use crate::{Priority, RequestId, SpeakEvent, SpeakRequest};

    #[derive(Default)]
    struct RecordingBus {
        events: Mutex<Vec<Event>>,
    }

    impl EventPublisher for RecordingBus {
        fn publish(&self, event: Event) {
            self.events.lock().unwrap().push(event);
        }
    }

    fn request(source: Option<EventId>) -> SpeakRequest {
        SpeakRequest {
            request_id: RequestId("req-1".into()),
            viewer_id: "v-1".into(),
            viewer_name: "nova".into(),
            text: "hi chat".into(),
            priority: Priority::Normal,
            engine_override: None,
            voice_override: None,
            source_event_id: source,
            is_reward: false,
            target: None,
            message_emotes: Default::default(),
        }
    }

    fn emitted(emit: impl FnOnce(&SpeakEmitter<'_>)) -> (Event, Vec<SpeakEvent>) {
        let bus = Arc::new(RecordingBus::default());
        let (tx, mut rx) = broadcast::channel(8);
        emit(&SpeakEmitter::new(&tx, bus.as_ref()));
        let mut events = bus.events.lock().unwrap();
        assert_eq!(events.len(), 1, "exactly one bus event per emission");
        let feed = std::iter::from_fn(|| rx.try_recv().ok()).collect();
        (events.remove(0), feed)
    }

    #[test]
    fn every_bus_kind_keeps_its_wire_kind_payload_and_cause() {
        let src = EventId::new();
        let req = request(Some(src));
        let voice = VoiceId("en-amy".into());
        let engine = EngineId("piper".into());
        let language = DetectedLanguage {
            code: LanguageCode::from_locale("uk-UA").unwrap(),
            confidence: 0.75,
        };
        type Emission<'a> = Box<dyn FnOnce(&SpeakEmitter<'_>) + 'a>;
        let rows: Vec<(&str, Emission<'_>, Value, Option<EventId>)> = vec![
            (
                "speak.enqueued",
                Box::new(|e| {
                    e.enqueued(
                        &req,
                        3,
                        EnqueuePreview {
                            voice_preview: "Amy".into(),
                            estimated_secs: 4,
                        },
                    )
                }),
                json!({ "request_id": "req-1", "queue_len": 3, "viewer_name": "nova", "text": "hi chat" }),
                Some(src),
            ),
            (
                "speak.started",
                Box::new(|e| {
                    e.started(
                        &req,
                        Some(ResolvedPlayback {
                            voice_id: &voice,
                            engine_id: &engine,
                            duration_secs: 2,
                            language: Some(&language),
                        }),
                        1,
                    )
                }),
                json!({
                    "request_id": "req-1",
                    "voice_id": "en-amy",
                    "engine_id": "piper",
                    "queue_len": 1,
                    "viewer_name": "nova",
                    "text": "hi chat",
                    "detected_language": "uk",
                    "language_confidence": 0.75,
                }),
                Some(src),
            ),
            (
                "speak.started",
                Box::new(|e| e.started(&req, None, 0)),
                json!({
                    "request_id": "req-1",
                    "voice_id": null,
                    "engine_id": null,
                    "queue_len": 0,
                    "viewer_name": "nova",
                    "text": "hi chat",
                    "detected_language": null,
                    "language_confidence": null,
                }),
                Some(src),
            ),
            (
                "speak.finished",
                Box::new(|e| e.outcome(&req, Outcome::Finished)),
                json!({ "request_id": "req-1", "viewer_name": "nova", "text": "hi chat" }),
                Some(src),
            ),
            (
                "speak.skipped",
                Box::new(|e| {
                    e.outcome(
                        &req,
                        Outcome::Skipped {
                            reason: SkipReason::SkipRuleMatched,
                            detail: Some("no urls".into()),
                        },
                    )
                }),
                json!({
                    "request_id": "req-1",
                    "reason": "skip_rule_matched",
                    "detail": "no urls",
                    "viewer_name": "nova",
                    "text": "hi chat",
                }),
                Some(src),
            ),
            (
                "speak.skipped",
                Box::new(|e| e.skipped_in_flight(RequestId("req-1".into()), SkipReason::Cancelled)),
                json!({
                    "request_id": "req-1",
                    "reason": "cancelled",
                    "detail": null,
                    "viewer_name": null,
                    "text": null,
                }),
                None,
            ),
            (
                "speak.failed",
                Box::new(|e| {
                    e.outcome(
                        &req,
                        Outcome::Failed {
                            reason: FailureReason::DeviceError,
                            error: "device unplugged".into(),
                        },
                    )
                }),
                json!({
                    "request_id": "req-1",
                    "reason": "device_error",
                    "error": "device unplugged",
                    "viewer_name": "nova",
                    "text": "hi chat",
                }),
                Some(src),
            ),
            (
                "speak.rejected",
                Box::new(|e| {
                    e.outcome(
                        &req,
                        Outcome::Rejected {
                            reason: RejectReason::QueueFull,
                            limit: 5,
                            queue_len: 5,
                        },
                    )
                }),
                json!({
                    "request_id": "req-1",
                    "reason": "queue_full",
                    "limit": 5,
                    "queue_len": 5,
                    "viewer_name": "nova",
                    "text": "hi chat",
                }),
                Some(src),
            ),
            (
                "speak.removed",
                Box::new(|e| e.outcome(&req, Outcome::Removed)),
                json!({ "request_id": "req-1", "viewer_name": "nova", "text": "hi chat" }),
                Some(src),
            ),
            (
                "speak.reordered",
                Box::new(|e| e.reordered(&req, Some(RequestId("req-0".into())))),
                json!({ "request_id": "req-1", "before": "req-0", "viewer_name": "nova", "text": "hi chat" }),
                Some(src),
            ),
            (
                "speak.reordered",
                Box::new(|e| e.reordered(&req, None)),
                json!({ "request_id": "req-1", "before": null, "viewer_name": "nova", "text": "hi chat" }),
                Some(src),
            ),
            (
                "speak.cleared",
                Box::new(|e| e.cleared(true)),
                json!({ "keep_current": true }),
                None,
            ),
            (
                "speak.paused",
                Box::new(|e| e.paused(HoldReason::VoiceGate)),
                json!({ "reason": "voicegate" }),
                None,
            ),
            (
                "speak.resumed",
                Box::new(|e| e.resumed(HoldReason::User)),
                json!({ "reason": "user" }),
                None,
            ),
        ];

        for (kind, emit, payload, caused_by) in rows {
            let (event, _) = emitted(emit);
            assert_eq!(event.kind, kind);
            assert_eq!(event.source, EventSource::Audio, "{kind}");
            assert_eq!(event.payload, payload, "{kind}");
            assert_eq!(event.caused_by, caused_by, "{kind}");
        }
    }

    #[test]
    fn every_reason_serializes_to_its_wire_token_and_parses_back() {
        fn round_trips<T>(cases: &[(T, &str)])
        where
            T: serde::Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug,
        {
            for (reason, token) in cases {
                assert_eq!(serde_json::to_value(reason).unwrap(), json!(token));
                assert_eq!(&serde_json::from_value::<T>(json!(token)).unwrap(), reason);
            }
        }
        round_trips(&[
            (SkipReason::SkipRuleMatched, "skip_rule_matched"),
            (SkipReason::BlockedByWordFilter, "blocked_by_word_filter"),
            (SkipReason::EmptyAfterProcessing, "empty_after_processing"),
            (SkipReason::BlockedByAlias, "blocked_by_alias"),
            (SkipReason::NoVoicesAvailable, "no_voices_available"),
            (
                SkipReason::VoiceOverrideNotFound,
                "voice_override_not_found",
            ),
            (SkipReason::VoiceResolutionFailed, "voice_resolution_failed"),
            (SkipReason::UserSkip, "user_skip"),
            (SkipReason::Promoted, "promoted"),
            (SkipReason::Cancelled, "cancelled"),
            (SkipReason::Cleared, "cleared"),
        ]);
        round_trips(&[
            (FailureReason::EngineError, "engine_error"),
            (FailureReason::DeviceError, "device_error"),
            (FailureReason::EngineTimeout, "engine_timeout"),
        ]);
        round_trips(&[
            (RejectReason::QueueFull, "queue_full"),
            (RejectReason::PerUserLimit, "per_user_limit"),
        ]);
        round_trips(&[
            (HoldReason::User, "user"),
            (HoldReason::VoiceGate, "voicegate"),
        ]);
    }

    #[test]
    fn feed_skip_text_appends_the_detail_only_when_present() {
        for (reason, detail, expected) in [
            (
                SkipReason::SkipRuleMatched,
                Some("no urls"),
                "matched a skip rule: no urls",
            ),
            (SkipReason::UserSkip, None, "skipped by user"),
        ] {
            let req = request(None);
            let (_, feed) = emitted(|e| {
                e.outcome(
                    &req,
                    Outcome::Skipped {
                        reason,
                        detail: detail.map(str::to_owned),
                    },
                )
            });
            assert!(
                matches!(
                    feed.as_slice(),
                    [SpeakEvent::Skipped { reason, .. }] if reason == expected
                ),
                "{expected}"
            );
        }
    }

    #[test]
    fn feed_reject_text_names_the_limit_that_was_hit() {
        for (reason, expected) in [
            (RejectReason::QueueFull, "queue full (max 7)"),
            (RejectReason::PerUserLimit, "per-user limit (max 7)"),
        ] {
            let req = request(None);
            let (_, feed) = emitted(|e| {
                e.outcome(
                    &req,
                    Outcome::Rejected {
                        reason,
                        limit: 7,
                        queue_len: 2,
                    },
                )
            });
            assert!(
                matches!(
                    feed.as_slice(),
                    [SpeakEvent::Rejected { reason, .. }] if reason == expected
                ),
                "{expected}"
            );
        }
    }
}
