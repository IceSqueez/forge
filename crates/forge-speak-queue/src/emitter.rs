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
