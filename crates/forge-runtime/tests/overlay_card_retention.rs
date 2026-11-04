#![expect(clippy::expect_used, clippy::unwrap_used, reason = "test fixtures")]
mod common;
use std::sync::{Arc, Mutex};

use forge_overlay::config::{HEADLINE, SPEECH, SUBLINE, TEXT_COLOR};
use forge_runtime::{EventBus, NullEventLogRepo, OverlayServiceHandle, OverlayServiceParts};
use forge_storage::test_support::MemorySettings;
use forge_storage::{
    MockOverlayRepo, OverlayConfig, OverlayDefinition, OverlayId, OverlayRepo, SettingsRepo,
};
use forge_types::Variant;

const CARD_KIND: &str = forge_overlay::kinds::card::KIND_ID;
const ALERT_KIND: &str = forge_overlay::kinds::alert::KIND_ID;
const OVERLAY: &str = "panel";

type Retained = Arc<Mutex<Vec<OverlayConfig>>>;

struct Rig {
    service: OverlayServiceHandle,
    retained: Retained,
}

fn text(value: &str) -> Variant {
    Variant::String(value.to_owned())
}

fn one(key: &str, value: &str) -> OverlayConfig {
    OverlayConfig::from([(key.to_owned(), text(value))])
}

fn rig(kind_id: &str, saved: OverlayConfig, stores: bool) -> Rig {
    let definition: OverlayDefinition =
        common::overlay_definition(OVERLAY, "Panel", kind_id, saved);
    let retained: Retained = Arc::default();

    let mut repo = MockOverlayRepo::new();
    repo.expect_get()
        .returning(move |_| Ok(Some(definition.clone())));
    repo.expect_update_config()
        .returning(move |_, _| Ok(stores));
    let writes = Arc::clone(&retained);
    repo.expect_set_retained_content()
        .returning(move |_, content| {
            writes.lock().unwrap().push(content.clone());
            Ok(())
        });

    let service = OverlayServiceHandle::new(OverlayServiceParts {
        default_root: std::path::PathBuf::new(),
        date_names: forge_runtime::DateNamesHandle::default(),
        tasks: forge_net::TaskOwner::new(),
        repo: Arc::new(repo) as Arc<dyn OverlayRepo>,
        settings: Arc::new(MemorySettings::new()) as Arc<dyn SettingsRepo>,
        kinds: common::builtin_overlay_kinds(),
        bus: EventBus::new(Arc::new(NullEventLogRepo)),
        frames: common::no_pages(),
        media: None,
        speaker: None,
        latest: None,
    });
    Rig { service, retained }
}

impl Rig {
    async fn save(&self, config: &OverlayConfig) -> Vec<OverlayConfig> {
        self.service
            .update_config(&OverlayId::new(OVERLAY), config)
            .await
            .expect("a valid config saves");
        self.retained.lock().unwrap().clone()
    }
}

#[tokio::test]
async fn new_card_words_become_what_a_reconnecting_page_opens_on() {
    let rig = rig(CARD_KIND, OverlayConfig::new(), true);
    let mut edited = one(HEADLINE, "Starting soon");
    edited.insert(SPEECH.to_owned(), text("We start soon"));

    let retained = rig.save(&edited).await;

    let [content] = retained.as_slice() else {
        panic!("expected one retained write, got {retained:?}");
    };
    assert_eq!(
        (
            content.get(HEADLINE),
            content.get(SUBLINE),
            content.contains_key(SPEECH)
        ),
        (
            Some(&text("Starting soon")),
            Some(&text("Say hi in chat")),
            false
        )
    );
}

#[tokio::test]
async fn restyling_a_card_keeps_the_content_it_last_showed() {
    let rig = rig(CARD_KIND, OverlayConfig::new(), true);

    let retained = rig.save(&one(TEXT_COLOR, "#ff8800")).await;

    assert!(
        retained.is_empty(),
        "a style edit replaced delivered content"
    );
}

#[tokio::test]
async fn a_card_config_the_store_did_not_take_retains_nothing() {
    let rig = rig(CARD_KIND, OverlayConfig::new(), false);

    let retained = rig.save(&one(HEADLINE, "Starting soon")).await;

    assert!(
        retained.is_empty(),
        "content was retained for a config that was never stored"
    );
}

#[tokio::test]
async fn editing_an_alerts_words_leaves_its_retained_content_alone() {
    let rig = rig(ALERT_KIND, OverlayConfig::new(), true);

    let retained = rig.save(&one(HEADLINE, "New alert words")).await;

    assert!(
        retained.is_empty(),
        "an alert panel edit was retained as if it were on screen"
    );
}
