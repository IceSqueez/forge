use std::collections::BTreeMap;

use forge_speech_types::{AssignmentStrategy, IgnoreProfile, VoiceAlias};
use forge_storage::{FilterRule, TtsPipelineSettings};
use forge_types::{ClipId, Variant};
use serde::{Deserialize, Serialize};

use crate::pack::documents::PackDocuments;

pub const SETUP_DOCUMENTS_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetupDocuments {
    pub version: u32,
    pub configuration: PackDocuments,
    pub global_values: BTreeMap<String, Variant>,
    pub clip_hotkeys: Vec<ClipHotkeyDocument>,
    pub tts: TtsDocument,
    pub voices: VoicesDocument,
    pub settings: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClipHotkeyDocument {
    pub clip_id: ClipId,
    pub hotkey: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TtsDocument {
    pub pipeline: TtsPipelineSettings,
    pub filter_rules: Vec<FilterRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoicesDocument {
    pub aliases: Vec<VoiceAlias>,
    pub strategy: AssignmentStrategy,
    pub ignore_profile: IgnoreProfile,
}
