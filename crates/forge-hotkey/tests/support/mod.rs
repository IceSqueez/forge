#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

use forge_events::test_support::NullPublisher;
use std::collections::BTreeMap;
use std::sync::Arc;

use forge_hotkey::testing::{RecordingBackend, test_client};
use forge_hotkey::{COMBO_FIELD, HotkeyClient, HotkeyConfig};
use forge_storage::{DataProvider, StoredClip};
use forge_types::{
    Action, ActionId, ClipId, ExecutionMode, OutputDevice, PermissionRung, PlatformScope,
    TriggerInstance, TriggerInstanceId, Variant,
};
use tempfile::TempDir;
use time::OffsetDateTime;

pub const OTHER_KIND: &str = "midi.note_on";
const TEST_KEY: [u8; 32] = [0x11; 32];

pub struct Store {
    pub provider: Arc<dyn DataProvider>,
    _media_root: TempDir,
}

pub async fn store() -> Store {
    let media_root = tempfile::tempdir().expect("a temporary media root");
    let backend = forge_storage_sqlite::SqliteBackend::open_for_test(
        "sqlite::memory:",
        TEST_KEY,
        media_root.path().to_owned(),
        None,
    )
    .await
    .expect("an in-memory backend");
    Store {
        provider: Arc::new(backend),
        _media_root: media_root,
    }
}

pub async fn seed_action(backend: &Arc<dyn DataProvider>, name: &str) -> ActionId {
    let queue_id = backend
        .queue_repo()
        .get_by_name("Default")
        .await
        .unwrap()
        .expect("migrations seed the default queue")
        .id;
    let action = Action {
        id: ActionId::new(),
        name: name.to_owned(),
        group: None,
        queue_id,
        enabled: true,
        concurrent: false,
        bypass_pause: false,
        execution_mode: ExecutionMode::Sequential,
        description: None,
        sub_actions: vec![],
    };
    backend.action_repo().save(&action).await.unwrap();
    action.id
}

pub fn instance(kind_id: &str, combo: Option<&str>) -> TriggerInstance {
    let overrides = combo
        .map(|combo| BTreeMap::from([(COMBO_FIELD.to_owned(), Variant::String(combo.to_owned()))]))
        .unwrap_or_default();
    TriggerInstance {
        id: TriggerInstanceId::new(),
        kind_id: kind_id.to_owned(),
        name: combo.unwrap_or("no combo").to_owned(),
        overrides,
        enabled: true,
        user_defined: true,
        platform_scope: PlatformScope::default(),
        cooldown_secs: 0,
        cooldown_global: true,
        permission_rung: PermissionRung::Everyone,
    }
}

pub fn with_combo(mut instance: TriggerInstance, combo: &str) -> TriggerInstance {
    instance
        .overrides
        .insert(COMBO_FIELD.to_owned(), Variant::String(combo.to_owned()));
    instance
}

pub async fn seed_instance(
    backend: &Arc<dyn DataProvider>,
    kind_id: &str,
    combo: Option<&str>,
) -> TriggerInstanceId {
    let instance = instance(kind_id, combo);
    backend
        .trigger_instance_repo()
        .save(&instance)
        .await
        .unwrap();
    instance.id
}

pub async fn seed_disabled_instance(
    backend: &Arc<dyn DataProvider>,
    kind_id: &str,
    combo: &str,
) -> TriggerInstanceId {
    let id = seed_instance(backend, kind_id, Some(combo)).await;
    backend
        .trigger_instance_repo()
        .set_enabled(id, false)
        .await
        .unwrap();
    id
}

pub fn clip(hotkey: Option<&str>) -> StoredClip {
    StoredClip {
        id: ClipId::new(),
        name: "airhorn".to_owned(),
        file_path: "/clips/airhorn.wav".into(),
        volume: 1.0,
        output_device: OutputDevice::Default,
        hotkey: hotkey.map(str::to_owned),
        created_at: OffsetDateTime::now_utc(),
        category: String::new(),
        loop_playback: false,
        duration_secs: None,
        builtin_id: None,
    }
}

pub fn stored_combo(instance: &TriggerInstance) -> Option<&str> {
    match instance.overrides.get(COMBO_FIELD) {
        Some(Variant::String(combo)) => Some(combo),
        _ => None,
    }
}

pub fn stored_combos(instances: &[TriggerInstance]) -> Vec<(String, Option<String>)> {
    let mut combos: Vec<(String, Option<String>)> = instances
        .iter()
        .map(|instance| {
            (
                instance.kind_id.clone(),
                stored_combo(instance).map(str::to_owned),
            )
        })
        .collect();
    combos.sort();
    combos
}

pub fn recording_client() -> (Arc<HotkeyClient>, Arc<RecordingBackend>) {
    test_client(HotkeyConfig::default(), Arc::new(NullPublisher))
}

pub fn registered(client: &HotkeyClient) -> Vec<String> {
    let mut combos: Vec<String> = client
        .registered_combos()
        .into_iter()
        .map(|(_, combo)| combo.as_str().to_owned())
        .collect();
    combos.sort();
    combos
}
