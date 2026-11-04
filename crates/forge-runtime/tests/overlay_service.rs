#![expect(clippy::expect_used, clippy::unwrap_used, reason = "test fixtures")]
mod common;
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use forge_overlay::{
    AudioAnnouncement, CONFIG_FILE, GENERATOR_VERSION, MARKUP_FILE, OverlayKindRegistry,
    RESERVED_DIRECTORY, RowAvatar, RowRemoval, RowRemovalScope, STYLE_FILE, announcement_content,
    register_builtin_kinds,
};
use forge_runtime::OVERLAY_TEST_FIRE_KIND;
use forge_runtime::{
    EventBus, MaterializePass, NullEventLogRepo, OverlayConnectFanout, OverlayConnectListener,
    OverlayDefinitionChanges, OverlayDelivery, OverlayDispatch, OverlayFrameSink, OverlayPage,
    OverlayReceivers, OverlayServiceHandle, OverlayServiceParts, RuntimeError,
};

use forge_storage::settings::core_settings;
use forge_storage::test_support::MemorySettings;
use forge_storage::{
    MockOverlayRepo, OverlayConfig, OverlayDefinition, OverlayId, OverlayRepo, SettingsRepo,
    StorageError,
};
use forge_types::OverlayCredential;
use forge_types::{ArgStack, Variant};
use futures_util::FutureExt;
use tempfile::TempDir;
use time::OffsetDateTime;

const ALERT_KIND: &str = "overlay.alert";
const BLANK_KIND: &str = "overlay.blank";
const CHAT_KIND: &str = "overlay.chat";
const GOAL_KIND: &str = "overlay.goal";
const UNSHIPPED_KIND: &str = "overlay.vendor_unshipped";

const CLIP_ID: &str = "5f2a";
const CLIP_PATH: &str = "/audio/v1/clip/n3Zq";
const REPORT_PATH: &str = "/audio/v1/report/n3Zq";
const CLIP_MEDIA_TYPE: &str = "audio/wav";
const CLIP_DURATION_MS: u64 = 2_500;

const LABEL_KEY: &str = "label";
const VALUE_KEY: &str = "value";
const TARGET_KEY: &str = "target";
const TEXT_COLOR_KEY: &str = "text_color";
const HEADLINE_KEY: &str = "headline";
const SUBLINE_KEY: &str = "subline";
const POSITION_KEY: &str = "position";

const TAG_KEY: &str = "type";
const TAGGED_VALUE_KEY: &str = "value";

const SAMPLE_HEADLINE: &str = "Thanks for the raid";
const SAMPLE_SUBLINE: &str = "200 viewers arrived";
const SAMPLE_UNIX_SECONDS: i64 = 1_700_000_000;
const SAMPLE_RFC3339: &str = "2023-11-14T22:13:20Z";

#[derive(Debug, Clone, PartialEq)]
struct ContentFrame {
    identity: OverlayId,
    content: serde_json::Value,
    duration_ms: Option<u64>,
}

#[derive(Default)]
struct RecordingSink {
    frames: Mutex<Vec<ContentFrame>>,
    reloads: Mutex<Vec<OverlayId>>,
    revoked: Mutex<Vec<OverlayId>>,
    removals: Mutex<Vec<(OverlayId, RowRemoval)>>,
    avatars: Mutex<Vec<(OverlayId, RowAvatar)>>,
}

impl RecordingSink {
    fn avatars(&self) -> Vec<(OverlayId, RowAvatar)> {
        self.avatars.lock().unwrap().clone()
    }

    fn removals(&self) -> Vec<(OverlayId, RowRemoval)> {
        self.removals.lock().unwrap().clone()
    }

    fn frames(&self) -> Vec<ContentFrame> {
        self.frames.lock().unwrap().clone()
    }

    fn reloads(&self) -> Vec<OverlayId> {
        self.reloads.lock().unwrap().clone()
    }

    fn revoked(&self) -> Vec<OverlayId> {
        self.revoked.lock().unwrap().clone()
    }
}

#[async_trait]
impl OverlayFrameSink for RecordingSink {
    async fn deliver_content(
        &self,
        identity: &OverlayId,
        content: serde_json::Value,
        duration_ms: Option<u64>,
    ) -> OverlayReceivers {
        self.frames.lock().unwrap().push(ContentFrame {
            identity: identity.clone(),
            content,
            duration_ms,
        });
        OverlayReceivers {
            sources: 1,
            preview_tabs: 0,
        }
    }

    async fn deliver_content_to_page(
        &self,
        identity: &OverlayId,
        _: OverlayPage,
        content: serde_json::Value,
    ) -> OverlayReceivers {
        self.deliver_content(identity, content, None).await
    }

    async fn deliver_reload(&self, identity: &OverlayId) {
        self.reloads.lock().unwrap().push(identity.clone());
    }

    async fn revoke(&self, identity: &OverlayId) {
        self.revoked.lock().unwrap().push(identity.clone());
    }

    async fn deliver_removal(&self, identity: &OverlayId, removal: &RowRemoval) {
        self.removals
            .lock()
            .unwrap()
            .push((identity.clone(), removal.clone()));
    }

    async fn deliver_avatar(&self, identity: &OverlayId, avatar: &RowAvatar) {
        self.avatars
            .lock()
            .unwrap()
            .push((identity.clone(), avatar.clone()));
    }
}

fn tagged_variant_nodes(value: &serde_json::Value) -> Vec<serde_json::Value> {
    let mut found = Vec::new();
    match value {
        serde_json::Value::Object(map) => {
            if map.len() == 2 && map.contains_key(TAG_KEY) && map.contains_key(TAGGED_VALUE_KEY) {
                found.push(value.clone());
            }
            for nested in map.values() {
                found.extend(tagged_variant_nodes(nested));
            }
        }
        serde_json::Value::Array(items) => {
            for nested in items {
                found.extend(tagged_variant_nodes(nested));
            }
        }
        _ => {}
    }
    found
}

fn definition(id: &str) -> OverlayDefinition {
    common::overlay_definition(id, "Sub alert", ALERT_KIND, OverlayConfig::new())
}

type RetainedStore = Arc<Mutex<Vec<(OverlayId, OverlayConfig)>>>;

struct Harness {
    _home: TempDir,
    root: PathBuf,
    bus: Arc<EventBus>,
    sink: Arc<RecordingSink>,
    saved: Arc<Mutex<Vec<OverlayDefinition>>>,
    retained: RetainedStore,
    service: OverlayServiceHandle,
}

impl Harness {
    fn saved(&self) -> Vec<OverlayDefinition> {
        self.saved.lock().unwrap().clone()
    }

    fn retained_for(&self, id: &OverlayId) -> Option<OverlayConfig> {
        self.retained
            .lock()
            .unwrap()
            .iter()
            .rev()
            .find(|(stored, _)| stored == id)
            .map(|(_, content)| content.clone())
    }

    fn directory_of(&self, id: &OverlayId) -> PathBuf {
        self.root.canonicalize().unwrap().join(id.as_str())
    }
}

fn registry() -> Arc<OverlayKindRegistry> {
    let mut reg = OverlayKindRegistry::new();
    register_builtin_kinds(&mut reg).expect("the builtin overlay kinds register");
    Arc::new(reg)
}

