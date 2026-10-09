#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use forge_events::{Event, EventSource, LatestChanged};
use forge_overlay::{OverlayKindRegistry, register_builtin_kinds};
use forge_runtime::{
    Config, EventBus, NullEventLogRepo, OverlayConnectListener, OverlayFrameSink, OverlayReceivers,
    OverlayServiceError, OverlayServiceHandle, spawn_latest_overlay_feed,
};
use forge_storage::settings::MockSettingsRepo;
use forge_storage::{
    MockOverlayRepo, OverlayConfig, OverlayCredential, OverlayDefinition, OverlayId, OverlayRepo,
    SettingsRepo,
};
use forge_types::{
    ArgStack, LATEST_DONATION_SLOT, LatestScope, LatestValue, LatestValueReader, NOW_PLAYING_SLOT,
    Variant, latest_fields,
};
use serde_json::Value;
use time::OffsetDateTime;

const LATEST_KIND: &str = "overlay.latest";
const ALERT_KIND: &str = "overlay.alert";
const HEADLINE: &str = "headline";
const PLACEHOLDER: &str = "placeholder";
const LIVE_DONOR: &str = "Olena";
const LIVE_TRACK: &str = "Night Bus";
const STALE_RETAINED: &str = "stale retained";
const SAMPLE_DONOR: &str = "PixelPal";
const PREVIEW_HOLD: Duration = Duration::from_secs(5);
const WAIT_BOUND: Duration = Duration::from_secs(30);
const HOLD_SLACK: Duration = Duration::from_millis(10);

async fn bounded<F: std::future::Future>(work: F) -> F::Output {
    tokio::time::timeout(WAIT_BOUND, work)
        .await
        .expect("the overlay service finished within the bound")
}

#[derive(Default)]
struct RecordingSink {
    frames: Mutex<Vec<(OverlayId, Value)>>,
}

impl RecordingSink {
    fn headlines_for(&self, id: &str) -> Vec<String> {
        self.frames
            .lock()
            .unwrap()
            .iter()
            .filter(|(identity, _)| identity.as_str() == id)
            .map(|(_, content)| content[HEADLINE].as_str().unwrap_or_default().to_owned())
            .collect()
    }

    fn identities(&self) -> Vec<String> {
        self.frames
            .lock()
            .unwrap()
            .iter()
            .map(|(identity, _)| identity.as_str().to_owned())
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
        identity: &OverlayId,
        content: Value,
        _: Option<u64>,
    ) -> OverlayReceivers {
        self.frames
            .lock()
            .unwrap()
            .push((identity.clone(), content));
        OverlayReceivers {
            sources: 1,
            preview_tabs: 0,
        }
    }

    async fn deliver_reload(&self, _: &OverlayId) {}

    async fn revoke(&self, _: &OverlayId) {}
}

#[derive(Default)]
struct SlotStore(Mutex<HashMap<String, LatestValue>>);

impl SlotStore {
    fn fill(&self, slot: &str, field: &str, text: &str) {
        let value = LatestValue::new(
            "donatello",
            OffsetDateTime::UNIX_EPOCH,
            BTreeMap::from([(field.to_owned(), Variant::String(text.to_owned()))]),
        );
        self.0.lock().unwrap().insert(slot.to_owned(), value);
    }
}

impl LatestValueReader for SlotStore {
    fn latest(&self, slot: &str, scope: LatestScope<'_>) -> Option<LatestValue> {
        let held = self.0.lock().unwrap().get(slot).cloned()?;
        match scope {
            LatestScope::Platform(platform) if platform != held.platform => None,
            _ => Some(held),
        }
    }
}

struct Overlay {
    id: &'static str,
    kind_id: &'static str,
    slot: Option<&'static str>,
    platform: &'static str,
    enabled: bool,
}

const fn latest(id: &'static str, slot: &'static str) -> Overlay {
    Overlay {
        id,
        kind_id: LATEST_KIND,
        slot: Some(slot),
        platform: "",
        enabled: true,
    }
}

