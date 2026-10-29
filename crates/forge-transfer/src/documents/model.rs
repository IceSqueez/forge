use std::collections::BTreeMap;

use forge_storage::{
    GlobalEntry, MediaBlob, MediaFormat, OverlayDefinition, ScriptRecord, StoredClip,
};
use forge_types::{
    Action, ActionId, ClipId, ExecutionMode, PermissionRung, PlatformScope, Queue, QueueId,
    ScriptContract, ScriptId, SubActionStep, TriggerInstance, TriggerInstanceId, Variant,
    VariantKind,
};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

pub const PACK_DOCUMENTS_VERSION: u32 = 1;

pub type DocumentConfig = BTreeMap<String, Variant>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PackDocuments {
    pub version: u32,
    pub actions: Vec<ActionDocument>,
    pub triggers: Vec<TriggerDocument>,
    pub queues: Vec<QueueDocument>,
    pub scripts: Vec<ScriptDocument>,
    pub globals: Vec<GlobalDocument>,
    pub clips: Vec<ClipDocument>,
    pub overlays: Vec<OverlayDocument>,
    pub media: Vec<MediaDocument>,
}

impl PackDocuments {
    pub fn bundled_files(&self) -> impl Iterator<Item = &str> {
        let media = self.media.iter().map(|media| media.file.as_str());
        let clips = self.clips.iter().filter_map(|clip| match &clip.source {
            ClipSourceDocument::File { file, .. } => Some(file.as_str()),
            ClipSourceDocument::Media { .. } | ClipSourceDocument::Missing => None,
        });
        let overlays = self
            .overlays
            .iter()
            .flat_map(|overlay| overlay.files.iter().map(|file| file.file.as_str()));
        media.chain(clips).chain(overlays)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionDocument {
    pub id: ActionId,
    pub name: String,
    pub group: Option<String>,
    pub queue_id: QueueId,
    pub enabled: bool,
    pub concurrent: bool,
    pub bypass_pause: bool,
    pub execution_mode: ExecutionMode,
    pub description: Option<String>,
    pub steps: Vec<StepDocument>,
    pub trigger_ids: Vec<TriggerInstanceId>,
}

impl ActionDocument {
    pub(crate) fn of(action: &Action, trigger_ids: Vec<TriggerInstanceId>) -> Self {
        Self {
            id: action.id,
            name: action.name.clone(),
            group: action.group.clone(),
            queue_id: action.queue_id,
            enabled: action.enabled,
            concurrent: action.concurrent,
            bypass_pause: action.bypass_pause,
            execution_mode: action.execution_mode,
            description: action.description.clone(),
            steps: action.sub_actions.iter().map(StepDocument::of).collect(),
            trigger_ids,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StepDocument {
    pub kind_id: String,
    pub config: DocumentConfig,
    pub enabled: bool,
    pub continue_on_error: bool,
    pub condition: Option<String>,
    pub label: Option<String>,
}

impl StepDocument {
    fn of(step: &SubActionStep) -> Self {
        Self {
            kind_id: step.kind_id.clone(),
            config: step.config.clone(),
            enabled: step.enabled,
            continue_on_error: step.continue_on_error,
            condition: step.condition.clone(),
            label: step.label.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TriggerDocument {
    pub id: TriggerInstanceId,
    pub kind_id: String,
    pub name: String,
    pub overrides: DocumentConfig,
    pub enabled: bool,
    pub user_defined: bool,
    pub platform_scope: PlatformScope,
    pub cooldown_secs: u32,
    pub cooldown_global: bool,
    pub permission_rung: PermissionRung,
}

impl TriggerDocument {
    pub(crate) fn of(trigger: &TriggerInstance) -> Self {
        Self {
            id: trigger.id,
            kind_id: trigger.kind_id.clone(),
            name: trigger.name.clone(),
            overrides: trigger.overrides.clone(),
            enabled: trigger.enabled,
            user_defined: trigger.user_defined,
            platform_scope: trigger.platform_scope.clone(),
            cooldown_secs: trigger.cooldown_secs,
            cooldown_global: trigger.cooldown_global,
            permission_rung: trigger.permission_rung,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueueDocument {
    pub id: QueueId,
    pub name: String,
    pub description: String,
    pub concurrency: u32,
}

impl QueueDocument {
    pub(crate) fn of(queue: &Queue) -> Self {
        Self {
            id: queue.id,
            name: queue.name.clone(),
            description: queue.description.clone(),
            concurrency: queue.concurrency,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScriptDocument {
    pub id: ScriptId,
    pub name: String,
    pub body: String,
    pub contract: ScriptContract,
    pub enabled: bool,
}

impl ScriptDocument {
    pub(crate) fn of(script: &ScriptRecord) -> Self {
        Self {
            id: script.id,
            name: script.name.clone(),
            body: script.body.clone(),
            contract: script.contract.clone(),
            enabled: script.enabled,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GlobalDocument {
    pub name: String,
    pub value_type: VariantKind,
    pub default_value: Variant,
    pub persisted: bool,
}

impl GlobalDocument {
    pub(crate) fn of(global: &GlobalEntry) -> Self {
        let value_type = global.value.type_tag();
        Self {
            name: global.name.clone(),
            value_type,
            default_value: default_of(value_type),
            persisted: global.persisted,
        }
    }
}

fn default_of(value_type: VariantKind) -> Variant {
    match value_type {
        VariantKind::Int => Variant::Int(0),
        VariantKind::Float => Variant::Float(0.0),
        VariantKind::Bool => Variant::Bool(false),
        VariantKind::String => Variant::String(String::new()),
        VariantKind::Datetime => Variant::Datetime(OffsetDateTime::UNIX_EPOCH),
        VariantKind::Array => Variant::Array(Vec::new()),
        VariantKind::Object => Variant::Object(BTreeMap::new()),
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClipDocument {
    pub id: ClipId,
    pub name: String,
    pub volume: f32,
    pub category: String,
    pub loop_playback: bool,
    pub duration_secs: Option<f32>,
    pub builtin_id: Option<String>,
    pub source: ClipSourceDocument,
}

impl ClipDocument {
    pub(crate) fn of(clip: &StoredClip, source: ClipSourceDocument) -> Self {
        Self {
            id: clip.id,
            name: clip.name.clone(),
            volume: clip.volume,
            category: clip.category.clone(),
            loop_playback: clip.loop_playback,
            duration_secs: clip.duration_secs,
            builtin_id: clip.builtin_id.clone(),
            source,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ClipSourceDocument {
    Media { media_id: String },
    File { file: String, label: String },
    Missing,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OverlayDocument {
    pub id: String,
    pub display_name: String,
    pub kind_id: String,
    pub enabled: bool,
    pub position: i64,
    pub config: DocumentConfig,
    pub config_schema_version: u32,
    pub source_overrides: Vec<String>,
    pub files: Vec<OverlayFileDocument>,
}

impl OverlayDocument {
    pub(crate) fn of(overlay: &OverlayDefinition, files: Vec<OverlayFileDocument>) -> Self {
        Self {
            id: overlay.id.as_str().to_owned(),
            display_name: overlay.display_name.clone(),
            kind_id: overlay.kind_id.clone(),
            enabled: overlay.enabled,
            position: overlay.position,
            config: overlay.config.clone(),
            config_schema_version: overlay.config_schema_version,
            source_overrides: overlay.source_overrides.clone(),
            files,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OverlayFileDocument {
    pub name: String,
    pub file: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaDocument {
    pub id: String,
    pub label: String,
    pub format: MediaFormat,
    pub file: String,
}

impl MediaDocument {
    pub(crate) fn of(blob: &MediaBlob, file: String) -> Self {
        Self {
            id: blob.id.as_str().to_owned(),
            label: blob.label.clone(),
            format: blob.format,
            file,
        }
    }
}