fn harness(definitions: Vec<OverlayDefinition>, attach_sink: bool) -> Harness {
    let home = TempDir::new().unwrap();
    let root = home.path().join("overlays");
    let saved: Arc<Mutex<Vec<OverlayDefinition>>> = Arc::new(Mutex::new(Vec::new()));

    let mut repo = MockOverlayRepo::new();
    let listed = definitions.clone();
    repo.expect_list().returning(move || Ok(listed.clone()));
    let stored = definitions;
    repo.expect_get()
        .returning(move |id| Ok(stored.iter().find(|d| &d.id == id).cloned()));
    let captured = Arc::clone(&saved);
    repo.expect_save().returning(move |definition| {
        captured.lock().unwrap().push(definition.clone());
        Ok(())
    });

    repo.expect_set_enabled().returning(|_, _| Ok(true));

    let retained: RetainedStore = Arc::new(Mutex::new(Vec::new()));
    let writes = Arc::clone(&retained);
    repo.expect_set_retained_content()
        .returning(move |id, content| {
            writes.lock().unwrap().push((id.clone(), content.clone()));
            Ok(())
        });
    let reads = Arc::clone(&retained);
    repo.expect_get_retained_content().returning(move |id| {
        Ok(reads
            .lock()
            .unwrap()
            .iter()
            .rev()
            .find(|(stored, _)| stored == id)
            .map(|(_, content)| content.clone()))
    });

    let settings = MemorySettings::new();
    settings
        .seed(
            &core_settings::SERVER_OVERLAY_ROOT,
            &Some(root.to_string_lossy().into_owned()),
        )
        .expect("overlay root setting");

    let bus = EventBus::new(Arc::new(NullEventLogRepo));
    let sink = Arc::new(RecordingSink::default());
    let frames: Arc<dyn OverlayFrameSink> = if attach_sink {
        Arc::clone(&sink) as Arc<dyn OverlayFrameSink>
    } else {
        common::no_pages()
    };

    let service = OverlayServiceHandle::new(OverlayServiceParts {
        default_root: std::path::PathBuf::new(),
        date_names: forge_runtime::DateNamesHandle::default(),
        tasks: forge_net::TaskOwner::new(),
        repo: Arc::new(repo) as Arc<dyn OverlayRepo>,
        settings: Arc::new(settings) as Arc<dyn SettingsRepo>,
        kinds: registry(),
        bus: Arc::clone(&bus),
        frames,
        media: None,
        speaker: None,
        latest: None,
    });

    Harness {
        _home: home,
        root,
        bus,
        sink,
        saved,
        retained,
        service,
    }
}

fn current_schema(kind_id: &str) -> u32 {
    registry()
        .get(kind_id)
        .expect("a builtin overlay kind")
        .config_schema_version()
}

fn definition_of_kind(id: &str, kind_id: &str) -> OverlayDefinition {
    let mut stored = definition(id);
    stored.kind_id = kind_id.to_owned();
    stored
}

fn text_config(pairs: &[(&str, &str)]) -> OverlayConfig {
    pairs
        .iter()
        .map(|(key, value)| ((*key).to_owned(), Variant::String((*value).to_owned())))
        .collect()
}

#[derive(Debug, Clone, Copy)]
enum Regeneration {
    BootPass,
    SingleOverlay,
}

#[tokio::test]
async fn a_user_owned_source_file_survives_every_regeneration_trigger() {
    for trigger in [Regeneration::BootPass, Regeneration::SingleOverlay] {
        let mut claimed = definition("sub-alert");
        claimed.source_overrides = vec![STYLE_FILE.to_owned()];
        let harness = harness(vec![claimed.clone()], false);

        harness
            .service
            .materialize_all(&common::NeutralSamples)
            .await
            .expect("the first pass builds the page");
        let directory = harness.directory_of(&claimed.id);
        let owned = directory.join(STYLE_FILE);
        let user_body = "#stage { opacity: 0.5; }\n";
        fs::write(&owned, user_body).unwrap();
        fs::write(directory.join(MARKUP_FILE), "stale markup").unwrap();

        match trigger {
            Regeneration::BootPass => {
                harness
                    .service
                    .materialize_all(&common::NeutralSamples)
                    .await
                    .expect("boot pass");
            }
            Regeneration::SingleOverlay => {
                harness
                    .service
                    .materialize(&claimed.id, &common::NeutralSamples)
                    .await
                    .expect("single overlay pass");
            }
        }

        assert_ne!(
            fs::read_to_string(directory.join(MARKUP_FILE)).unwrap(),
            "stale markup",
            "{trigger:?} regenerated nothing, so it proves nothing about overrides"
        );
        assert_eq!(
            fs::read_to_string(&owned).unwrap(),
            user_body,
            "{trigger:?} rewrote a source file the user owns"
        );
    }
}

#[tokio::test]
async fn test_fire_records_its_origin_without_broadcasting_anything() {
    let stored = definition("sub-alert");
    let harness = harness(vec![stored.clone()], true);
    let mut subscription = harness.bus.subscribe();

    harness
        .service
        .test_fire(&stored.id, &common::NeutralSamples)
        .await
        .expect("a bound overlay samples its event");

    let leaked = subscription.try_recv().expect("the bus stayed open");
    assert!(
        leaked.is_none(),
        "a sample reached the bus as {:?} and can drive actions, scripts and queues",
        leaked.map(|event| event.kind)
    );
    assert!(
        harness
            .bus
            .recent(8)
            .iter()
            .any(|event| event.kind == OVERLAY_TEST_FIRE_KIND),
        "the test fire left no trace in the ring, so run history cannot show it"
    );
}

#[test]
fn a_tally_of_receivers_reports_browser_sources_first_and_preview_tabs_only_alone() {
    for (reached, expected) in [
        (
            OverlayReceivers {
                sources: 0,
                preview_tabs: 0,
            },
            OverlayDelivery::NoPage,
        ),
        (
            OverlayReceivers {
                sources: 0,
                preview_tabs: 2,
            },
            OverlayDelivery::OnlyPreview { tabs: 2 },
        ),
        (
            OverlayReceivers {
                sources: 3,
                preview_tabs: 0,
            },
            OverlayDelivery::Delivered { sources: 3 },
        ),
        (
            OverlayReceivers {
                sources: 3,
                preview_tabs: 2,
            },
            OverlayDelivery::Delivered { sources: 3 },
        ),
    ] {
        assert_eq!(
            reached.outcome(),
            expected,
            "{reached:?} was worded to the user as something other than {expected:?}"
        );
    }
}

#[tokio::test]
async fn test_fire_delivers_one_frame_carrying_the_content_it_returns() {
    let mut stored = definition("sub-alert");
    stored.config = text_config(&[
        (HEADLINE_KEY, SAMPLE_HEADLINE),
        (SUBLINE_KEY, SAMPLE_SUBLINE),
    ]);
    let harness = harness(vec![stored.clone()], true);

    let fired = harness
        .service
        .test_fire(&stored.id, &common::NeutralSamples)
        .await
        .expect("fire");

    let frames = harness.sink.frames();
    assert_eq!(frames.len(), 1, "a single test fire delivered {frames:?}");
    assert_eq!(
        frames[0].identity, stored.id,
        "a test fire reached an overlay it does not target"
    );
    assert_eq!(
        frames[0].content,
        serde_json::json!({
            HEADLINE_KEY: SAMPLE_HEADLINE,
            SUBLINE_KEY: SAMPLE_SUBLINE,
        }),
        "the page received something other than the previewed content as plain values"
    );
    assert_eq!(
        fired.content,
        text_config(&[
            (HEADLINE_KEY, SAMPLE_HEADLINE),
            (SUBLINE_KEY, SAMPLE_SUBLINE),
        ]),
        "the caller previews content the page never received"
    );
    assert_eq!(fired.delivery, OverlayDelivery::Delivered { sources: 1 });
}

