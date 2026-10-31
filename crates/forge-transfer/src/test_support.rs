use std::num::NonZeroU32;
use std::path::PathBuf;

use async_trait::async_trait;
use forge_events::Event;
use forge_overlay::OverlayKindRegistry;
use forge_registry::references::{
    ACTION_OPTIONS_KEY, GLOBAL_OPTIONS_KEY, MANAGED_MEDIA_NAMESPACE, SCRIPT_OPTIONS_KEY,
    SOUNDBOARD_CLIP_OPTIONS_KEY,
};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, ReferenceField, RunContext, SubActionCategory,
    SubActionRegistry, SubActionRunner, TriggerCategory, TriggerKindDescriptor, TriggerRegistry,
};
use forge_storage::{GlobalEntry, OverlayDefinition, OverlayId, ScriptRecord, StoredClip};
use forge_types::{
    Action, ActionId, ArgStack, ClipId, ExecutionMode, IntegrationId, OutputDevice,
    OverlayCredential, PermissionRung, PlatformScope, Queue, QueueId, ScriptContract, ScriptId,
    SubActionConfig, SubActionStep, SubActionTelemetry, TriggerConfig, TriggerInstance,
    TriggerInstanceId, Variant,
};
use time::OffsetDateTime;

use crate::bundle::manifest::{BUNDLE_FORMAT_VERSION, BundleKind, FORGE_VERSION};
use crate::documents::closure::ClosureRegistries;
use crate::documents::model::{PACK_DOCUMENTS_VERSION, PackDocuments};
use crate::pack::{PackManifest, StagedPack};

pub(crate) const PLAIN_STEP: &str = "fixture.plain";
pub(crate) const HTTP_STEP: &str = "fixture.http";
pub(crate) const SCRIPT_STEP: &str = "fixture.script";
pub(crate) const OWNED_STEP: &str = "fixture.owned_step";
pub(crate) const PLAIN_TRIGGER: &str = "fixture.plain_trigger";
pub(crate) const OWNED_TRIGGER: &str = "fixture.owned_trigger";
pub(crate) const STEP_OWNER: &str = "fixture-step-owner";
pub(crate) const TRIGGER_OWNER: &str = "fixture-trigger-owner";
pub(crate) const ACTION_KEY: &str = "action";
pub(crate) const SCRIPT_KEY: &str = "script";
pub(crate) const GLOBAL_KEY: &str = "global";
pub(crate) const CLIP_KEY: &str = "clip";
pub(crate) const MEDIA_KEY: &str = "media";
pub(crate) const SCENE_KEY: &str = "scene";
pub(crate) const SCENE_NAMESPACE: &str = "fixture_scene";

struct FixtureRunner {
    id: &'static str,
    category: SubActionCategory,
}

#[async_trait]
impl SubActionRunner for FixtureRunner {
    fn id(&self) -> &str {
        self.id
    }

    fn category(&self) -> SubActionCategory {
        self.category
    }

    fn label(&self) -> &str {
        self.id
    }

    fn summary(&self) -> &str {
        self.id
    }

    fn search_text(&self) -> &str {
        self.id
    }

    fn icon_name(&self) -> &str {
        self.id
    }

    fn declared_references(&self) -> Vec<ReferenceField> {
        [
            (ACTION_KEY, ACTION_OPTIONS_KEY),
            (SCRIPT_KEY, SCRIPT_OPTIONS_KEY),
            (GLOBAL_KEY, GLOBAL_OPTIONS_KEY),
            (CLIP_KEY, SOUNDBOARD_CLIP_OPTIONS_KEY),
            (MEDIA_KEY, MANAGED_MEDIA_NAMESPACE),
            (SCENE_KEY, SCENE_NAMESPACE),
        ]
        .map(|(key, options_key)| ReferenceField { key, options_key })
        .to_vec()
    }

    async fn execute(
        &self,
        _: &SubActionConfig,
        _: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        unreachable!("fixture steps are inspected, never run")
    }
}

struct FixtureTrigger {
    id: &'static str,
}

impl TriggerKindDescriptor for FixtureTrigger {
    fn id(&self) -> &str {
        self.id
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Chat
    }

    fn label(&self) -> &str {
        self.id
    }

    fn summary(&self) -> &str {
        self.id
    }

    fn search_text(&self) -> &str {
        self.id
    }

    fn icon_name(&self) -> &str {
        self.id
    }

    fn platform_contract(&self) -> KindPlatformContract {
        KindPlatformContract::Universal
    }

    fn default_config(&self) -> TriggerConfig {
        TriggerConfig::new()
    }

    fn config_fields(&self) -> Vec<FormField> {
        Vec::new()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: None,
            kind_prefix: None,
        }
    }

    fn matches_trigger(&self, _: &TriggerConfig, _: &Event) -> bool {
        false
    }
}

pub(crate) struct FixtureRegistries {
    sub_actions: SubActionRegistry,
    triggers: TriggerRegistry,
    overlays: OverlayKindRegistry,
}

