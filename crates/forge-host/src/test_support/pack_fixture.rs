use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use forge_overlay::{OverlayKindRegistry, register_builtin_kinds};
use forge_paths::{CREDENTIALS_KEY_FILE_NAME, MEDIA_DIR_NAME, OVERLAYS_DIR_NAME};
use forge_registry::{InternalNamespace, ReferenceCatalog, SubActionRegistry, TriggerRegistry};
use forge_storage::{
    CredentialId, DATABASE_FILE_NAME, DataProvider, MediaBlobId, MediaReferrer, MediaReferrerKind,
    OverlayId, ScriptRecord, ScriptRepo, StoredClip, clip_source_referrer,
};
use forge_storage_sqlite::SqliteBackend;
use forge_transfer::ClosureRegistries;
use forge_types::{
    Action, ActionId, ClipId, ExecutionMode, OutputDevice, PermissionRung, PlatformScope, Queue,
    QueueId, ScriptContract, ScriptId, SubActionStep, TriggerInstance, TriggerInstanceId, Variant,
};
use tempfile::TempDir;
use time::OffsetDateTime;

use super::every_registry_entry;

pub const STORAGE_KEY: [u8; 32] = [0x11; 32];

pub const QUEUE_NAME: &str = "Alerts queue";
pub const SCRIPT_NAME: &str = "greet";
pub const SCRIPT_BODY: &str = "let greeting = 1;";
pub const GLOBAL_NAME: &str = "score";
pub const CLIP_NAME: &str = "Ding";
pub const OVERLAY_NAME: &str = "Alert box";
pub const OVERLAY_KIND: &str = "overlay.alert";
pub const OVERRIDE_FILE: &str = "custom.css";
pub const OVERRIDE_BYTES: &[u8] = b"body { color: red; }";
pub const OGG: &[u8] = b"OggS\x00\x02\x00\x00ding";
pub const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\x00\x00\x00\x0dicon";
pub const TRIGGER_KIND: &str = "script.event.custom";
pub const SCENE: &str = "Live";
pub const SCENE_STEP: &str = "obs.scene.set_preview";
pub const HTTP_STEP: &str = "core.http.get";
pub const SCRIPT_STEP: &str = "script.run.named";
pub const SECRET_TOKEN: &str = "secret-broadcaster-token-7f3a";
pub const SECRET_SETTING: &str = "secret-setting-value-91bc";

pub type Config = BTreeMap<String, Variant>;

pub struct Registries {
    pub triggers: Arc<TriggerRegistry>,
    pub sub_actions: Arc<SubActionRegistry>,
    pub overlays: OverlayKindRegistry,
}

impl Registries {
    pub async fn real() -> Self {
        let (triggers, sub_actions) = every_registry_entry().await;
        let mut overlays = OverlayKindRegistry::new();
        register_builtin_kinds(&mut overlays).unwrap();
        Self {
            triggers,
            sub_actions,
            overlays,
        }
    }

    pub fn closure(&self) -> ClosureRegistries<'_> {
        ClosureRegistries {
            sub_actions: &self.sub_actions,
            triggers: &self.triggers,
            overlays: &self.overlays,
        }
    }

    pub fn catalog(&self) -> ReferenceCatalog<'_> {
        ReferenceCatalog::new(&self.sub_actions, &self.triggers)
    }
}

pub struct Store {
    dir: TempDir,
    pub provider: Arc<dyn DataProvider>,
    pub sqlite: Arc<SqliteBackend>,
}