#[tokio::test]
async fn test_fire_without_a_serving_sink_still_returns_the_sample_and_says_it_landed_nowhere() {
    let mut stored = definition("sub-alert");
    stored.config = text_config(&[
        (HEADLINE_KEY, SAMPLE_HEADLINE),
        (SUBLINE_KEY, SAMPLE_SUBLINE),
    ]);
    let harness = harness(vec![stored.clone()], false);

    let fired = harness
        .service
        .test_fire(&stored.id, &common::NeutralSamples)
        .await
        .expect("fire");

    assert_eq!(
        fired.delivery,
        OverlayDelivery::NoPage,
        "nothing is serving, so the caller must be told the preview ran alone"
    );
    assert_eq!(
        fired.content,
        text_config(&[
            (HEADLINE_KEY, SAMPLE_HEADLINE),
            (SUBLINE_KEY, SAMPLE_SUBLINE),
        ]),
        "the caller still needs the very content a connected page would have received"
    );
}

#[tokio::test]
async fn test_fire_refuses_and_delivers_nothing_when_no_sample_can_be_built() {
    let mut unshipped = definition("vendor-box");
    unshipped.kind_id = UNSHIPPED_KIND.to_owned();
    let harness = harness(vec![unshipped.clone()], true);
    let mut subscription = harness.bus.subscribe();

    type Check = fn(&RuntimeError) -> bool;
    for (id, matches_expected, label) in [
        (
            OverlayId::new("nobody"),
            (|e: &RuntimeError| matches!(e, RuntimeError::UnknownOverlay(_))) as Check,
            "an identity no record carries",
        ),
        (
            unshipped.id.clone(),
            (|e: &RuntimeError| matches!(e, RuntimeError::UnavailableOverlayKind { .. })) as Check,
            "an overlay type this build lacks",
        ),
    ] {
        let err = harness
            .service
            .test_fire(&id, &common::NeutralSamples)
            .await
            .expect_err("a sample that cannot be built must be refused");

        assert!(matches_expected(&err), "{label} produced {err:?}");
    }

    assert!(
        harness.sink.frames().is_empty(),
        "a refused test fire still pushed a frame to connected pages"
    );
    assert!(
        subscription.try_recv().expect("open").is_none(),
        "a refused test fire published to the bus"
    );
}

#[tokio::test]
async fn a_pass_keeps_the_record_of_an_overlay_type_this_build_lacks_and_carries_on() {
    let mut unshipped = definition("vendor-box");
    unshipped.kind_id = UNSHIPPED_KIND.to_owned();
    let known = definition("sub-alert");
    let harness = harness(vec![unshipped.clone(), known.clone()], false);

    let pass = harness
        .service
        .materialize_all(&common::NeutralSamples)
        .await
        .expect("pass");

    assert_eq!(
        pass,
        MaterializePass {
            materialized: 1,
            unavailable: 1,
            failed: 0,
        }
    );
    assert!(
        !harness.root.join(unshipped.id.as_str()).exists(),
        "an overlay type this build lacks had a directory built for it anyway"
    );
    assert!(
        harness.directory_of(&known.id).join(CONFIG_FILE).exists(),
        "the pass stopped at the unavailable record instead of carrying on"
    );
    assert!(
        harness.saved().iter().all(|d| d.id != unshipped.id),
        "an unavailable record was rewritten and lost its stored generator version"
    );
}

#[tokio::test]
async fn a_pass_stamps_only_the_records_whose_generator_version_is_stale() {
    let stale = definition("stale-box");
    let mut current = definition("current-box");
    current.generator_version = GENERATOR_VERSION;
    current.config_schema_version = current_schema(ALERT_KIND);
    let harness = harness(vec![stale.clone(), current.clone()], false);

    harness
        .service
        .materialize_all(&common::NeutralSamples)
        .await
        .expect("pass");

    let saved = harness.saved();
    assert_eq!(
        saved.iter().map(|d| d.id.clone()).collect::<Vec<_>>(),
        vec![stale.id.clone()],
        "an up to date record was written back for no reason"
    );
    assert_eq!(saved[0].generator_version, GENERATOR_VERSION);
}

#[tokio::test]
async fn one_failing_overlay_does_not_abort_the_rest_of_the_pass() {
    let mut refused = definition("sub-alert");
    refused.id = OverlayId::new("Not-A-Directory-Name");
    let good = definition("sub-alert");
    let harness = harness(vec![refused, good.clone()], false);

    let pass = harness
        .service
        .materialize_all(&common::NeutralSamples)
        .await
        .expect("pass");

    assert_eq!(
        pass,
        MaterializePass {
            materialized: 1,
            unavailable: 0,
            failed: 1,
        }
    );
    assert!(
        harness.directory_of(&good.id).join(CONFIG_FILE).exists(),
        "a failure on one overlay left every later overlay unbuilt"
    );
}

#[tokio::test]
async fn removing_an_overlay_folder_is_idempotent_and_spares_the_shared_subtree() {
    let stored = definition("sub-alert");
    let harness = harness(vec![stored.clone()], false);
    harness
        .service
        .materialize_all(&common::NeutralSamples)
        .await
        .expect("pass");
    let directory = harness.directory_of(&stored.id);

    assert!(
        harness.service.remove_folder(&stored.id).await.expect("ok"),
        "the first removal must report that a directory was there"
    );
    assert!(!directory.exists());
    assert!(
        !harness.service.remove_folder(&stored.id).await.expect("ok"),
        "a repeated removal must report that nothing was there"
    );
    assert!(
        harness
            .root
            .canonicalize()
            .unwrap()
            .join(RESERVED_DIRECTORY)
            .exists(),
        "removing one overlay took the shared runtime subtree with it"
    );
}

#[tokio::test]
async fn a_reload_control_frame_names_only_the_overlay_it_targets() {
    let harness = harness(Vec::new(), true);
    let mut subscription = harness.bus.subscribe();
    let target = OverlayId::new("sub-alert");

    harness.service.reload_page(&target).await;

    assert_eq!(
        harness.sink.reloads(),
        vec![target],
        "a reload must name the identity it targets so other pages are untouched"
    );
    assert!(
        subscription
            .try_recv()
            .expect("the bus stayed open")
            .is_none(),
        "a reload reached the bus and can drive actions, scripts and queues"
    );
}

#[tokio::test]
async fn materializing_one_overlay_reloads_the_page_it_rebuilt() {
    let stored = definition("sub-alert");
    let harness = harness(vec![stored.clone()], true);

    harness
        .service
        .materialize(&stored.id, &common::NeutralSamples)
        .await
        .expect("materialize");

    assert_eq!(
        harness.sink.reloads(),
        vec![stored.id],
        "a rebuilt page was never told to reload"
    );
}

#[tokio::test]
async fn delivered_content_is_kept_for_replay_only_by_a_kind_whose_delivery_is_the_display_itself()
{
    for (kind_id, label) in [
        (GOAL_KIND, "a kind that replaces what the page shows"),
        (
            ALERT_KIND,
            "a kind whose delivery is gone once it has been shown",
        ),
        (CHAT_KIND, "a kind that appends a row to a running list"),
    ] {
        let stored = definition_of_kind("box", kind_id);
        let harness = harness(vec![stored.clone()], true);
        let content = text_config(&[(LABEL_KEY, "Sub goal"), (VALUE_KEY, "42")]);

        harness
            .service
            .send_to(
                &stored.id,
                &content,
                &ArgStack::new(),
                forge_runtime::OverlaySendOptions::default(),
            )
            .await
            .expect("a bound overlay accepts content");

        assert_eq!(
            harness.retained_for(&stored.id).is_some(),
            kind_id == GOAL_KIND,
            "{label} retained the wrong thing, so the next connection replays the wrong thing"
        );
    }
}

