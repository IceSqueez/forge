#![allow(clippy::unwrap_used)]
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use forge_overlay::OverlayKindRegistry;
use forge_registry::{IdSuccession, SubActionRegistry, TriggerRegistry};
use forge_speech_types::{AssignmentStrategy, IgnoreProfile};
use forge_storage::settings::core_settings::{
    PICKER_FAVORITES_SUB_ACTIONS, PICKER_FAVORITES_TRIGGERS,
};
use forge_storage::{EXPECTED_SCHEMA_VERSION, TtsPipelineSettings};
use forge_transfer::{
    ActionDocument, BUNDLE_FORMAT_VERSION, BundleKind, ClosureRegistries, ContentEntry,
    MANIFEST_ENTRY_NAME, PACK_DOCUMENTS_VERSION, PackDocuments, PackManifest,
    SETUP_DOCUMENTS_VERSION, SetupDocuments, SetupManifest, StepDocument, TriggerDocument,
    TtsDocument, VoicesDocument, stage_pack, stage_setup,
};
use forge_types::{
    ActionId, ExecutionMode, PermissionRung, PlatformScope, QueueId, TriggerInstanceId, Variant,
};
use sha2::{Digest, Sha256};
use tempfile::TempDir;
use time::OffsetDateTime;

const PACK_DOCUMENTS_ENTRY: &str = "pack/documents.json";
const SETUP_DOCUMENTS_ENTRY: &str = "setup/documents.json";
const RETIRED_TRIGGER: &str = "obs.record.started";
const CURRENT_TRIGGER: &str = "obs.recording.started";
const RETIRED_STEP: &str = "obs.record.start";
const CURRENT_STEP: &str = "obs.recording.start";
const UNRELATED_KIND: &str = "core.log.write";

const TRIGGER_SUCCESSION: IdSuccession = IdSuccession {
    current: CURRENT_TRIGGER,
    retired: &[RETIRED_TRIGGER],
};
const STEP_SUCCESSION: IdSuccession = IdSuccession {
    current: CURRENT_STEP,
    retired: &[RETIRED_STEP],
};

struct Registries {
    sub_actions: SubActionRegistry,
    triggers: TriggerRegistry,
    overlays: OverlayKindRegistry,
}

impl Registries {
    fn declaring_renames() -> Self {
        let mut sub_actions = SubActionRegistry::new();
        sub_actions.declare_retired(&STEP_SUCCESSION).unwrap();
        let mut triggers = TriggerRegistry::new();
        triggers.declare_retired(&TRIGGER_SUCCESSION).unwrap();
        Self {
            sub_actions,
            triggers,
            overlays: OverlayKindRegistry::new(),
        }
    }

    fn closure(&self) -> ClosureRegistries<'_> {
        ClosureRegistries {
            sub_actions: &self.sub_actions,
            triggers: &self.triggers,
            overlays: &self.overlays,
            schema: None,
        }
    }
}

fn s(value: &str) -> Variant {
    Variant::String(value.to_owned())
}

fn obj<const N: usize>(fields: [(&str, Variant); N]) -> Variant {
    Variant::Object(fields.into_iter().map(|(k, v)| (k.to_owned(), v)).collect())
}

fn step(kind_id: &str, config: BTreeMap<String, Variant>) -> StepDocument {
    StepDocument {
        kind_id: kind_id.to_owned(),
        config,
        enabled: true,
        continue_on_error: false,
        condition: None,
        label: None,
    }
}

fn trigger(id: &str, kind_id: &str) -> TriggerDocument {
    TriggerDocument {
        id: id.parse().unwrap(),
        kind_id: kind_id.to_owned(),
        name: "trigger".to_owned(),
        overrides: BTreeMap::new(),
        enabled: true,
        user_defined: true,
        platform_scope: PlatformScope::default(),
        cooldown_secs: 0,
        cooldown_global: true,
        permission_rung: PermissionRung::Everyone,
    }
}

struct Spelling {
    trigger: &'static str,
    step: &'static str,
}

const RETIRED: Spelling = Spelling {
    trigger: RETIRED_TRIGGER,
    step: RETIRED_STEP,
};
const CURRENT: Spelling = Spelling {
    trigger: CURRENT_TRIGGER,
    step: CURRENT_STEP,
};

fn configuration(spelling: &Spelling) -> PackDocuments {
    let undecodable_nested = obj([
        ("enabled", s("yes")),
        ("kind_id", s(spelling.step)),
        ("x_extra", Variant::Int(1)),
    ]);
    let note = BTreeMap::from([("note".to_owned(), s(RETIRED_STEP))]);
    let branch = BTreeMap::from([(
        "then_chain".to_owned(),
        Variant::Array(vec![undecodable_nested]),
    )]);
    let queue_id: QueueId = "01JBZ8Q4T5N7W3K2M9X6V1R0Q1".parse().unwrap();
    PackDocuments {
        version: PACK_DOCUMENTS_VERSION,
        actions: vec![ActionDocument {
            id: "01JBZ8Q4T5N7W3K2M9X6V1R0A1".parse::<ActionId>().unwrap(),
            name: "renamed".to_owned(),
            group: None,
            queue_id,
            enabled: true,
            concurrent: false,
            bypass_pause: false,
            execution_mode: ExecutionMode::Sequential,
            description: None,
            steps: vec![
                step(spelling.step, note.clone()),
                step("core.logic.if_then_else", branch),
                step(RETIRED_TRIGGER, note),
                step(UNRELATED_KIND, BTreeMap::new()),
            ],
            trigger_ids: Vec::<TriggerInstanceId>::new(),
        }],
        triggers: vec![
            trigger("01JBZ8Q4T5N7W3K2M9X6V1R0T1", spelling.trigger),
            trigger("01JBZ8Q4T5N7W3K2M9X6V1R0T2", RETIRED_STEP),
            trigger("01JBZ8Q4T5N7W3K2M9X6V1R0T3", UNRELATED_KIND),
        ],
        queues: Vec::new(),
        scripts: Vec::new(),
        globals: Vec::new(),
        clips: Vec::new(),
        overlays: Vec::new(),
        media: Vec::new(),
    }
}

