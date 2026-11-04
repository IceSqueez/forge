#![expect(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "test fixtures"
)]
mod common;
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use forge_runtime::{
    EventBus, NullEventLogRepo, OverlayConnectFanout, OverlayConnectListener, OverlayFrameSink,
    OverlayPage, OverlayReceivers, OverlayServiceHandle, OverlayServiceParts,
};
use forge_storage::test_support::MemorySettings;
use forge_storage::{
    MockOverlayRepo, OverlayConfig, OverlayDefinition, OverlayId, OverlayRepo, SettingsRepo,
};
use forge_types::{
    LATEST_DONATION_SLOT, LatestScope, LatestValue, LatestValueReader, Variant, latest_fields,
};
use serde_json::Value;
use time::OffsetDateTime;

const GOAL_KIND: &str = "overlay.goal";
const CARD_KIND: &str = "overlay.card";
const LATEST_KIND: &str = "overlay.latest";
const HEADLINE: &str = "headline";
const VALUE_KEY: &str = "value";
const RETAINED_VALUE: &str = "42 of 100";
const LIVE_DONOR: &str = "Olena";
const SAMPLE_DONOR: &str = "PixelPal";
const PREVIEW_HOLD: Duration =
    Duration::from_secs(forge_overlay::DEFAULT_DISPLAY_SECS.unsigned_abs());
const WAIT_BOUND: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, PartialEq)]
enum Delivered {
    EveryPage(OverlayId, Value),
    OnePage(OverlayId, OverlayPage, Value),
}

#[derive(Default)]
struct RecordingSink(Mutex<Vec<Delivered>>);

impl RecordingSink {
    fn delivered(&self) -> Vec<Delivered> {
        self.0.lock().unwrap().clone()
    }
}

#[async_trait]
impl OverlayFrameSink for RecordingSink {
    async fn deliver_content(
        &self,
        identity: &OverlayId,
        content: Value,
        _: Option<u64>,
    ) -> OverlayReceivers {
        self.0
            .lock()
            .unwrap()
            .push(Delivered::EveryPage(identity.clone(), content));
        OverlayReceivers {
            sources: 2,
            preview_tabs: 0,
        }
    }

    async fn deliver_content_to_page(
        &self,
        identity: &OverlayId,
        page: OverlayPage,
        content: Value,
    ) -> OverlayReceivers {
        self.0
            .lock()
            .unwrap()
            .push(Delivered::OnePage(identity.clone(), page, content));
        OverlayReceivers {
            sources: 1,
            preview_tabs: 0,
        }
    }

    async fn deliver_reload(&self, _: &OverlayId) {}

    async fn revoke(&self, _: &OverlayId) {}
}

struct FixedDonor;

impl LatestValueReader for FixedDonor {
    fn latest(&self, slot: &str, _: LatestScope<'_>) -> Option<LatestValue> {
        (slot == LATEST_DONATION_SLOT).then(|| {
            LatestValue::new(
                "donatello",
                OffsetDateTime::UNIX_EPOCH,
                BTreeMap::from([(
                    latest_fields::USER_NAME.to_owned(),
                    Variant::String(LIVE_DONOR.to_owned()),
                )]),
            )
        })
    }
}

fn text(value: &str) -> Variant {
    Variant::String(value.to_owned())
}

fn overlay(id: &str, kind_id: &str) -> OverlayDefinition {
    let config = if kind_id == LATEST_KIND {
        OverlayConfig::from([
            ("slot".to_owned(), text(LATEST_DONATION_SLOT)),
            (HEADLINE.to_owned(), text("%user_name%")),
            ("platform".to_owned(), text("")),
        ])
    } else {
        OverlayConfig::new()
    };
    common::overlay_definition(id, id, kind_id, config)
}

struct Harness {
    sink: Arc<RecordingSink>,
    service: OverlayServiceHandle,
    _tasks: forge_net::TaskOwner,
}

fn harness(
    definitions: Vec<OverlayDefinition>,
    retained: HashMap<OverlayId, OverlayConfig>,
) -> Harness {
    let mut repo = MockOverlayRepo::new();
    let listed = definitions.clone();
    repo.expect_list().returning(move || Ok(listed.clone()));
    repo.expect_get()
        .returning(move |id| Ok(definitions.iter().find(|d| &d.id == id).cloned()));
    repo.expect_get_retained_content()
        .returning(move |id| Ok(retained.get(id).cloned()));
    repo.expect_set_retained_content().returning(|_, _| Ok(()));

    let sink = Arc::new(RecordingSink::default());
    let tasks = forge_net::TaskOwner::new();
    let service = OverlayServiceHandle::new(OverlayServiceParts {
        default_root: std::path::PathBuf::new(),
        date_names: forge_runtime::DateNamesHandle::default(),
        tasks: tasks.clone(),
        repo: Arc::new(repo) as Arc<dyn OverlayRepo>,
        settings: Arc::new(MemorySettings::new()) as Arc<dyn SettingsRepo>,
        kinds: common::builtin_overlay_kinds(),
        bus: EventBus::new(Arc::new(NullEventLogRepo)),
        frames: Arc::clone(&sink) as Arc<dyn OverlayFrameSink>,
        media: None,
        speaker: None,
        latest: Some(Arc::new(FixedDonor) as Arc<dyn LatestValueReader>),
    });
    Harness {
        sink,
        service,
        _tasks: tasks,
    }
}