#[tokio::test]
async fn every_variant_kind_reaches_the_page_as_a_plain_json_value() {
    let stored = definition_of_kind("goal-box", GOAL_KIND);
    let harness = harness(vec![stored.clone()], true);
    let moment = OffsetDateTime::from_unix_timestamp(SAMPLE_UNIX_SECONDS).unwrap();

    for (held, expected) in [
        (Variant::Int(42), serde_json::json!(42)),
        (Variant::Float(0.5), serde_json::json!(0.5)),
        (Variant::Bool(true), serde_json::json!(true)),
        (
            Variant::String("Sub goal".to_owned()),
            serde_json::json!("Sub goal"),
        ),
        (Variant::Datetime(moment), serde_json::json!(SAMPLE_RFC3339)),
        (
            Variant::Array(vec![Variant::Int(1), Variant::Bool(false)]),
            serde_json::json!([1, false]),
        ),
        (
            Variant::Object(BTreeMap::from([
                ("target".to_owned(), Variant::Int(100)),
                ("live".to_owned(), Variant::Bool(true)),
            ])),
            serde_json::json!({ "target": 100, "live": true }),
        ),
    ] {
        let content = OverlayConfig::from([(VALUE_KEY.to_owned(), held.clone())]);

        harness
            .service
            .send_to(
                &stored.id,
                &content,
                &ArgStack::new(),
                forge_runtime::OverlaySendOptions::default(),
            )
            .await
            .expect("a bound overlay accepts content");

        let frame = harness
            .sink
            .frames()
            .pop()
            .expect("the delivery reached the sink");
        assert_eq!(
            frame.content[VALUE_KEY], expected,
            "{held:?} reached the page in a shape its runtime renders as something else"
        );
        assert_eq!(
            tagged_variant_nodes(&frame.content),
            Vec::<serde_json::Value>::new(),
            "{held:?} reached the page wrapped in its stored form, which renders as [object Object]"
        );
    }
}

#[tokio::test]
async fn a_reconnecting_page_is_handed_back_the_content_it_was_last_showing() {
    let stored = definition_of_kind("goal-box", GOAL_KIND);
    let harness = harness(vec![stored.clone()], true);
    let content = text_config(&[(LABEL_KEY, "Sub goal"), (VALUE_KEY, "42")]);
    harness
        .service
        .send_to(
            &stored.id,
            &content,
            &ArgStack::new(),
            forge_runtime::OverlaySendOptions {
                duration_ms: Some(4_000),
                caused_by: None,
            },
        )
        .await
        .expect("a bound overlay accepts content");

    harness
        .service
        .overlay_connected(&stored.id, OverlayPage::new(1))
        .await;

    let frames = harness.sink.frames();
    assert_eq!(
        frames.len(),
        2,
        "a reconnect left {} frames instead of the original and its one replay",
        frames.len()
    );
    assert_eq!(
        frames[1],
        ContentFrame {
            identity: stored.id.clone(),
            content: frames[0].content.clone(),
            duration_ms: None,
        },
        "the replay must restore the display without re-running the original timer"
    );
}

#[tokio::test]
async fn a_page_connecting_with_nothing_retained_for_it_receives_no_replay() {
    let stored = definition_of_kind("goal-box", GOAL_KIND);
    let harness = harness(vec![stored.clone()], true);

    for identity in [stored.id.clone(), OverlayId::new("nobody")] {
        harness
            .service
            .overlay_connected(&identity, OverlayPage::new(1))
            .await;
    }

    assert!(
        harness.sink.frames().is_empty(),
        "a page that was never delivered to was handed content on connect: {:?}",
        harness.sink.frames()
    );
}

#[tokio::test]
async fn sending_content_funnels_the_step_fields_over_the_overlays_own_and_retains_the_result() {
    let mut stored = definition_of_kind("goal-box", GOAL_KIND);
    stored.config = text_config(&[
        (LABEL_KEY, "%label% goal"),
        (TARGET_KEY, "100"),
        (TEXT_COLOR_KEY, "green"),
    ]);
    let harness = harness(vec![stored.clone()], true);
    let args = ArgStack::new()
        .set("label".to_owned(), Variant::String("Sub".to_owned()))
        .set("bits".to_owned(), Variant::String("42".to_owned()));

    let delivered = harness
        .service
        .send_to(
            &stored.id,
            &text_config(&[(VALUE_KEY, "%bits%"), (TEXT_COLOR_KEY, "red")]),
            &args,
            forge_runtime::OverlaySendOptions {
                duration_ms: Some(2_000),
                caused_by: None,
            },
        )
        .await
        .expect("a bound overlay of a shipped kind accepts a send");

    assert!(
        matches!(
            delivered,
            OverlayDispatch::Applied(OverlayDelivery::Delivered { sources: 1 })
        ),
        "a replacing look was queued instead of applied, or its connected page went uncounted: {delivered:?}"
    );
    let expected = text_config(&[
        (LABEL_KEY, "Sub goal"),
        (VALUE_KEY, "42"),
        (TARGET_KEY, "100"),
    ]);
    assert_eq!(
        harness.sink.frames(),
        vec![ContentFrame {
            identity: stored.id.clone(),
            content: serde_json::json!({
                LABEL_KEY: "Sub goal",
                VALUE_KEY: "42",
                TARGET_KEY: "100",
            }),
            duration_ms: Some(2_000),
        }],
        "the page received something other than the funnelled content"
    );
    assert_eq!(
        harness.retained_for(&stored.id),
        Some(expected),
        "a replacing kind must retain exactly what it sent"
    );
}

#[tokio::test]
async fn sending_content_with_nothing_serving_still_retains_it_for_the_next_connection() {
    let stored = definition_of_kind("goal-box", GOAL_KIND);
    let harness = harness(vec![stored.clone()], false);

    let delivered = harness
        .service
        .send_to(
            &stored.id,
            &text_config(&[(VALUE_KEY, "42")]),
            &ArgStack::new(),
            forge_runtime::OverlaySendOptions::default(),
        )
        .await
        .expect("a send does not fail merely because no page is connected");

    assert!(
        matches!(delivered, OverlayDispatch::Applied(OverlayDelivery::NoPage)),
        "nothing is serving, so the caller must be told the content landed nowhere: {delivered:?}"
    );
    assert!(
        harness.retained_for(&stored.id).is_some(),
        "content sent while no page was connected was dropped instead of held for the next one"
    );
}

#[tokio::test]
async fn sending_content_refuses_an_unknown_identity_or_an_overlay_type_this_build_lacks() {
    let unshipped = definition_of_kind("vendor-box", UNSHIPPED_KIND);
    let harness = harness(vec![unshipped.clone()], true);

    type Check = fn(&RuntimeError) -> bool;
    for (id, matches_expected, label) in [
        (
            OverlayId::new("nobody"),
            (|e: &RuntimeError| matches!(e, RuntimeError::UnknownOverlay(_))) as Check,
            "an identity no record carries",
        ),
        (
            unshipped.id.clone(),
            (|e: &RuntimeError| matches!(e, RuntimeError::UnavailableOverlayKind { .. })) as Check,
            "an overlay type this build lacks",
        ),
    ] {
        let err = harness
            .service
            .send_to(
                &id,
                &text_config(&[(VALUE_KEY, "42")]),
                &ArgStack::new(),
                forge_runtime::OverlaySendOptions::default(),
            )
            .await
            .expect_err("a send that cannot be resolved must be refused");

        assert!(matches_expected(&err), "{label} produced {err:?}");
    }

    assert!(
        harness.sink.frames().is_empty(),
        "a refused send still pushed a frame to connected pages"
    );
    assert!(
        harness.retained_for(&unshipped.id).is_none(),
        "a refused send left content behind for the next connection to replay"
    );
}