impl Store {
    pub async fn open() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let url = format!("sqlite://{}", dir.path().join(DATABASE_FILE_NAME).display());
        let backend =
            SqliteBackend::open_for_test(&url, STORAGE_KEY, dir.path().join(MEDIA_DIR_NAME), None)
                .await
                .unwrap();
        std::fs::write(dir.path().join(CREDENTIALS_KEY_FILE_NAME), STORAGE_KEY).unwrap();
        let sqlite = Arc::new(backend);
        Self {
            dir,
            provider: Arc::clone(&sqlite) as Arc<dyn DataProvider>,
            sqlite,
        }
    }

    pub fn data_dir(&self) -> &Path {
        self.dir.path()
    }

    pub fn overlay_root(&self) -> PathBuf {
        self.dir.path().join(OVERLAYS_DIR_NAME)
    }

    pub async fn action(&self, id: ActionId) -> Option<Action> {
        self.provider.action_repo().get(id).await.unwrap()
    }

    pub async fn trigger(&self, id: TriggerInstanceId) -> Option<TriggerInstance> {
        self.provider.trigger_instance_repo().get(id).await.unwrap()
    }

    pub async fn linked(&self, action: ActionId) -> Vec<TriggerInstanceId> {
        self.provider
            .trigger_instance_repo()
            .list_for_action(action)
            .await
            .unwrap()
            .into_iter()
            .map(|trigger| trigger.id)
            .collect()
    }

    pub async fn script(&self, name: &str) -> Option<ScriptRecord> {
        self.provider.script_repo().get_by_name(name).await.unwrap()
    }

    pub async fn overlay_named(&self, name: &str) -> forge_storage::OverlayDefinition {
        self.provider
            .overlay_repo()
            .list()
            .await
            .unwrap()
            .into_iter()
            .find(|overlay| overlay.display_name == name)
            .unwrap()
    }

    pub async fn clip_blob(&self, clip: ClipId) -> Option<MediaBlobId> {
        self.provider
            .media_repo()
            .blob_of(&clip_source_referrer(clip))
            .await
            .unwrap()
    }

    pub async fn holds(&self, namespace: InternalNamespace, value: &str) -> bool {
        let p = self.provider.as_ref();
        match namespace {
            InternalNamespace::Action => self.action(value.parse().unwrap()).await.is_some(),
            InternalNamespace::TriggerInstance => {
                self.trigger(value.parse().unwrap()).await.is_some()
            }
            InternalNamespace::Queue => p
                .queue_repo()
                .list()
                .await
                .unwrap()
                .iter()
                .any(|queue| queue.id.to_string() == value),
            InternalNamespace::Script => self.script(value).await.is_some(),
            InternalNamespace::Global => p.globals_repo().get(value).await.unwrap().is_some(),
            InternalNamespace::SoundboardClip => p
                .soundboard_clips_repo()
                .list()
                .await
                .unwrap()
                .iter()
                .any(|clip| clip.id.to_string() == value),
            InternalNamespace::Overlay => p
                .overlay_repo()
                .get(&OverlayId::new(value))
                .await
                .unwrap()
                .is_some(),
            InternalNamespace::ManagedMedia => p
                .media_repo()
                .get(&MediaBlobId::from_stored(value).unwrap())
                .await
                .unwrap()
                .is_some(),
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct StoreState {
    actions: Vec<Action>,
    links: Vec<(ActionId, Vec<TriggerInstanceId>)>,
    triggers: Vec<TriggerInstance>,
    queues: Vec<Queue>,
    scripts: Vec<(ScriptId, String, String, bool)>,
    globals: Vec<(String, Variant, bool)>,
    clips: Vec<StoredClip>,
    overlays: Vec<(String, String, Config, Vec<String>)>,
    media: Vec<String>,
    overlay_files: Vec<(PathBuf, Vec<u8>)>,
}

pub async fn state_of(store: &Store) -> StoreState {
    let p = store.provider.as_ref();
    let actions = p.action_repo().list().await.unwrap();
    let mut links = Vec::new();
    for action in &actions {
        links.push((action.id, store.linked(action.id).await));
    }
    let mut scripts: Vec<_> = p
        .script_repo()
        .list()
        .await
        .unwrap()
        .into_iter()
        .map(|s| (s.id, s.name, s.body, s.enabled))
        .collect();
    scripts.sort();
    let mut globals: Vec<_> = p
        .globals_repo()
        .list()
        .await
        .unwrap()
        .into_iter()
        .map(|g| (g.name, g.value, g.persisted))
        .collect();
    globals.sort_by(|a, b| a.0.cmp(&b.0));
    let mut overlays: Vec<_> = p
        .overlay_repo()
        .list()
        .await
        .unwrap()
        .into_iter()
        .map(|o| {
            (
                o.id.as_str().to_owned(),
                o.display_name,
                o.config,
                o.source_overrides,
            )
        })
        .collect();
    overlays.sort_by(|a, b| a.0.cmp(&b.0));
    let mut media: Vec<_> = p
        .media_repo()
        .list()
        .await
        .unwrap()
        .into_iter()
        .map(|blob| blob.id.as_str().to_owned())
        .collect();
    media.sort();
    StoreState {
        actions,
        links,
        triggers: p.trigger_instance_repo().list_all().await.unwrap(),
        queues: p.queue_repo().list().await.unwrap(),
        scripts,
        globals,
        clips: p.soundboard_clips_repo().list().await.unwrap(),
        overlays,
        media,
        overlay_files: files_under(&store.overlay_root()),
    }
}

pub fn files_under(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut out = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else {
                let bytes = std::fs::read(&path).unwrap();
                out.push((path.strip_prefix(root).unwrap().to_path_buf(), bytes));
            }
        }
    }
    out.sort();
    out
}

