use forge_events::TypedEvent;
use forge_speech_types::{EngineId, VoiceId};
use serde::{Deserialize, Serialize};

use super::event_kinds;
use super::reasons::{FailureReason, HoldReason, RejectReason, SkipReason};
use crate::RequestId;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpeakEnqueued {
    pub request_id: RequestId,
    pub queue_len: usize,
    pub viewer_name: String,
    pub text: String,
}

impl TypedEvent for SpeakEnqueued {
    const KIND: &'static str = event_kinds::ENQUEUED;
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpeakStarted {
    pub request_id: RequestId,
    pub voice_id: Option<VoiceId>,
    pub engine_id: Option<EngineId>,
    pub queue_len: usize,
    pub viewer_name: String,
    pub text: String,
    pub detected_language: Option<String>,
    pub language_confidence: Option<f64>,
}

impl TypedEvent for SpeakStarted {
    const KIND: &'static str = event_kinds::STARTED;
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpeakFinished {
    pub request_id: RequestId,
    pub viewer_name: String,
    pub text: String,
}

impl TypedEvent for SpeakFinished {
    const KIND: &'static str = event_kinds::FINISHED;
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpeakSkipped {
    pub request_id: RequestId,
    pub reason: SkipReason,
    pub detail: Option<String>,
    pub viewer_name: Option<String>,
    pub text: Option<String>,
}

impl TypedEvent for SpeakSkipped {
    const KIND: &'static str = event_kinds::SKIPPED;
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpeakFailed {
    pub request_id: RequestId,
    pub reason: FailureReason,
    pub error: String,
    pub viewer_name: String,
    pub text: String,
}

impl TypedEvent for SpeakFailed {
    const KIND: &'static str = event_kinds::FAILED;
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpeakRejected {
    pub request_id: RequestId,
    pub reason: RejectReason,
    pub limit: usize,
    pub queue_len: usize,
    pub viewer_name: String,
    pub text: String,
}

impl TypedEvent for SpeakRejected {
    const KIND: &'static str = event_kinds::REJECTED;
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpeakRemoved {
    pub request_id: RequestId,
    pub viewer_name: String,
    pub text: String,
}

impl TypedEvent for SpeakRemoved {
    const KIND: &'static str = event_kinds::REMOVED;
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpeakReordered {
    pub request_id: RequestId,
    pub before: Option<RequestId>,
    pub viewer_name: String,
    pub text: String,
}

impl TypedEvent for SpeakReordered {
    const KIND: &'static str = event_kinds::REORDERED;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpeakCleared {
    pub keep_current: bool,
}

impl TypedEvent for SpeakCleared {
    const KIND: &'static str = event_kinds::CLEARED;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpeakPaused {
    pub reason: HoldReason,
}

impl TypedEvent for SpeakPaused {
    const KIND: &'static str = event_kinds::PAUSED;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpeakResumed {
    pub reason: HoldReason,
}

impl TypedEvent for SpeakResumed {
    const KIND: &'static str = event_kinds::RESUMED;
}