fn filtering_chat(id: &str) -> OverlayDefinition {
    let mut stored = definition_of_kind(id, CHAT_KIND);
    stored.config = OverlayConfig::from([
        ("hide_commands".to_owned(), Variant::Bool(true)),
        (
            "hidden_accounts".to_owned(),
            Variant::String("Nightbot".to_owned()),
        ),
    ]);
    stored
}

fn chat_row(login: &str, message: &str) -> ArgStack {
    ArgStack::new()
        .set("user_login".to_owned(), Variant::String(login.to_owned()))
        .set(
            "message_text".to_owned(),
            Variant::String(message.to_owned()),
        )
}

#[tokio::test]
async fn a_filtered_chat_row_is_withheld_and_never_reaches_the_page() {
    for (args, case) in [
        (chat_row("nightbot", "hello"), "a hidden account"),
        (chat_row("viewer", "!so ada"), "a command"),
    ] {
        let stored = filtering_chat("chat");
        let harness = harness(vec![stored.clone()], true);

        let dispatch = harness
            .service
            .send_to(
                &stored.id,
                &OverlayConfig::new(),
                &args,
                forge_runtime::OverlaySendOptions::default(),
            )
            .await
            .expect("a filtered row is not an error");

        assert!(
            matches!(dispatch, OverlayDispatch::Withheld),
            "{case} was dispatched as {dispatch:?}"
        );
        assert!(
            harness.sink.frames().is_empty(),
            "{case} still reached the page as {:?}",
            harness.sink.frames()
        );
    }
}

#[tokio::test]
async fn a_chat_row_the_filters_pass_reaches_the_page() {
    let stored = filtering_chat("chat");
    let harness = harness(vec![stored.clone()], true);

    let dispatch = harness
        .service
        .send_to(
            &stored.id,
            &OverlayConfig::new(),
            &chat_row("viewer", "hello"),
            forge_runtime::OverlaySendOptions::default(),
        )
        .await
        .expect("a stored chat overlay accepts a send");

    assert!(
        matches!(dispatch, OverlayDispatch::Applied(_)) && harness.sink.frames().len() == 1,
        "a row no filter matches was dispatched as {dispatch:?} with frames {:?}",
        harness.sink.frames()
    );
}

#[tokio::test]
async fn a_test_fire_of_a_chat_overlay_is_shown_even_when_its_sample_matches_the_filters() {
    let sample_login = forge_overlay::SampleContext::neutral()
        .args()
        .get("user_login")
        .and_then(Variant::as_str)
        .expect("the neutral sample names a sender")
        .to_owned();
    let mut stored = filtering_chat("chat");
    stored.config.extend(text_config(&[
        ("hidden_accounts", &sample_login),
        ("author", &sample_login),
        ("message", "!so ada"),
    ]));
    let harness = harness(vec![stored.clone()], true);

    harness
        .service
        .test_fire(&stored.id, &common::NeutralSamples)
        .await
        .expect("a stored chat overlay samples a row");

    assert_eq!(
        harness.sink.frames().len(),
        1,
        "the preview of a filtering chat overlay never reached the page"
    );
}

fn announcement() -> OverlayConfig {
    announcement_content(&AudioAnnouncement {
        clip_id: CLIP_ID,
        clip_path: CLIP_PATH,
        report_path: REPORT_PATH,
        media_type: CLIP_MEDIA_TYPE,
        duration_ms: CLIP_DURATION_MS,
    })
}

#[tokio::test]
async fn audio_reaches_an_overlay_of_any_look_untouched_by_its_content_funnel() {
    for kind_id in [BLANK_KIND, ALERT_KIND, GOAL_KIND, CHAT_KIND, UNSHIPPED_KIND] {
        let receiver = definition_of_kind("receiver", kind_id);
        let harness = harness(vec![receiver.clone()], true);

        let delivered = harness
            .service
            .deliver_audio(&receiver.id, announcement())
            .await
            .expect("any stored overlay can carry audio");

        assert_eq!(
            (delivered, harness.sink.frames()),
            (
                OverlayDelivery::Delivered { sources: 1 },
                vec![ContentFrame {
                    identity: receiver.id.clone(),
                    content: serde_json::json!({
                        "clip_id": CLIP_ID,
                        "clip_path": CLIP_PATH,
                        "report_path": REPORT_PATH,
                        "clip_media_type": CLIP_MEDIA_TYPE,
                        "clip_duration_ms": CLIP_DURATION_MS,
                    }),
                    duration_ms: None,
                }],
            ),
            "a {kind_id} receiver got the announcement filtered, tagged or timed by its look"
        );
    }
}

#[tokio::test]
async fn audio_is_never_retained_even_by_a_look_that_keeps_its_last_content() {
    let goal = definition_of_kind("goal-box", GOAL_KIND);
    let harness = harness(vec![goal.clone()], true);
    harness
        .service
        .send_to(
            &goal.id,
            &text_config(&[(VALUE_KEY, "42")]),
            &ArgStack::new(),
            forge_runtime::OverlaySendOptions::default(),
        )
        .await
        .expect("a goal accepts a send");
    let shown = harness.retained_for(&goal.id);

    harness
        .service
        .deliver_audio(&goal.id, announcement())
        .await
        .expect("a goal can carry audio");

    assert_eq!(
        harness.retained_for(&goal.id),
        shown,
        "an announcement replaced what the goal keeps, so a reconnect replays a spent capability"
    );
}

#[tokio::test]
async fn audio_to_an_identity_no_record_carries_is_refused_without_a_frame() {
    let harness = harness(Vec::new(), true);

    let err = harness
        .service
        .deliver_audio(&OverlayId::new("nobody"), announcement())
        .await
        .expect_err("an unknown receiver must be refused");

    assert!(
        matches!(err, RuntimeError::UnknownOverlay(_)) && harness.sink.frames().is_empty(),
        "an unknown receiver produced {err:?} or still pushed a frame"
    );
}