async fn bounded<F: std::future::Future>(work: F) -> F::Output {
    tokio::time::timeout(WAIT_BOUND, work)
        .await
        .expect("the overlay service finished within the bound")
}

fn only_page_delivery(delivered: &[Delivered]) -> (OverlayId, OverlayPage, Value) {
    match delivered {
        [Delivered::OnePage(identity, page, content)] => (identity.clone(), *page, content.clone()),
        other => panic!("expected one delivery addressed to one page, got {other:?}"),
    }
}

#[tokio::test]
async fn a_connecting_page_is_handed_the_retained_content_addressed_to_it_alone() {
    let goal = OverlayId::new("goal-box");
    let harness = harness(
        vec![overlay("goal-box", GOAL_KIND)],
        HashMap::from([(
            goal.clone(),
            OverlayConfig::from([(VALUE_KEY.to_owned(), text(RETAINED_VALUE))]),
        )]),
    );

    bounded(
        harness
            .service
            .overlay_connected(&goal, OverlayPage::new(7)),
    )
    .await;

    assert_eq!(
        harness.sink.delivered(),
        vec![Delivered::OnePage(
            goal,
            OverlayPage::new(7),
            serde_json::json!({ "value": RETAINED_VALUE }),
        )],
        "the replay for one reconnecting page re-animated every page of the overlay"
    );
}

#[tokio::test]
async fn a_connecting_card_page_is_handed_its_opening_content_addressed_to_it_alone() {
    let card = OverlayId::new("pinned-card");
    let harness = harness(vec![overlay("pinned-card", CARD_KIND)], HashMap::new());

    bounded(
        harness
            .service
            .overlay_connected(&card, OverlayPage::new(7)),
    )
    .await;

    let (identity, page, content) = only_page_delivery(&harness.sink.delivered());
    assert_eq!(
        (identity, page, content.is_object()),
        (card, OverlayPage::new(7), true)
    );
}

#[tokio::test]
async fn a_connecting_latest_page_is_handed_the_live_value_addressed_to_it_alone() {
    let latest = OverlayId::new("last-donation");
    let harness = harness(vec![overlay("last-donation", LATEST_KIND)], HashMap::new());

    bounded(
        harness
            .service
            .overlay_connected(&latest, OverlayPage::new(7)),
    )
    .await;

    let (identity, page, content) = only_page_delivery(&harness.sink.delivered());
    assert_eq!(
        (identity, page, content[HEADLINE].as_str()),
        (latest, OverlayPage::new(7), Some(LIVE_DONOR))
    );
}

#[tokio::test(start_paused = true)]
async fn the_live_value_returning_after_a_preview_reaches_every_page_like_the_preview_did() {
    let latest = OverlayId::new("last-donation");
    let harness = harness(vec![overlay("last-donation", LATEST_KIND)], HashMap::new());

    bounded(harness.service.test_fire(&latest, &common::NeutralSamples))
        .await
        .expect("a latest overlay can be test fired");
    tokio::time::sleep(PREVIEW_HOLD + Duration::from_millis(1)).await;
    bounded(async {
        while harness.sink.delivered().len() < 2 {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await;

    let headlines: Vec<_> = harness
        .sink
        .delivered()
        .into_iter()
        .map(|delivered| match delivered {
            Delivered::EveryPage(identity, content) => (
                identity,
                content[HEADLINE].as_str().unwrap_or_default().to_owned(),
            ),
            other @ Delivered::OnePage(..) => {
                panic!("a preview restore was addressed to one page: {other:?}")
            }
        })
        .collect();
    assert_eq!(
        headlines,
        [
            (latest.clone(), SAMPLE_DONOR.to_owned()),
            (latest, LIVE_DONOR.to_owned())
        ]
    );
}

struct PageListener {
    name: &'static str,
    heard: Arc<Mutex<Vec<(&'static str, OverlayPage)>>>,
}

#[async_trait]
impl OverlayConnectListener for PageListener {
    async fn overlay_connected(&self, _: &OverlayId, page: OverlayPage) {
        self.heard.lock().unwrap().push((self.name, page));
    }
}

#[tokio::test]
async fn a_connect_fanout_hands_every_listener_the_page_that_connected() {
    let heard = Arc::new(Mutex::new(Vec::new()));
    let listener = |name| {
        Arc::new(PageListener {
            name,
            heard: Arc::clone(&heard),
        }) as Arc<dyn OverlayConnectListener>
    };
    let fanout = OverlayConnectFanout::new(vec![listener("replay"), listener("catch-up")]);

    fanout
        .overlay_connected(&OverlayId::new("donation-alert"), OverlayPage::new(9))
        .await;

    assert_eq!(
        heard.lock().unwrap().clone(),
        vec![
            ("replay", OverlayPage::new(9)),
            ("catch-up", OverlayPage::new(9))
        ]
    );
}
