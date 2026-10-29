#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use forge_events::{Event, EventPublisher};
use forge_registry::SubActionRegistry;
use forge_runtime::sub_action_runners::{
    register_core_sub_actions, register_scheduled_run_sub_actions,
};
use forge_runtime::{
    ActionCancelRegistry, Config, CoreSubActionDeps, DonationIngest, OverlayServiceCell,
    ScheduledRunsCell, SchedulerCell, ScriptRegistry, SpeakDispatchError, SpeakDispatcher,
    SpeechOrigin, TwitchEmoteLexicon, register_donation_sub_actions, register_latest_sub_actions,
    register_speech_sub_actions,
};
use forge_storage::{
    DataProvider, GlobalsRepo, MockDonationRepo, ScriptRepo, SettingsRepo, UserGlobalsRepo,
};
use forge_storage_sqlite::SqliteBackend;
use forge_types::{LatestScope, LatestValue, LatestValueReader, Shared};
use serde_json::Value;
use tempfile::TempDir;

struct NullPublisher;

impl EventPublisher for NullPublisher {
    fn publish(&self, _event: Event) {}
}

struct SilentSpeaker;

#[async_trait]
impl SpeakDispatcher for SilentSpeaker {
    async fn speak(
        &self,
        _text: String,
        _voice: Option<String>,
        _origin: SpeechOrigin,
    ) -> Result<(), SpeakDispatchError> {
        Ok(())
    }
}

struct NoLatestValues;

impl LatestValueReader for NoLatestValues {
    fn latest(&self, _slot: &str, _scope: LatestScope<'_>) -> Option<LatestValue> {
        None
    }
}

async fn every_runtime_step() -> (SubActionRegistry, TempDir) {
    let media = tempfile::tempdir().unwrap();
    let backend =
        SqliteBackend::open_for_test(":memory:", [0x5e; 32], media.path().join("media"), None)
            .await
            .unwrap();
    let dp: Arc<dyn DataProvider> = Arc::new(backend);
    let publisher: Arc<dyn EventPublisher> = Arc::new(NullPublisher);
    let mut registry = SubActionRegistry::new();
    register_core_sub_actions(
        &mut registry,
        CoreSubActionDeps {
            globals: Arc::clone(&dp) as Arc<dyn GlobalsRepo>,
            user_globals: Arc::clone(&dp) as Arc<dyn UserGlobalsRepo>,
            scripts: Arc::new(ScriptRegistry::new()),
            publisher: Arc::clone(&publisher),
            settings: Arc::clone(&dp) as Arc<dyn SettingsRepo>,
            scheduler: SchedulerCell::new(),
            trigger_instances: dp.trigger_instance_repo(),
            actions: dp.action_repo(),
            script_repo: Arc::new(forge_storage::script::MockScriptRepo::new())
                as Arc<dyn ScriptRepo>,
            cancel_registry: Arc::new(ActionCancelRegistry::new()),
            overlays: OverlayServiceCell::new(),
            config: Config::default(),
        },
    )
    .unwrap();
    register_scheduled_run_sub_actions(&mut registry, ScheduledRunsCell::new()).unwrap();
    register_speech_sub_actions(
        &mut registry,
        Arc::new(SilentSpeaker),
        TwitchEmoteLexicon::default(),
    )
    .unwrap();
    register_latest_sub_actions(&mut registry, Arc::new(NoLatestValues)).unwrap();
    register_donation_sub_actions(
        &mut registry,
        Arc::new(DonationIngest::new(
            Arc::new(MockDonationRepo::new()),
            publisher,
            Shared::new(String::new()),
        )),
    )
    .unwrap();
    (registry, media)
}

fn stored_defaults(registry: &SubActionRegistry) -> BTreeMap<String, Value> {
    registry
        .all()
        .map(|runner| {
            (
                runner.id().to_owned(),
                serde_json::to_value(runner.default_config()).unwrap(),
            )
        })
        .collect()
}

fn golden() -> BTreeMap<String, Value> {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/step_default_configs.json");
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

#[tokio::test]
async fn every_pinned_step_still_seeds_the_same_stored_default_config() {
    let (registry, _media) = every_runtime_step().await;
    let current = stored_defaults(&registry);

    let drifted: Vec<_> = golden()
        .into_iter()
        .filter(|(kind, pinned)| current.get(kind) != Some(pinned))
        .map(|(kind, pinned)| (kind.clone(), pinned, current.get(&kind).cloned()))
        .collect();
    assert!(drifted.is_empty(), "{drifted:#?}");
}

#[tokio::test]
async fn every_registered_runtime_step_has_a_pinned_stored_default() {
    let (registry, _media) = every_runtime_step().await;
    let pinned = golden();

    let unpinned: Vec<_> = stored_defaults(&registry)
        .into_keys()
        .filter(|kind| !pinned.contains_key(kind))
        .collect();
    assert!(unpinned.is_empty(), "{unpinned:?}");
}