fn text(value: &str) -> Variant {
    Variant::String(value.to_owned())
}

fn definition(overlay: &Overlay) -> OverlayDefinition {
    let mut config = OverlayConfig::new();
    if let Some(slot) = overlay.slot {
        config.insert("slot".to_owned(), text(slot));
        let headline = if slot == NOW_PLAYING_SLOT {
            "%title%"
        } else {
            "%user_name%"
        };
        config.insert(HEADLINE.to_owned(), text(headline));
        config.insert("platform".to_owned(), text(overlay.platform));
    }
    OverlayDefinition {
        id: OverlayId::new(overlay.id),
        display_name: overlay.id.to_owned(),
        kind_id: overlay.kind_id.to_owned(),
        enabled: overlay.enabled,
        position: 0,
        config,
        config_schema_version: 1,
        generator_version: 0,
        source_overrides: Vec::new(),
        credential: OverlayCredential::new("2f8b1d0c9a7e6f5b4c3d2e1f0a9b8c7d"),
        created_at: OffsetDateTime::UNIX_EPOCH,
        updated_at: OffsetDateTime::UNIX_EPOCH,
    }
}

type RetainedStore = Arc<Mutex<HashMap<OverlayId, OverlayConfig>>>;

struct Harness {
    sink: Arc<RecordingSink>,
    slots: Arc<SlotStore>,
    retained: RetainedStore,
    service: OverlayServiceHandle,
}

impl Harness {
    fn new(overlays: &[Overlay], bus: Arc<EventBus>) -> Self {
        let definitions: Vec<OverlayDefinition> = overlays.iter().map(definition).collect();
        let retained: RetainedStore = Arc::default();

        let mut repo = MockOverlayRepo::new();
        let listed = definitions.clone();
        repo.expect_list().returning(move || Ok(listed.clone()));
        repo.expect_get()
            .returning(move |id| Ok(definitions.iter().find(|d| &d.id == id).cloned()));
        let writes = Arc::clone(&retained);
        repo.expect_set_retained_content()
            .returning(move |id, content| {
                writes.lock().unwrap().insert(id.clone(), content.clone());
                Ok(())
            });
        let reads = Arc::clone(&retained);
        repo.expect_get_retained_content()
            .returning(move |id| Ok(reads.lock().unwrap().get(id).cloned()));

        let mut kinds = OverlayKindRegistry::new();
        register_builtin_kinds(&mut kinds).expect("the builtin overlay kinds register");

        let sink = Arc::new(RecordingSink::default());
        let slots = Arc::new(SlotStore::default());
        let service = OverlayServiceHandle::new(
            Arc::new(repo) as Arc<dyn OverlayRepo>,
            Arc::new(MockSettingsRepo::new()) as Arc<dyn SettingsRepo>,
            Arc::new(kinds),
            bus,
            Some(Arc::clone(&sink) as Arc<dyn OverlayFrameSink>),
        )
        .with_latest_values(Arc::clone(&slots) as Arc<dyn LatestValueReader>);

        Self {
            sink,
            slots,
            retained,
            service,
        }
    }

    fn standalone(overlays: &[Overlay]) -> Self {
        Self::new(overlays, EventBus::new(Arc::new(NullEventLogRepo)))
    }

    fn retain(&self, id: &str, headline: &str) {
        self.retained.lock().unwrap().insert(
            OverlayId::new(id),
            OverlayConfig::from([(HEADLINE.to_owned(), text(headline))]),
        );
    }