pub fn text(value: &str) -> Variant {
    Variant::String(value.to_owned())
}

pub fn config(pairs: &[(&str, Variant)]) -> Config {
    pairs
        .iter()
        .map(|(key, value)| ((*key).to_owned(), value.clone()))
        .collect()
}

pub fn step(kind_id: &str, step_config: Config) -> SubActionStep {
    SubActionStep {
        kind_id: kind_id.to_owned(),
        config: step_config,
        enabled: true,
        continue_on_error: false,
        condition: None,
        label: None,
    }
}

pub fn action(id: ActionId, name: &str, queue_id: QueueId, steps: Vec<SubActionStep>) -> Action {
    Action {
        id,
        name: name.to_owned(),
        group: Some("Pack group".to_owned()),
        queue_id,
        enabled: true,
        concurrent: false,
        bypass_pause: false,
        execution_mode: ExecutionMode::default(),
        description: Some(format!("{name} description")),
        sub_actions: steps,
    }
}

pub fn trigger(name: &str, enabled: bool) -> TriggerInstance {
    TriggerInstance {
        id: TriggerInstanceId::new(),
        kind_id: TRIGGER_KIND.to_owned(),
        name: name.to_owned(),
        overrides: Config::new(),
        enabled,
        user_defined: true,
        platform_scope: PlatformScope::default(),
        cooldown_secs: 5,
        cooldown_global: false,
        permission_rung: PermissionRung::Everyone,
    }
}

pub fn last_action_id() -> ActionId {
    "7ZZZZZZZZZZZZZZZZZZZZZZZZZ"
        .parse()
        .expect("the largest representable id")
}

pub struct Fixture {
    pub root: ActionId,
    pub helper: ActionId,
    pub unrelated: ActionId,
    pub root_trigger: TriggerInstanceId,
    pub helper_trigger: TriggerInstanceId,
    pub toggled_trigger: TriggerInstanceId,
    pub queue: QueueId,
    pub clip: ClipId,
    pub overlay: OverlayId,
    pub ding: MediaBlobId,
    pub icon: MediaBlobId,
}