type SettingsCase = (fn(&MemorySettings), &'static str);

#[tokio::test]
async fn an_overlay_root_setting_that_names_nowhere_falls_back_to_the_default_directory() {
    let cases: [SettingsCase; 3] = [
        (|_| {}, "an absent setting"),
        (
            |settings| settings.seed_raw(&core_settings::SERVER_OVERLAY_ROOT, ""),
            "a setting holding an empty string",
        ),
        (
            |settings| settings.fail_reads(true),
            "a setting storage cannot read",
        ),
    ];
    for (arrange, label) in cases {
        let settings = MemorySettings::new();
        arrange(&settings);

        let service = OverlayServiceHandle::new(OverlayServiceParts {
            default_root: std::path::PathBuf::from("/default/overlays"),
            date_names: forge_runtime::DateNamesHandle::default(),
            tasks: forge_net::TaskOwner::new(),
            repo: Arc::new(MockOverlayRepo::new()) as Arc<dyn OverlayRepo>,
            settings: Arc::new(settings) as Arc<dyn SettingsRepo>,
            kinds: registry(),
            bus: EventBus::new(Arc::new(NullEventLogRepo)),
            frames: common::no_pages(),
            media: None,
            speaker: None,
            latest: None,
        });

        assert_eq!(
            service.root().await,
            std::path::PathBuf::from("/default/overlays"),
            "{label} must resolve to the default overlay directory, never to a relative path"
        );
    }
}

#[tokio::test]
async fn only_a_real_disable_revokes_the_connected_page() {
    let stored = definition("sub-alert");
    let harness = harness(vec![stored.clone()], true);

    harness
        .service
        .set_enabled(&stored.id, false)
        .await
        .expect("the disable persists");
    harness
        .service
        .set_enabled(&stored.id, true)
        .await
        .expect("the enable persists");

    assert_eq!(
        harness.sink.revoked(),
        vec![stored.id],
        "exactly the disable, and only the disable, revokes the page"
    );
}

#[tokio::test]
async fn content_kept_under_an_earlier_look_replays_only_while_the_current_look_keeps_its_last_content()
 {
    for (kind_id, replays) in [
        (GOAL_KIND, true),
        (ALERT_KIND, false),
        (CHAT_KIND, false),
        (UNSHIPPED_KIND, false),
    ] {
        let stored = definition_of_kind("switched-look", kind_id);
        let harness = harness(vec![stored.clone()], true);
        harness.retained.lock().unwrap().push((
            stored.id.clone(),
            text_config(&[(LABEL_KEY, "Sub goal"), (VALUE_KEY, "42")]),
        ));

        harness
            .service
            .overlay_connected(&stored.id, OverlayPage::new(1))
            .await;

        assert_eq!(
            harness.sink.frames().len(),
            usize::from(replays),
            "a page of look {kind_id} connecting over a row kept by an earlier look"
        );
    }
}

fn fired(changes: &mut OverlayDefinitionChanges) -> bool {
    changes.changed().now_or_never() == Some(true)
}

fn service_whose_repo_answers(removed: bool, enabled_changed: bool) -> OverlayServiceHandle {
    let mut repo = MockOverlayRepo::new();
    repo.expect_delete().returning(move |_| Ok(removed));
    repo.expect_set_enabled()
        .returning(move |_, _| Ok(enabled_changed));
    OverlayServiceHandle::new(OverlayServiceParts {
        default_root: std::path::PathBuf::new(),
        date_names: forge_runtime::DateNamesHandle::default(),
        tasks: forge_net::TaskOwner::new(),
        repo: Arc::new(repo) as Arc<dyn OverlayRepo>,
        settings: Arc::new(MemorySettings::new()) as Arc<dyn SettingsRepo>,
        kinds: registry(),
        bus: EventBus::new(Arc::new(NullEventLogRepo)),
        frames: common::no_pages(),
        media: None,
        speaker: None,
        latest: None,
    })
}

#[tokio::test]
async fn a_delete_or_toggle_signals_a_definition_change_only_when_the_store_changed() {
    let id = OverlayId::new("donation-alert");
    for (store_changed, expected) in [(true, true), (false, false)] {
        let service = service_whose_repo_answers(store_changed, store_changed);
        let mut after_delete = service.definition_changes();
        service.delete(&id).await.expect("the delete is answered");
        let mut after_toggle = service.definition_changes();
        service
            .set_enabled(&id, false)
            .await
            .expect("the toggle is answered");

        assert_eq!(
            (fired(&mut after_delete), fired(&mut after_toggle)),
            (expected, expected),
            "the store reported a change: {store_changed}"
        );
    }
}

#[tokio::test]
async fn materializing_signals_a_definition_change_only_for_an_overlay_it_rebuilt() {
    let stored = definition("sub-alert");
    let harness = harness(vec![stored.clone()], true);
    for (id, expected) in [
        (OverlayId::new("no-such-overlay"), false),
        (stored.id, true),
    ] {
        let mut changes = harness.service.definition_changes();

        let outcome = harness
            .service
            .materialize(&id, &common::NeutralSamples)
            .await;

        assert_eq!(
            (outcome.is_ok(), fired(&mut changes)),
            (expected, expected),
            "{id}"
        );
    }
}

struct NamedListener {
    name: &'static str,
    heard: Arc<Mutex<Vec<(&'static str, OverlayId)>>>,
}

#[async_trait]
impl OverlayConnectListener for NamedListener {
    async fn overlay_connected(&self, identity: &OverlayId, _: OverlayPage) {
        self.heard
            .lock()
            .unwrap()
            .push((self.name, identity.clone()));
    }
}

#[tokio::test]
async fn a_connect_fanout_tells_every_listener_in_the_order_given() {
    let heard = Arc::new(Mutex::new(Vec::new()));
    let listener = |name| {
        Arc::new(NamedListener {
            name,
            heard: Arc::clone(&heard),
        }) as Arc<dyn OverlayConnectListener>
    };
    let fanout = OverlayConnectFanout::new(vec![listener("replay"), listener("catch-up")]);
    let id = OverlayId::new("donation-alert");

    fanout.overlay_connected(&id, OverlayPage::new(1)).await;

    assert_eq!(
        heard.lock().unwrap().clone(),
        vec![("replay", id.clone()), ("catch-up", id)]
    );
}

const KEPT_CREDENTIAL: &str = "c0ffee00c0ffee00c0ffee00c0ffee00";
const KEPT_FIELD: &str = "handWritten";
const CREDENTIAL_FIELD: &str = "credential";

fn unshipped_with_kept_config(harness: &Harness, unshipped: &OverlayDefinition) -> PathBuf {
    let directory = harness.root.join(unshipped.id.as_str());
    fs::create_dir_all(&directory).unwrap();
    let kept = serde_json::json!({ CREDENTIAL_FIELD: KEPT_CREDENTIAL, KEPT_FIELD: true });
    fs::write(directory.join(CONFIG_FILE), kept.to_string()).unwrap();
    directory
}

fn read_config(directory: &std::path::Path) -> serde_json::Value {
    serde_json::from_str(&fs::read_to_string(directory.join(CONFIG_FILE)).unwrap()).unwrap()
}

#[tokio::test]
async fn a_pass_rewrites_the_stored_credential_into_the_kept_config_of_an_unavailable_overlay() {
    let mut unshipped = definition("vendor-box");
    unshipped.kind_id = UNSHIPPED_KIND.to_owned();
    let harness = harness(vec![unshipped.clone()], false);
    let directory = unshipped_with_kept_config(&harness, &unshipped);

    let pass = harness
        .service
        .materialize_all(&common::NeutralSamples)
        .await
        .expect("pass");

    let config = read_config(&directory);
    assert_eq!(
        config[CREDENTIAL_FIELD].as_str(),
        Some(unshipped.credential.expose()),
        "a reissued credential never reached the page of an overlay this build lacks"
    );
    assert_eq!(config[KEPT_FIELD], serde_json::Value::Bool(true));
    assert_eq!(
        pass,
        MaterializePass {
            materialized: 0,
            unavailable: 1,
            failed: 0,
        }
    );
}

#[cfg(unix)]
#[tokio::test]
async fn a_kept_config_that_cannot_be_refreshed_counts_as_failed_and_the_pass_carries_on() {
    let mut unshipped = definition("vendor-box");
    unshipped.kind_id = UNSHIPPED_KIND.to_owned();
    let known = definition("sub-alert");
    let harness = harness(vec![unshipped.clone(), known.clone()], false);
    let directory = harness.root.join(unshipped.id.as_str());
    fs::create_dir_all(&directory).unwrap();
    let outside = harness.root.parent().unwrap().join("victim.json");
    fs::write(&outside, "{}").unwrap();
    std::os::unix::fs::symlink(&outside, directory.join(CONFIG_FILE)).unwrap();

    let pass = harness
        .service
        .materialize_all(&common::NeutralSamples)
        .await
        .expect("pass");

    assert_eq!(
        pass,
        MaterializePass {
            materialized: 1,
            unavailable: 0,
            failed: 1,
        },
        "one overlay is counted once, as failed, not also as unavailable"
    );
    assert_eq!(fs::read_to_string(&outside).unwrap(), "{}");
}

#[tokio::test]
async fn saving_an_overlay_refuses_settings_its_look_rejects_and_writes_nothing() {
    let h = harness(Vec::new(), false);
    let mut unknown_choice = definition_of_kind("chat", CHAT_KIND);
    unknown_choice.config_schema_version = current_schema(CHAT_KIND);
    unknown_choice.config = text_config(&[(POSITION_KEY, "middle")]);
    let unshipped = definition_of_kind("vendor", UNSHIPPED_KIND);

    let refusals = [
        h.service.save(&unknown_choice).await,
        h.service.save(&unshipped).await,
    ];

    assert!(
        matches!(
            &refusals,
            [
                Err(RuntimeError::Overlay(
                    forge_overlay::OverlayError::UnknownChoice { .. }
                )),
                Err(RuntimeError::Overlay(
                    forge_overlay::OverlayError::UnknownKind(_)
                )),
            ]
        ),
        "{refusals:?}"
    );
    assert!(h.saved().is_empty());
}

#[tokio::test]
async fn saving_an_overlay_with_settings_its_look_accepts_writes_the_definition_unchanged() {
    let h = harness(Vec::new(), false);
    let mut valid = definition_of_kind("chat", CHAT_KIND);
    valid.config_schema_version = current_schema(CHAT_KIND);
    valid.config = text_config(&[(POSITION_KEY, "top")]);

    h.service.save(&valid).await.unwrap();

    assert_eq!(h.saved(), vec![valid]);
}

struct ChangesAfterRead {
    inner: Arc<dyn OverlayRepo>,
}

#[async_trait]
impl OverlayRepo for ChangesAfterRead {
    async fn list(&self) -> Result<Vec<OverlayDefinition>, StorageError> {
        self.inner.list().await
    }

    async fn get(&self, id: &OverlayId) -> Result<Option<OverlayDefinition>, StorageError> {
        let read = self.inner.get(id).await?;
        self.inner.set_enabled(id, false).await?;
        self.inner.reissue_all_credentials().await?;
        Ok(read)
    }

    async fn get_by_credential(
        &self,
        credential: &OverlayCredential,
    ) -> Result<Option<OverlayDefinition>, StorageError> {
        self.inner.get_by_credential(credential).await
    }

    async fn create(
        &self,
        display_name: &str,
        kind_id: &str,
        config_schema_version: u32,
    ) -> Result<OverlayDefinition, StorageError> {
        self.inner
            .create(display_name, kind_id, config_schema_version)
            .await
    }

    async fn reissue_all_credentials(&self) -> Result<Vec<OverlayId>, StorageError> {
        self.inner.reissue_all_credentials().await
    }

    async fn save(&self, definition: &OverlayDefinition) -> Result<(), StorageError> {
        self.inner.save(definition).await
    }

    async fn update_config(
        &self,
        id: &OverlayId,
        config: &OverlayConfig,
    ) -> Result<bool, StorageError> {
        self.inner.update_config(id, config).await
    }

    async fn rename(&self, id: &OverlayId, display_name: &str) -> Result<bool, StorageError> {
        self.inner.rename(id, display_name).await
    }

    async fn restore(&self, definition: &OverlayDefinition) -> Result<(), StorageError> {
        self.inner.restore(definition).await
    }

    async fn set_enabled(&self, id: &OverlayId, enabled: bool) -> Result<bool, StorageError> {
        self.inner.set_enabled(id, enabled).await
    }

    async fn delete(&self, id: &OverlayId) -> Result<bool, StorageError> {
        self.inner.delete(id).await
    }

    async fn get_retained_content(
        &self,
        id: &OverlayId,
    ) -> Result<Option<OverlayConfig>, StorageError> {
        self.inner.get_retained_content(id).await
    }

    async fn set_retained_content(
        &self,
        id: &OverlayId,
        content: &OverlayConfig,
    ) -> Result<(), StorageError> {
        self.inner.set_retained_content(id, content).await
    }
}

struct RacedOverlay {
    service: OverlayServiceHandle,
    stored: Arc<dyn OverlayRepo>,
    id: OverlayId,
    credential_at_read: String,
    _database_dir: tempfile::TempDir,
}

async fn raced_overlay() -> RacedOverlay {
    let database_dir = tempfile::tempdir().unwrap();
    let url = format!(
        "sqlite://{}",
        database_dir.path().join("forge.db").display()
    );
    let pools = forge_storage_sqlite::connect_pools(&url).await.unwrap();
    forge_storage_sqlite::apply_migrations(pools.writer())
        .await
        .unwrap();
    let stored: Arc<dyn OverlayRepo> =
        Arc::new(forge_storage_sqlite::SqliteOverlayRepo::new(pools));
    let created = stored
        .create("Sub alert", ALERT_KIND, current_schema(ALERT_KIND))
        .await
        .unwrap();
    let service = OverlayServiceHandle::new(OverlayServiceParts {
        default_root: std::path::PathBuf::new(),
        date_names: forge_runtime::DateNamesHandle::default(),
        tasks: forge_net::TaskOwner::new(),
        repo: Arc::new(ChangesAfterRead {
            inner: Arc::clone(&stored),
        }),
        settings: Arc::new(MemorySettings::new()) as Arc<dyn SettingsRepo>,
        kinds: registry(),
        bus: EventBus::new(Arc::new(NullEventLogRepo)),
        frames: common::no_pages(),
        media: None,
        speaker: None,
        latest: None,
    });
    RacedOverlay {
        service,
        stored,
        id: created.id,
        credential_at_read: created.credential.expose().to_owned(),
        _database_dir: database_dir,
    }
}

#[tokio::test]
async fn a_toggle_and_a_reissue_landing_after_the_read_survive_a_config_write() {
    let raced = raced_overlay().await;
    let config = OverlayConfig::from([(
        forge_overlay::config::MARGIN_TOP.to_owned(),
        Variant::Int(forge_overlay::config::MARGIN_MAX_PERCENT),
    )]);

    let written = raced.service.update_config(&raced.id, &config).await;

    assert!(matches!(written, Ok(true)), "got {written:?}");
    let stored = raced.stored.get(&raced.id).await.unwrap().unwrap();
    assert_eq!(stored.config, config);
    assert!(!stored.enabled, "the write brought a disabled overlay back");
    assert_ne!(
        stored.credential.expose(),
        raced.credential_at_read,
        "the write restored a retired credential"
    );
}

#[tokio::test]
async fn a_config_write_on_an_unknown_overlay_reports_a_miss() {
    let harness = harness(Vec::new(), false);

    let written = harness
        .service
        .update_config(&OverlayId::new("gone"), &OverlayConfig::new())
        .await;

    assert!(matches!(written, Ok(false)), "got {written:?}");
}

#[tokio::test]
async fn a_config_write_the_look_refuses_stores_nothing() {
    let mut repo = MockOverlayRepo::new();
    let overlay = definition_of_kind("alert", ALERT_KIND);
    repo.expect_get()
        .returning(move |_| Ok(Some(overlay.clone())));
    repo.expect_update_config().never();
    let service = OverlayServiceHandle::new(OverlayServiceParts {
        default_root: std::path::PathBuf::new(),
        date_names: forge_runtime::DateNamesHandle::default(),
        tasks: forge_net::TaskOwner::new(),
        repo: Arc::new(repo) as Arc<dyn OverlayRepo>,
        settings: Arc::new(MemorySettings::new()) as Arc<dyn SettingsRepo>,
        kinds: registry(),
        bus: EventBus::new(Arc::new(NullEventLogRepo)),
        frames: common::no_pages(),
        media: None,
        speaker: None,
        latest: None,
    });
    let invalid = OverlayConfig::from([(
        forge_overlay::config::MARGIN_TOP.to_owned(),
        Variant::Int(forge_overlay::config::MARGIN_MAX_PERCENT + 1),
    )]);

    let written = service
        .update_config(&OverlayId::new("alert"), &invalid)
        .await;

    assert!(written.is_err(), "got {written:?}");
}

const REFUSED_FILE: &str = "../escape.html";

#[tokio::test]
async fn a_refused_source_write_takes_a_new_override_back_and_leaves_the_page_alone() {
    let overlay = definition("override-new");
    let id = overlay.id.clone();
    let rig = harness(vec![overlay], true);

    let written = rig
        .service
        .write_override(&id, REFUSED_FILE, "<p>edit</p>".to_owned())
        .await;

    let held: Vec<Vec<String>> = rig
        .saved()
        .into_iter()
        .map(|saved| saved.source_overrides)
        .collect();
    assert!(written.is_err(), "{written:?}");
    assert_eq!(held, vec![vec![REFUSED_FILE.to_owned()], Vec::new()]);
    assert!(rig.sink.reloads().is_empty());
}

#[tokio::test]
async fn a_refused_write_of_an_already_held_override_changes_no_record() {
    let mut overlay = definition("override-held");
    overlay.source_overrides = vec![REFUSED_FILE.to_owned()];
    let id = overlay.id.clone();
    let rig = harness(vec![overlay], true);

    let written = rig
        .service
        .write_override(&id, REFUSED_FILE, "<p>edit</p>".to_owned())
        .await;

    assert!(written.is_err(), "{written:?}");
    assert!(rig.saved().is_empty());
}

#[tokio::test]
async fn an_override_of_an_unknown_overlay_reports_nothing_written() {
    let rig = harness(Vec::new(), true);

    let written = rig
        .service
        .write_override(&OverlayId::new("gone"), MARKUP_FILE, String::new())
        .await;

    assert!(!written.unwrap());
    assert!(rig.saved().is_empty());
}

#[tokio::test]
async fn a_landed_override_is_recorded_once_and_reloads_the_page() {
    let overlay = definition("override-landed");
    let id = overlay.id.clone();
    let rig = harness(vec![overlay.clone()], true);
    rig.service
        .materialize(&id, &common::NeutralSamples)
        .await
        .unwrap();
    let reloads_before = rig.sink.reloads().len();

    let written = rig
        .service
        .write_override(&id, STYLE_FILE, "p {}".to_owned())
        .await;

    assert!(written.unwrap());
    assert_eq!(
        rig.saved()
            .last()
            .map(|saved| saved.source_overrides.clone()),
        Some(vec![STYLE_FILE.to_owned()])
    );
    assert_eq!(rig.sink.reloads().len(), reloads_before + 1);
}

#[tokio::test]
async fn a_chat_removal_reaches_every_enabled_chat_overlay_and_no_other_kind() {
    let chat = definition_of_kind("chat", CHAT_KIND);
    let other_chat = definition_of_kind("other-chat", CHAT_KIND);
    let mut disabled_chat = definition_of_kind("off-chat", CHAT_KIND);
    disabled_chat.enabled = false;
    let alert = definition_of_kind("alert", ALERT_KIND);
    let rig = harness(
        vec![chat.clone(), other_chat.clone(), disabled_chat, alert],
        true,
    );
    let removal = RowRemoval {
        platform: forge_types::PlatformId::Twitch,
        scope: RowRemovalScope::Message {
            message_id: "m-1".to_owned(),
        },
    };

    rig.service.remove_chat_rows(&removal).await;

    assert_eq!(
        rig.sink.removals(),
        vec![(chat.id, removal.clone()), (other_chat.id, removal)]
    );
    assert!(rig.sink.frames().is_empty());
}

const AVATAR_KEY: &str = "avatar";
const VIEWER_ID: &str = "42";
const VIEWER_IMAGE: &str = "https://static-cdn.jtvnw.net/jtv_user_pictures/42.png";

struct OneViewerSource;

#[async_trait]
impl forge_platform_core::ViewerAvatarSource for OneViewerSource {
    fn batch_limit(&self) -> usize {
        100
    }

    async fn avatars(&self, viewer_ids: &[String]) -> forge_platform_core::AvatarLookup {
        forge_platform_core::AvatarLookup::Answered(
            viewer_ids
                .iter()
                .filter(|id| id.as_str() == VIEWER_ID)
                .map(|id| (id.clone(), VIEWER_IMAGE.to_owned()))
                .collect(),
        )
    }
}

fn avatar_chat(id: &str, enabled: bool) -> OverlayDefinition {
    let mut stored = definition_of_kind(id, CHAT_KIND);
    stored.enabled = enabled;
    stored.config = OverlayConfig::from([("show_avatars".to_owned(), Variant::Bool(true))]);
    stored
}

fn viewer_slot() -> forge_overlay::ChatAvatarSlot {
    forge_overlay::ChatAvatarSlot {
        platform: forge_types::PlatformId::Twitch,
        user_id: VIEWER_ID.to_owned(),
    }
}

fn twitch_row() -> OverlayConfig {
    text_config(&[("platform", "twitch"), ("user_id", VIEWER_ID)])
}

async fn send_row(harness: &Harness, id: &OverlayId) {
    harness
        .service
        .send_to(
            id,
            &twitch_row(),
            &chat_row("viewer", "hello"),
            forge_runtime::OverlaySendOptions::default(),
        )
        .await
        .expect("a stored chat overlay accepts a row");
}

#[tokio::test]
async fn a_row_whose_viewer_avatar_is_already_known_carries_it_in_its_content() {
    let stored = avatar_chat("chat", true);
    let harness = harness(vec![stored.clone()], true);
    let avatars = harness.service.chat_avatars();
    avatars.register(forge_types::PlatformId::Twitch, Arc::new(OneViewerSource));
    avatars.ask(viewer_slot());
    avatars.resolve_batch(forge_types::PlatformId::Twitch).await;

    send_row(&harness, &stored.id).await;

    assert_eq!(
        harness.sink.frames()[0].content[AVATAR_KEY],
        serde_json::json!(VIEWER_IMAGE)
    );
}

#[tokio::test]
async fn a_row_whose_viewer_avatar_is_unknown_goes_out_unchanged() {
    let stored = avatar_chat("chat", true);
    let harness = harness(vec![stored.clone()], true);
    harness
        .service
        .chat_avatars()
        .register(forge_types::PlatformId::Twitch, Arc::new(OneViewerSource));

    send_row(&harness, &stored.id).await;

    assert_eq!(
        harness.sink.frames()[0].content[AVATAR_KEY],
        serde_json::json!("")
    );
}

#[tokio::test]
async fn a_looked_up_avatar_reaches_every_enabled_chat_overlay_and_no_other() {
    let first = avatar_chat("chat-a", true);
    let disabled = avatar_chat("chat-off", false);
    let alert = definition("alert");
    let last = avatar_chat("chat-b", true);
    let harness = harness(vec![first.clone(), disabled, alert, last.clone()], true);
    harness
        .service
        .chat_avatars()
        .register(forge_types::PlatformId::Twitch, Arc::new(OneViewerSource));

    send_row(&harness, &first.id).await;
    tokio::time::timeout(std::time::Duration::from_secs(30), async {
        while !harness.sink.avatars().iter().any(|(id, _)| id == &last.id) {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the looked-up avatar never reached the last chat overlay");

    let fill = RowAvatar {
        slot: viewer_slot(),
        url: VIEWER_IMAGE.to_owned(),
    };
    assert_eq!(
        harness.sink.avatars(),
        vec![(first.id.clone(), fill.clone()), (last.id.clone(), fill)]
    );
}