    async fn wait_for_frames(&self, at_least: usize, bound: Duration) {
        tokio::time::timeout(bound, async {
            while self.sink.count() < at_least {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        })
        .await
        .expect("the expected frames reached the page");
    }
}

#[tokio::test]
async fn a_connecting_latest_page_gets_the_live_slot_value_not_the_retained_content() {
    let harness = Harness::standalone(&[latest("last-donation", LATEST_DONATION_SLOT)]);
    harness.retain("last-donation", STALE_RETAINED);
    harness
        .slots
        .fill(LATEST_DONATION_SLOT, latest_fields::USER_NAME, LIVE_DONOR);

    bounded(
        harness
            .service
            .overlay_connected(&OverlayId::new("last-donation")),
    )
    .await;

    assert_eq!(harness.sink.headlines_for("last-donation"), [LIVE_DONOR]);
}

#[tokio::test]
async fn a_connecting_latest_page_with_an_empty_slot_gets_the_placeholder() {
    let harness = Harness::standalone(&[latest("last-donation", LATEST_DONATION_SLOT)]);
    harness.retain("last-donation", STALE_RETAINED);

    bounded(
        harness
            .service
            .overlay_connected(&OverlayId::new("last-donation")),
    )
    .await;

    let frames = harness.sink.frames.lock().unwrap();
    assert_eq!(frames.len(), 1, "one frame per connect");
    assert!(
        frames[0].1[PLACEHOLDER].is_string() && frames[0].1.get(HEADLINE).is_none(),
        "an empty slot reached the page as {}",
        frames[0].1
    );
}

#[tokio::test]
async fn a_platform_scoped_latest_page_ignores_a_value_from_another_platform() {
    let harness = Harness::standalone(&[Overlay {
        platform: "monobank",
        ..latest("last-donation", LATEST_DONATION_SLOT)
    }]);
    harness
        .slots
        .fill(LATEST_DONATION_SLOT, latest_fields::USER_NAME, LIVE_DONOR);

    bounded(
        harness
            .service
            .overlay_connected(&OverlayId::new("last-donation")),
    )
    .await;

    let frames = harness.sink.frames.lock().unwrap();
    assert!(
        frames.len() == 1 && frames[0].1[PLACEHOLDER].is_string(),
        "a donatello value reached a monobank-scoped page: {frames:?}"
    );
}

#[tokio::test]
async fn an_action_sending_content_to_a_latest_overlay_is_refused_and_nothing_reaches_the_page() {
    let harness = Harness::standalone(&[latest("last-donation", LATEST_DONATION_SLOT)]);
    let supplied = OverlayConfig::from([(HEADLINE.to_owned(), text("from an action"))]);

    let refused = bounded(harness.service.send_to(
        &OverlayId::new("last-donation"),
        &supplied,
        &ArgStack::new(),
        None,
        None,
    ))
    .await;

    assert!(
        matches!(&refused, Err(OverlayServiceError::SlotBound(id)) if id.as_str() == "last-donation"),
        "got {refused:?}"
    );
    assert_eq!(harness.sink.count(), 0);
}

#[tokio::test]
async fn a_test_fire_on_a_latest_overlay_shows_the_sample_and_never_stores_it_as_retained() {
    let harness = Harness::standalone(&[latest("last-donation", LATEST_DONATION_SLOT)]);
    harness
        .slots
        .fill(LATEST_DONATION_SLOT, latest_fields::USER_NAME, LIVE_DONOR);

    bounded(harness.service.test_fire(&OverlayId::new("last-donation")))
        .await
        .expect("a latest overlay can be test fired");

    assert_eq!(harness.sink.headlines_for("last-donation"), [SAMPLE_DONOR]);
    assert!(
        harness.retained.lock().unwrap().is_empty(),
        "the sample was kept as the overlay's retained content"
    );
}

#[tokio::test(start_paused = true)]
async fn a_test_fire_on_a_latest_overlay_returns_to_the_live_value_once_the_preview_hold_ends() {
    let harness = Harness::standalone(&[latest("last-donation", LATEST_DONATION_SLOT)]);
    harness
        .slots
        .fill(LATEST_DONATION_SLOT, latest_fields::USER_NAME, LIVE_DONOR);

    bounded(harness.service.test_fire(&OverlayId::new("last-donation")))
        .await
        .expect("a latest overlay can be test fired");
    tokio::time::sleep(PREVIEW_HOLD - Duration::from_millis(1)).await;
    let during_hold = harness.sink.headlines_for("last-donation");
    tokio::time::sleep(Duration::from_millis(2)).await;
    harness.wait_for_frames(2, HOLD_SLACK).await;

    assert_eq!(during_hold, [SAMPLE_DONOR], "the preview was cut short");
    assert_eq!(
        harness.sink.headlines_for("last-donation"),
        [SAMPLE_DONOR, LIVE_DONOR]
    );
}

#[tokio::test]
async fn refreshing_one_slot_pushes_only_enabled_latest_overlays_bound_to_it() {
    let harness = Harness::standalone(&[
        latest("now-playing", NOW_PLAYING_SLOT),
        Overlay {
            enabled: false,
            ..latest("disabled-donation", LATEST_DONATION_SLOT)
        },
        Overlay {
            id: "alert-box",
            kind_id: ALERT_KIND,
            slot: None,
            platform: "",
            enabled: true,
        },
        latest("last-donation", LATEST_DONATION_SLOT),
    ]);
    harness
        .slots
        .fill(LATEST_DONATION_SLOT, latest_fields::USER_NAME, LIVE_DONOR);

    bounded(harness.service.refresh_latest(Some(LATEST_DONATION_SLOT))).await;

    assert_eq!(harness.sink.identities(), ["last-donation"]);
}

#[tokio::test]
async fn a_latest_changed_event_refreshes_the_overlays_bound_to_that_slot() {
    let bus = EventBus::new(Arc::new(NullEventLogRepo));
    let harness = Harness::new(
        &[
            latest("now-playing", NOW_PLAYING_SLOT),
            latest("last-donation", LATEST_DONATION_SLOT),
        ],
        Arc::clone(&bus),
    );
    harness
        .slots
        .fill(LATEST_DONATION_SLOT, latest_fields::USER_NAME, LIVE_DONOR);
    spawn_latest_overlay_feed(&bus, harness.service.clone());

    bus.publish(
        LatestChanged {
            slot: LATEST_DONATION_SLOT.to_owned(),
            platform: Some("donatello".to_owned()),
            merged_changed: true,
            cleared: false,
        }
        .into_event(None)
        .unwrap(),
    );
    harness.wait_for_frames(1, WAIT_BOUND).await;

    assert_eq!(harness.sink.identities(), ["last-donation"]);
    assert_eq!(harness.sink.headlines_for("last-donation"), [LIVE_DONOR]);
}

#[tokio::test]
async fn a_feed_that_fell_behind_the_bus_refreshes_every_slot() {
    let bus = EventBus::with_config(
        Arc::new(NullEventLogRepo),
        &Config {
            bus_observer_capacity: 1,
            ..Config::default()
        },
    );
    let harness = Harness::new(
        &[
            latest("now-playing", NOW_PLAYING_SLOT),
            latest("last-donation", LATEST_DONATION_SLOT),
        ],
        Arc::clone(&bus),
    );
    harness
        .slots
        .fill(LATEST_DONATION_SLOT, latest_fields::USER_NAME, LIVE_DONOR);
    harness
        .slots
        .fill(NOW_PLAYING_SLOT, latest_fields::TITLE, LIVE_TRACK);
    spawn_latest_overlay_feed(&bus, harness.service.clone());

    for _ in 0..3 {
        bus.publish(Event::new(
            EventSource::Core,
            "timer.tick",
            serde_json::Value::Null,
        ));
    }
    harness.wait_for_frames(2, WAIT_BOUND).await;

    assert_eq!(
        (
            harness.sink.headlines_for("now-playing"),
            harness.sink.headlines_for("last-donation")
        ),
        (vec![LIVE_TRACK.to_owned()], vec![LIVE_DONOR.to_owned()])
    );
}