fn setup(spelling: &Spelling) -> SetupDocuments {
    let favorites = |ids: &[&str]| serde_json::to_string(ids).unwrap();
    SetupDocuments {
        version: SETUP_DOCUMENTS_VERSION,
        configuration: configuration(spelling),
        global_values: BTreeMap::new(),
        clip_hotkeys: Vec::new(),
        tts: TtsDocument {
            pipeline: TtsPipelineSettings::default(),
            filter_rules: Vec::new(),
        },
        voices: VoicesDocument {
            aliases: Vec::new(),
            strategy: AssignmentStrategy::default(),
            ignore_profile: IgnoreProfile::default(),
        },
        settings: BTreeMap::from([
            (
                PICKER_FAVORITES_SUB_ACTIONS.name().to_owned(),
                favorites(&[UNRELATED_KIND, spelling.step, RETIRED_TRIGGER]),
            ),
            (
                PICKER_FAVORITES_TRIGGERS.name().to_owned(),
                favorites(&[spelling.trigger, RETIRED_STEP]),
            ),
            ("ui.note".to_owned(), favorites(&[RETIRED_STEP])),
        ]),
    }
}

fn entry(path: &str, bytes: &[u8]) -> ContentEntry {
    ContentEntry {
        path: path.to_owned(),
        size: bytes.len() as u64,
        sha256_hex: Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
    }
}

fn bundle(dir: &Path, manifest: &[u8], documents_entry: &str, documents: &[u8]) -> PathBuf {
    let mut builder = tar::Builder::new(Vec::new());
    for (name, bytes) in [
        (MANIFEST_ENTRY_NAME, manifest),
        (documents_entry, documents),
    ] {
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Regular);
        header.set_size(bytes.len() as u64);
        header.set_mode(0o600);
        builder.append_data(&mut header, name, bytes).unwrap();
    }
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    gz.write_all(&builder.into_inner().unwrap()).unwrap();
    let path = dir.join(format!("{}.forgebundle", ulid::Ulid::generate()));
    std::fs::write(&path, gz.finish().unwrap()).unwrap();
    path
}

fn pack_bundle(dir: &Path, documents: &PackDocuments) -> PathBuf {
    let documents = serde_json::to_vec(documents).unwrap();
    let manifest = serde_json::to_vec(&PackManifest {
        format_version: BUNDLE_FORMAT_VERSION,
        kind: BundleKind::Pack,
        forge_version: "0.0.0".to_owned(),
        schema_version: EXPECTED_SCHEMA_VERSION,
        created_at: OffsetDateTime::UNIX_EPOCH,
        name: "renamed".to_owned(),
        author: String::new(),
        description: String::new(),
        required_integrations: Vec::new(),
        external_references: Vec::new(),
        unresolved_references: Vec::new(),
        unknown_kinds: Vec::new(),
        contents: vec![entry(PACK_DOCUMENTS_ENTRY, &documents)],
    })
    .unwrap();
    bundle(dir, &manifest, PACK_DOCUMENTS_ENTRY, &documents)
}

fn setup_bundle(dir: &Path, documents: &SetupDocuments) -> PathBuf {
    let documents = serde_json::to_vec(documents).unwrap();
    let manifest = serde_json::to_vec(&SetupManifest {
        format_version: BUNDLE_FORMAT_VERSION,
        kind: BundleKind::Setup,
        forge_version: "0.0.0".to_owned(),
        schema_version: EXPECTED_SCHEMA_VERSION,
        created_at: OffsetDateTime::UNIX_EPOCH,
        contents: vec![entry(SETUP_DOCUMENTS_ENTRY, &documents)],
    })
    .unwrap();
    bundle(dir, &manifest, SETUP_DOCUMENTS_ENTRY, &documents)
}

#[tokio::test]
async fn a_staged_pack_upgrade_renames_retired_trigger_and_step_kinds_at_every_depth_only() {
    let dir = TempDir::new().unwrap();
    let path = pack_bundle(dir.path(), &configuration(&RETIRED));
    let registries = Registries::declaring_renames();
    let mut staged = stage_pack(path, dir.path().to_path_buf()).await.unwrap();

    staged.upgrade_retired_ids(registries.closure());

    assert_eq!(staged.documents(), &configuration(&CURRENT));
}

#[tokio::test]
async fn a_staged_setup_upgrade_renames_its_configuration_and_both_favorite_lists_only() {
    let dir = TempDir::new().unwrap();
    let path = setup_bundle(dir.path(), &setup(&RETIRED));
    let registries = Registries::declaring_renames();
    let mut staged = stage_setup(path, dir.path().to_path_buf()).await.unwrap();

    staged.upgrade_retired_ids(registries.closure());

    let expected = setup(&CURRENT);
    assert_eq!(
        (
            &staged.documents().configuration,
            &staged.documents().settings
        ),
        (&expected.configuration, &expected.settings)
    );
}