impl FixtureRegistries {
    pub(crate) fn new() -> Self {
        let mut sub_actions = SubActionRegistry::new();
        for (id, category) in [
            (PLAIN_STEP, SubActionCategory::Util),
            (HTTP_STEP, SubActionCategory::Http),
            (SCRIPT_STEP, SubActionCategory::Scripts),
        ] {
            sub_actions
                .register(Box::new(FixtureRunner { id, category }))
                .unwrap();
        }
        sub_actions
            .owned_by(IntegrationId::new(STEP_OWNER))
            .register(Box::new(FixtureRunner {
                id: OWNED_STEP,
                category: SubActionCategory::Util,
            }))
            .unwrap();
        let mut triggers = TriggerRegistry::new();
        triggers
            .register(Box::new(FixtureTrigger { id: PLAIN_TRIGGER }))
            .unwrap();
        triggers
            .owned_by(IntegrationId::new(TRIGGER_OWNER))
            .register(Box::new(FixtureTrigger { id: OWNED_TRIGGER }))
            .unwrap();
        Self {
            sub_actions,
            triggers,
            overlays: OverlayKindRegistry::new(),
        }
    }

    pub(crate) fn closure(&self) -> ClosureRegistries<'_> {
        ClosureRegistries {
            sub_actions: &self.sub_actions,
            triggers: &self.triggers,
            overlays: &self.overlays,
            schema: None,
        }
    }
}

pub(crate) fn step(kind_id: &str, references: &[(&str, &str)]) -> SubActionStep {
    SubActionStep {
        kind_id: kind_id.to_owned(),
        config: references
            .iter()
            .map(|(key, value)| ((*key).to_owned(), Variant::String((*value).to_owned())))
            .collect(),
        enabled: true,
        continue_on_error: false,
        condition: None,
        label: None,
    }
}

pub(crate) fn action(name: &str, queue_id: QueueId, sub_actions: Vec<SubActionStep>) -> Action {
    Action {
        id: ActionId::new(),
        name: name.to_owned(),
        group: None,
        queue_id,
        enabled: true,
        concurrent: false,
        bypass_pause: false,
        execution_mode: ExecutionMode::default(),
        description: None,
        sub_actions,
    }
}

pub(crate) fn trigger(kind_id: &str, name: &str, user_defined: bool) -> TriggerInstance {
    TriggerInstance {
        id: TriggerInstanceId::new(),
        kind_id: kind_id.to_owned(),
        name: name.to_owned(),
        overrides: TriggerConfig::new(),
        enabled: true,
        user_defined,
        platform_scope: PlatformScope::default(),
        cooldown_secs: 0,
        cooldown_global: true,
        permission_rung: PermissionRung::default(),
    }
}

pub(crate) fn queue(name: &str) -> Queue {
    Queue {
        id: QueueId::new(),
        name: name.to_owned(),
        description: String::new(),
        concurrency: NonZeroU32::MIN,
    }
}

pub(crate) fn script(name: &str) -> ScriptRecord {
    ScriptRecord {
        id: ScriptId::new(),
        name: name.to_owned(),
        body: String::new(),
        contract: ScriptContract::default(),
        enabled: true,
        created_at: OffsetDateTime::UNIX_EPOCH,
        last_modified: OffsetDateTime::UNIX_EPOCH,
    }
}

pub(crate) fn global(name: &str) -> GlobalEntry {
    GlobalEntry {
        name: name.to_owned(),
        value: Variant::Int(0),
        persisted: true,
        reads: 0,
        writes: 0,
        created_at: OffsetDateTime::UNIX_EPOCH,
        last_modified: OffsetDateTime::UNIX_EPOCH,
    }
}

pub(crate) fn clip(name: &str) -> StoredClip {
    StoredClip {
        id: ClipId::new(),
        name: name.to_owned(),
        file_path: PathBuf::new(),
        volume: 1.0,
        output_device: OutputDevice::Default,
        hotkey: None,
        created_at: OffsetDateTime::UNIX_EPOCH,
        category: String::new(),
        loop_playback: false,
        duration_secs: None,
        builtin_id: None,
    }
}

pub(crate) fn overlay(id: &str, display_name: &str) -> OverlayDefinition {
    OverlayDefinition {
        id: OverlayId::new(id),
        display_name: display_name.to_owned(),
        kind_id: String::new(),
        enabled: true,
        position: 0,
        config: Default::default(),
        config_schema_version: 0,
        generator_version: 0,
        source_overrides: Vec::new(),
        credential: OverlayCredential::new(String::new()),
        created_at: OffsetDateTime::UNIX_EPOCH,
        updated_at: OffsetDateTime::UNIX_EPOCH,
    }
}

pub(crate) fn no_documents() -> PackDocuments {
    PackDocuments {
        version: PACK_DOCUMENTS_VERSION,
        actions: Vec::new(),
        triggers: Vec::new(),
        queues: Vec::new(),
        scripts: Vec::new(),
        globals: Vec::new(),
        clips: Vec::new(),
        overlays: Vec::new(),
        media: Vec::new(),
    }
}

pub(crate) fn blank_manifest() -> PackManifest {
    PackManifest {
        format_version: BUNDLE_FORMAT_VERSION,
        kind: BundleKind::Pack,
        forge_version: FORGE_VERSION.to_owned(),
        schema_version: 0,
        created_at: OffsetDateTime::UNIX_EPOCH,
        name: String::new(),
        author: String::new(),
        description: String::new(),
        required_integrations: Vec::new(),
        external_references: Vec::new(),
        unresolved_references: Vec::new(),
        unknown_kinds: Vec::new(),
        contents: Vec::new(),
    }
}

pub(crate) fn staged(manifest: PackManifest, documents: PackDocuments) -> StagedPack {
    StagedPack::in_memory(manifest, documents)
}