pub async fn seed(store: &Store) -> Fixture {
    let p = store.provider.as_ref();
    let now = OffsetDateTime::now_utc();
    let queue = Queue {
        id: QueueId::new(),
        name: QUEUE_NAME.to_owned(),
        description: "pack queue".to_owned(),
        concurrency: std::num::NonZeroU32::new(2).unwrap(),
    };
    p.queue_repo().save(&queue).await.unwrap();
    ScriptRepo::save(
        p.script_repo().as_ref(),
        ScriptRecord {
            id: ScriptId::new(),
            name: SCRIPT_NAME.to_owned(),
            body: SCRIPT_BODY.to_owned(),
            contract: ScriptContract::default(),
            enabled: true,
            created_at: now,
            last_modified: now,
        },
    )
    .await
    .unwrap();
    p.globals_repo()
        .set(GLOBAL_NAME, Variant::Int(7), true)
        .await
        .unwrap();

    let media = p.media_repo();
    let ding = media.store("ding.ogg", OGG.to_vec()).await.unwrap().id;
    let icon = media.store("icon.png", PNG.to_vec()).await.unwrap().id;
    let clip = StoredClip {
        id: ClipId::new(),
        name: CLIP_NAME.to_owned(),
        file_path: media.resolve(&ding).await.unwrap(),
        volume: 0.5,
        output_device: OutputDevice::Default,
        hotkey: None,
        created_at: now,
        category: "fx".to_owned(),
        loop_playback: false,
        duration_secs: Some(1.5),
        builtin_id: None,
    };
    p.soundboard_clips_repo().save(&clip).await.unwrap();
    media
        .retain(&clip_source_referrer(clip.id), &ding)
        .await
        .unwrap();

    let overlays = p.overlay_repo();
    let mut overlay = overlays
        .create(OVERLAY_NAME, OVERLAY_KIND, 1)
        .await
        .unwrap();
    overlay.config = config(&[
        ("sound", text(&format!("clip:{}", clip.id))),
        ("icon", text(&format!("image:{}", icon.as_str()))),
    ]);
    overlay.source_overrides = vec![OVERRIDE_FILE.to_owned()];
    overlays.save(&overlay).await.unwrap();
    media
        .retain(
            &MediaReferrer::new(MediaReferrerKind::Overlay, overlay.id.as_str(), "icon"),
            &icon,
        )
        .await
        .unwrap();
    let overlay_dir = store.overlay_root().join(overlay.id.as_str());
    std::fs::create_dir_all(&overlay_dir).unwrap();
    std::fs::write(overlay_dir.join(OVERRIDE_FILE), OVERRIDE_BYTES).unwrap();

    let triggers = p.trigger_instance_repo();
    let root_trigger = trigger("Root trigger", false);
    let helper_trigger = trigger("Helper trigger", true);
    let toggled_trigger = trigger("Toggled trigger", true);
    let unrelated_trigger = trigger("Unrelated trigger", true);
    for t in [
        &root_trigger,
        &helper_trigger,
        &toggled_trigger,
        &unrelated_trigger,
    ] {
        triggers.save(t).await.unwrap();
    }

    let actions = p.action_repo();
    let helper = action(
        ActionId::new(),
        "Helper",
        queue.id,
        vec![step(
            "core.globals.get",
            config(&[("name", text(GLOBAL_NAME))]),
        )],
    );
    let root = action(
        ActionId::new(),
        "Root",
        queue.id,
        vec![
            step(
                "core.action.run",
                config(&[("action_id", text(&helper.id.to_string()))]),
            ),
            step(
                "core.trigger.enable",
                config(&[("trigger_instance_id", text(&toggled_trigger.id.to_string()))]),
            ),
            step(SCRIPT_STEP, config(&[("script_name", text(SCRIPT_NAME))])),
            step(
                "core.globals.set",
                config(&[("name", text(GLOBAL_NAME)), ("value", text("1"))]),
            ),
            step(
                "soundboard.sound.play",
                config(&[("clip_id", text(&clip.id.to_string()))]),
            ),
            step(
                forge_runtime::OVERLAY_SEND_KIND_ID,
                config(&[(forge_runtime::OVERLAY_TARGET_KEY, text(overlay.id.as_str()))]),
            ),
            step(SCENE_STEP, config(&[("scene", text(SCENE))])),
            step(
                HTTP_STEP,
                config(&[("url", text("https://example.invalid/hook"))]),
            ),
        ],
    );
    let unrelated = action(ActionId::new(), "Unrelated", queue.id, Vec::new());
    for a in [&helper, &root, &unrelated] {
        actions.save(a).await.unwrap();
    }
    triggers
        .link_action(root.id, root_trigger.id)
        .await
        .unwrap();
    triggers
        .link_action(helper.id, helper_trigger.id)
        .await
        .unwrap();
    triggers
        .link_action(unrelated.id, unrelated_trigger.id)
        .await
        .unwrap();

    p.credentials_repo()
        .store(&CredentialId::new("twitch:broadcaster"), SECRET_TOKEN)
        .await
        .unwrap();
    store
        .sqlite
        .seed_foreign_setting("ui.last_note", SECRET_SETTING, None)
        .await
        .unwrap();

    Fixture {
        root: root.id,
        helper: helper.id,
        unrelated: unrelated.id,
        root_trigger: root_trigger.id,
        helper_trigger: helper_trigger.id,
        toggled_trigger: toggled_trigger.id,
        queue: queue.id,
        clip: clip.id,
        overlay: overlay.id,
        ding,
        icon,
    }
}
