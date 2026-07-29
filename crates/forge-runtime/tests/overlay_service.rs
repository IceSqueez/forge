#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use forge_overlay::{
    CONFIG_FILE, GENERATOR_VERSION, MARKUP_FILE, OverlayKindRegistry, RESERVED_DIRECTORY,
    STYLE_FILE, register_builtin_kinds, sample_content,
};
use forge_platform_core::paths;
use forge_runtime::overlay_service::OVERLAY_TEST_FIRE_KIND;
use forge_runtime::{
    EventBus, MaterializePass, NullEventLogRepo, OverlayConnectListener, OverlayFrameSink,
    OverlayServiceError, OverlayServiceHandle,
};
use forge_storage::settings::MockSettingsRepo;
use forge_storage::{
    MockOverlayRepo, OverlayConfig, OverlayCredential, OverlayDefinition, OverlayId, OverlayRepo,
    SettingsRepo, StorageError, reserved_keys,
};
use forge_types::{ArgStack, Variant};
use tempfile::TempDir;
use time::OffsetDateTime;

const ALERT_KIND: &str = "overlay.alert";
const CHAT_KIND: &str = "overlay.chat";
const GOAL_KIND: &str = "overlay.goal";
const UNSHIPPED_KIND: &str = "overlay.vendor_unshipped";

const LABEL_KEY: &str = "label";
const VALUE_KEY: &str = "value";
const TARGET_KEY: &str = "target";
const ACCENT_KEY: &str = "accent";

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
}

impl RecordingSink {
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
    ) -> usize {
        self.frames.lock().unwrap().push(ContentFrame {
            identity: identity.clone(),
            content,
            duration_ms,
        });
        1
    }

    async fn deliver_reload(&self, identity: &OverlayId) {
        self.reloads.lock().unwrap().push(identity.clone());
    }

    async fn revoke(&self, identity: &OverlayId) {
        self.revoked.lock().unwrap().push(identity.clone());
    }
}

fn content_json(content: &OverlayConfig) -> serde_json::Value {
    serde_json::Value::Object(
        content
            .iter()
            .map(|(key, value)| (key.clone(), value.to_json()))
            .collect(),
    )
}

