#![expect(clippy::expect_used, clippy::unwrap_used, reason = "test fixtures")]
mod common;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use forge_overlay::config::HEADLINE;
use forge_runtime::{
    EventBus, NullEventLogRepo, OverlayFrameSink, OverlayReceivers, OverlaySendOptions,
    OverlayServiceHandle, OverlayServiceParts,
};
use forge_storage::test_support::MemorySettings;
use forge_storage::{MockOverlayRepo, OverlayConfig, OverlayId, OverlayRepo, SettingsRepo};
use forge_types::{ArgStack, Variant};
use serde_json::Value;

const CARD_KIND: &str = forge_overlay::kinds::card::KIND_ID;
const OVERLAY: &str = "panel";
const CARD_HEADLINE: &str = "Hi %user_name%";
const REAL_EVENT_HEADLINE: &str = "Olena just followed";
const PREVIEW_HOLD: Duration =
    Duration::from_secs(forge_overlay::DEFAULT_DISPLAY_SECS.unsigned_abs());
const HOLD_SLACK: Duration = Duration::from_millis(10);

#[derive(Default)]
struct RecordingSink {
    frames: Mutex<Vec<Value>>,
}

impl RecordingSink {
    fn headlines(&self) -> Vec<String> {
        self.frames
            .lock()
            .unwrap()
            .iter()
            .map(|content| content[HEADLINE].as_str().unwrap_or_default().to_owned())
            .collect()
    }

    fn count(&self) -> usize {
        self.frames.lock().unwrap().len()
    }
}

#[async_trait]
impl OverlayFrameSink for RecordingSink {
    async fn deliver_content(
        &self,
        _: &OverlayId,
        content: Value,
        _: Option<u64>,
    ) -> OverlayReceivers {
        self.frames.lock().unwrap().push(content);
        OverlayReceivers {
            sources: 1,
            preview_tabs: 0,
        }
    }

    async fn deliver_reload(&self, _: &OverlayId) {}

    async fn revoke(&self, _: &OverlayId) {}
}

type RetainedStore = Arc<Mutex<HashMap<OverlayId, OverlayConfig>>>;

struct Rig {
    sink: Arc<RecordingSink>,
    retained: RetainedStore,
    service: OverlayServiceHandle,
}

fn card_config() -> OverlayConfig {
    OverlayConfig::from([(
        HEADLINE.to_owned(),
        Variant::String(CARD_HEADLINE.to_owned()),
    )])
}

fn rig() -> Rig {
    let definition = common::overlay_definition(OVERLAY, "Panel", CARD_KIND, card_config());
    let retained: RetainedStore = Arc::default();

    let mut repo = MockOverlayRepo::new();
    repo.expect_get()
        .returning(move |_| Ok(Some(definition.clone())));
    let writes = Arc::clone(&retained);
    repo.expect_set_retained_content()
        .returning(move |id, content| {
            writes.lock().unwrap().insert(id.clone(), content.clone());
            Ok(())
        });
    let reads = Arc::clone(&retained);
    repo.expect_get_retained_content()
        .returning(move |id| Ok(reads.lock().unwrap().get(id).cloned()));

    let sink = Arc::new(RecordingSink::default());
    let service = OverlayServiceHandle::new(OverlayServiceParts {
        default_root: std::path::PathBuf::new(),
        date_names: forge_runtime::DateNamesHandle::default(),
        tasks: forge_net::TaskOwner::new(),
        repo: Arc::new(repo) as Arc<dyn OverlayRepo>,
        settings: Arc::new(MemorySettings::new()) as Arc<dyn SettingsRepo>,
        kinds: common::builtin_overlay_kinds(),
        bus: EventBus::new(Arc::new(NullEventLogRepo)),
        frames: Arc::clone(&sink) as Arc<dyn OverlayFrameSink>,
        media: None,
        speaker: None,
        latest: None,
    });
    Rig {
        sink,
        retained,
        service,
    }
}

impl Rig {
    async fn show_real_event(&self) {
        self.service
            .send_to(
                &OverlayId::new(OVERLAY),
                &OverlayConfig::from([(
                    HEADLINE.to_owned(),
                    Variant::String(REAL_EVENT_HEADLINE.to_owned()),
                )]),
                &ArgStack::new(),
                OverlaySendOptions::default(),
            )
            .await
            .expect("a card accepts content");
    }

    async fn test_fire(&self) -> String {
        let fired = self
            .service
            .test_fire(&OverlayId::new(OVERLAY), &common::NeutralSamples)
            .await
            .expect("a card can be test fired");
        headline_text(&fired.content).expect("the sample carries headline text")
    }

    fn retained_headline(&self) -> Option<Variant> {
        self.retained
            .lock()
            .unwrap()
            .get(&OverlayId::new(OVERLAY))
            .and_then(|content| content.get(HEADLINE).cloned())
    }

    async fn wait_for_frames(&self, at_least: usize) {
        tokio::time::timeout(HOLD_SLACK, async {
            while self.sink.count() < at_least {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        })
        .await
        .expect("the restored content reached the page");
    }
}

fn headline_text(content: &OverlayConfig) -> Option<String> {
    match content.get(HEADLINE) {
        Some(Variant::String(headline)) => Some(headline.clone()),
        _ => None,
    }
}

fn opening_headline() -> String {
    let kinds = common::builtin_overlay_kinds();
    let opening = kinds
        .get(CARD_KIND)
        .expect("the card kind ships")
        .opening_content(&card_config())
        .expect("a card always has opening content");
    headline_text(&opening).expect("the opening content carries headline text")
}

#[tokio::test]
async fn a_card_sample_preview_leaves_the_real_event_as_its_retained_content() {
    let rig = rig();
    rig.show_real_event().await;

    rig.test_fire().await;

    assert_eq!(
        rig.retained_headline(),
        Some(Variant::String(REAL_EVENT_HEADLINE.to_owned())),
        "the sample replaced what a reconnecting page opens on"
    );
}

#[tokio::test(start_paused = true)]
async fn a_card_returns_to_the_real_event_once_the_preview_hold_ends() {
    let rig = rig();
    rig.show_real_event().await;
    let sample = rig.test_fire().await;

    tokio::time::sleep(PREVIEW_HOLD - Duration::from_millis(1)).await;
    let during_hold = rig.sink.headlines();
    tokio::time::sleep(Duration::from_millis(2)).await;
    rig.wait_for_frames(3).await;

    assert_ne!(
        sample, REAL_EVENT_HEADLINE,
        "the sample must be distinguishable"
    );
    assert_eq!(
        during_hold,
        [REAL_EVENT_HEADLINE.to_owned(), sample.clone()],
        "the preview was cut short"
    );
    assert_eq!(
        rig.sink.headlines(),
        [
            REAL_EVENT_HEADLINE.to_owned(),
            sample,
            REAL_EVENT_HEADLINE.to_owned()
        ]
    );
}

#[tokio::test(start_paused = true)]
async fn a_card_that_showed_nothing_returns_to_its_opening_content_after_a_preview() {
    let rig = rig();
    let sample = rig.test_fire().await;

    tokio::time::sleep(PREVIEW_HOLD + Duration::from_millis(1)).await;
    rig.wait_for_frames(2).await;

    let opening = opening_headline();
    assert_ne!(sample, opening, "the sample must be distinguishable");
    assert_eq!(rig.sink.headlines(), [sample, opening]);
    assert!(
        rig.retained_headline().is_none(),
        "the sample was stored as the card's retained content"
    );
}
