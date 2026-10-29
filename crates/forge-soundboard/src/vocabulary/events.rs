use forge_events::TypedEvent;
use forge_types::ClipId;
use serde::{Deserialize, Serialize};

use super::event_kinds;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlaybackStarted {
    pub play_id: u64,
    pub clip_id: Option<ClipId>,
    pub clip_label: Option<String>,
    pub device: String,
    pub duration_secs: Option<f64>,
    pub looped: bool,
}

impl TypedEvent for PlaybackStarted {
    const KIND: &'static str = event_kinds::PLAYBACK_STARTED;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaybackFinished {
    pub play_id: u64,
    pub clip_id: Option<ClipId>,
    pub clip_label: Option<String>,
}

impl TypedEvent for PlaybackFinished {
    const KIND: &'static str = event_kinds::PLAYBACK_FINISHED;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaybackFailed {
    pub play_id: u64,
    pub clip_id: Option<ClipId>,
    pub clip_label: Option<String>,
    pub error: String,
}

impl TypedEvent for PlaybackFailed {
    const KIND: &'static str = event_kinds::PLAYBACK_FAILED;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "verdict", rename_all = "snake_case")]
pub enum SettledVerdict {
    Adopted,
    AlreadyManaged,
    SourceMissing,
    Refused { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClipAdoptionSettled {
    pub clip_id: ClipId,
    #[serde(flatten)]
    pub verdict: SettledVerdict,
}

impl TypedEvent for ClipAdoptionSettled {
    const KIND: &'static str = event_kinds::CLIP_ADOPTED;
}
