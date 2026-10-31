#![allow(clippy::unwrap_used)]
mod common;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use forge_events::EventPublisher;
use forge_registry::SubActionRegistry;
use forge_runtime::{
    DonationIngest, EventBus, NullEventLogRepo, RuntimeWiring, SpeakDispatchError, SpeakDispatcher,
    SpeechOrigin, TwitchEmoteLexicon, register_donation_sub_actions, register_latest_sub_actions,
    register_speech_sub_actions,
};
use forge_storage::{DataProvider, MockDonationRepo};
use forge_types::{LatestScope, LatestValue, LatestValueReader, Shared};
use serde_json::Value;
use tempfile::TempDir;

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

async fn every_runtime_step() -> (Arc<SubActionRegistry>, TempDir) {
    let (backend, media) = common::sqlite_backend().await;
    let dp: Arc<dyn DataProvider> = Arc::new(backend);
    let bus = EventBus::new(Arc::new(NullEventLogRepo));
    let publisher = Arc::clone(&bus) as Arc<dyn EventPublisher>;
    let mut wiring =
        common::with_every_core_step(RuntimeWiring::new(common::wiring_parts(dp, bus).await));
    register_speech_sub_actions(
        wiring.sub_actions_mut(),
        Arc::new(SilentSpeaker),
        TwitchEmoteLexicon::default(),
    )
    .unwrap();
    register_latest_sub_actions(wiring.sub_actions_mut(), Arc::new(NoLatestValues)).unwrap();
    register_donation_sub_actions(
        wiring.sub_actions_mut(),
        Arc::new(DonationIngest::new(
            Arc::new(MockDonationRepo::new()),
            publisher,
            Shared::new(String::new()),
        )),
    )
    .unwrap();
    (wiring.freeze().await.sub_action_registry, media)
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