fn definition(id: &str) -> OverlayDefinition {
    OverlayDefinition {
        id: OverlayId::new(id),
        display_name: "Sub alert".to_owned(),
        kind_id: ALERT_KIND.to_owned(),
        enabled: true,
        position: 0,
        config: OverlayConfig::new(),
        config_schema_version: 1,
        generator_version: 0,
        source_overrides: Vec::new(),
        credential: OverlayCredential::new("2f8b1d0c9a7e6f5b4c3d2e1f0a9b8c7d"),
        created_at: OffsetDateTime::UNIX_EPOCH,
        updated_at: OffsetDateTime::UNIX_EPOCH,
    }
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

    let mut settings = MockSettingsRepo::new();
    let configured = root.to_string_lossy().into_owned();
    settings
        .expect_get_string()
        .returning(move |key| match key {
            reserved_keys::SERVER_OVERLAY_ROOT => Ok(Some(configured.clone())),
            _ => Ok(None),
        });

    let bus = EventBus::new(Arc::new(NullEventLogRepo));
    let sink = Arc::new(RecordingSink::default());
    let frames: Option<Arc<dyn OverlayFrameSink>> =
        attach_sink.then(|| Arc::clone(&sink) as Arc<dyn OverlayFrameSink>);

    let service = OverlayServiceHandle::new(
        Arc::new(repo) as Arc<dyn OverlayRepo>,
        Arc::new(settings) as Arc<dyn SettingsRepo>,
        registry(),
        Arc::clone(&bus),
        frames,
    );

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
            .materialize_all()
            .await
            .expect("the first pass builds the page");
        let directory = harness.directory_of(&claimed.id);
        let owned = directory.join(STYLE_FILE);
        let user_body = "#stage { opacity: 0.5; }\n";
        fs::write(&owned, user_body).unwrap();
        fs::write(directory.join(MARKUP_FILE), "stale markup").unwrap();

        match trigger {
            Regeneration::BootPass => {
                harness.service.materialize_all().await.expect("boot pass");
            }
            Regeneration::SingleOverlay => {
                harness
                    .service
                    .materialize(&claimed.id)
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
        .test_fire(&stored.id)
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

#[tokio::test]
async fn test_fire_delivers_one_frame_carrying_the_content_it_returns() {
    let stored = definition("sub-alert");
    let harness = harness(vec![stored.clone()], true);

    let fired = harness.service.test_fire(&stored.id).await.expect("fire");

    let frames = harness.sink.frames();
    assert_eq!(frames.len(), 1, "a single test fire delivered {frames:?}");
    assert_eq!(
        frames[0].identity, stored.id,
        "a test fire reached an overlay it does not target"
    );
    assert_eq!(
        frames[0].content,
        content_json(&fired.content),
        "the caller previews content the page never received"
    );
    assert!(fired.delivered);
}

#[tokio::test]
async fn test_fire_without_a_serving_sink_still_returns_the_sample_and_says_it_landed_nowhere() {
    let stored = definition("sub-alert");
    let harness = harness(vec![stored.clone()], false);

    let fired = harness.service.test_fire(&stored.id).await.expect("fire");

    assert!(
        !fired.delivered,
        "nothing is serving, so the caller must be told the preview ran alone"
    );
    assert_eq!(
        fired.content,
        sample_content(
            registry().get(&stored.kind_id).expect("a shipped kind"),
            &stored.config
        ),
        "the caller still needs the very content a connected page would have received"
    );
}

#[tokio::test]
async fn test_fire_refuses_and_delivers_nothing_when_no_sample_can_be_built() {
    let mut unshipped = definition("vendor-box");
    unshipped.kind_id = UNSHIPPED_KIND.to_owned();
    let harness = harness(vec![unshipped.clone()], true);
    let mut subscription = harness.bus.subscribe();

    type Check = fn(&OverlayServiceError) -> bool;
    for (id, matches_expected, label) in [
        (
            OverlayId::new("nobody"),
            (|e: &OverlayServiceError| matches!(e, OverlayServiceError::Unknown(_))) as Check,
            "an identity no record carries",
        ),
        (
            unshipped.id.clone(),
            (|e: &OverlayServiceError| matches!(e, OverlayServiceError::UnavailableKind { .. }))
                as Check,
            "an overlay type this build lacks",
        ),
    ] {
        let err = harness
            .service
            .test_fire(&id)
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

    let pass = harness.service.materialize_all().await.expect("pass");

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
    let harness = harness(vec![stale.clone(), current.clone()], false);

    harness.service.materialize_all().await.expect("pass");

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

    let pass = harness.service.materialize_all().await.expect("pass");

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
    harness.service.materialize_all().await.expect("pass");
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
        .materialize(&stored.id)
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
            .deliver_content(&stored.id, content.clone(), None)
            .await
            .expect("a bound overlay accepts content");

        let expected = (kind_id == GOAL_KIND).then(|| content.clone());
        assert_eq!(
            harness.retained_for(&stored.id),
            expected,
            "{label} retained the wrong thing, so the next connection replays the wrong thing"
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
        .deliver_content(&stored.id, content.clone(), Some(4_000))
        .await
        .expect("a bound overlay accepts content");

    harness.service.overlay_connected(&stored.id).await;

    let frames = harness.sink.frames();
    assert_eq!(
        frames.len(),
        2,
        "a reconnect delivered {} frames instead of the one replay",
        frames.len() - 1
    );
    assert_eq!(
        frames[1],
        ContentFrame {
            identity: stored.id.clone(),
            content: content_json(&content),
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
        harness.service.overlay_connected(&identity).await;
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
        (ACCENT_KEY, "green"),
    ]);
    let harness = harness(vec![stored.clone()], true);
    let args = ArgStack::new()
        .set("label".to_owned(), Variant::String("Sub".to_owned()))
        .set("bits".to_owned(), Variant::String("42".to_owned()));

    let delivered = harness
        .service
        .send_to(
            &stored.id,
            &text_config(&[(VALUE_KEY, "%bits%"), (ACCENT_KEY, "red")]),
            &args,
            Some(2_000),
        )
        .await
        .expect("a bound overlay of a shipped kind accepts a send");

    assert!(delivered, "a connected page was not counted as reached");
    let expected = text_config(&[
        (LABEL_KEY, "Sub goal"),
        (VALUE_KEY, "42"),
        (TARGET_KEY, "100"),
    ]);
    assert_eq!(
        harness.sink.frames(),
        vec![ContentFrame {
            identity: stored.id.clone(),
            content: content_json(&expected),
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
            None,
        )
        .await
        .expect("a send does not fail merely because no page is connected");

    assert!(
        !delivered,
        "nothing is serving, so the caller must be told the content landed nowhere"
    );
    assert!(
        harness.retained_for(&stored.id).is_some(),
        "content sent while no page was connected was dropped instead of held for the next one"
    );
}

#[tokio::test]
async fn sending_content_refuses_an_identity_or_an_overlay_type_it_cannot_resolve() {
    let unshipped = definition_of_kind("vendor-box", UNSHIPPED_KIND);
    let harness = harness(vec![unshipped.clone()], true);

    type Check = fn(&OverlayServiceError) -> bool;
    for (id, matches_expected, label) in [
        (
            OverlayId::new("nobody"),
            (|e: &OverlayServiceError| matches!(e, OverlayServiceError::Unknown(_))) as Check,
            "an identity no record carries",
        ),
        (
            unshipped.id.clone(),
            (|e: &OverlayServiceError| matches!(e, OverlayServiceError::UnavailableKind { .. }))
                as Check,
            "an overlay type this build lacks",
        ),
    ] {
        let err = harness
            .service
            .send_to(
                &id,
                &text_config(&[(VALUE_KEY, "42")]),
                &ArgStack::new(),
                None,
            )
            .await
            .expect_err("a send that cannot be resolved must be refused");

        assert!(matches_expected(&err), "{label} produced {err:?}");
    }

    assert!(
        harness.sink.frames().is_empty(),
        "a refused send still pushed a frame to connected pages"
    );
}

#[tokio::test]
async fn an_overlay_root_setting_that_names_nowhere_falls_back_to_the_default_directory() {
    for (case, label) in [
        (0u8, "an absent setting"),
        (1, "a setting holding an empty string"),
        (2, "a setting storage cannot read"),
    ] {
        let mut settings = MockSettingsRepo::new();
        settings.expect_get_string().returning(move |_| match case {
            0 => Ok(None),
            1 => Ok(Some(String::new())),
            _ => Err(StorageError::Connection {
                reason: "unreadable".to_owned(),
            }),
        });

        let service = OverlayServiceHandle::new(
            Arc::new(MockOverlayRepo::new()) as Arc<dyn OverlayRepo>,
            Arc::new(settings) as Arc<dyn SettingsRepo>,
            registry(),
            EventBus::new(Arc::new(NullEventLogRepo)),
            None,
        );

        assert_eq!(
            service.root().await,
            paths::overlays_dir(),
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
