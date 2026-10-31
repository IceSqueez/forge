#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod common;
use std::collections::HashMap;
use std::fs;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use forge_events::Event;
use forge_overlay::config::SPEECH;
use forge_overlay::{OverlayKindRegistry, SAMPLE_FILE, register_builtin_kinds};
use forge_registry::{
    CancelSignal, EventFilter, FormField, KindPlatformContract, SubActionRegistry, TriggerCategory,
    TriggerKindDescriptor, TriggerRegistry, TriggerVariables,
};
use forge_runtime::ActionsService;
use forge_runtime::{
    EngineVoiceError, EventBus, NullEventLogRepo, OVERLAY_SEND_KIND_ID, OVERLAY_TARGET_KEY,
    OverlayDispatch, OverlayFrameSink, OverlayReceivers, OverlaySampleSource, OverlaySamples,
    OverlayServiceHandle, OverlayServiceParts, ShowSpeech, SpeakDispatchError, SpeakDispatcher,
    SpeechOrigin, SpeechStartSignal,
};
use forge_storage::MockActionRepo;
use forge_storage::MockHistoryRepo;
use forge_storage::MockQueueRepo;
use forge_storage::MockTriggerInstanceRepo;
use forge_storage::settings::core_settings;
use forge_storage::test_support::MemorySettings;
use forge_storage::{
    MockOverlayRepo, OverlayConfig, OverlayDefinition, OverlayId, OverlayRepo, SettingsRepo,
    StorageError,
};
use forge_types::{
    Action, ActionId, ArgStack, PermissionRung, PlatformScope, QueueId, SubActionConfig,
    SubActionStep, TriggerConfig, TriggerInstance, TriggerInstanceId, Variant,
};
use tempfile::TempDir;
use tokio::sync::Notify;

const OVERLAY: &str = "sub-alert";
const ALERT_KIND: &str = "overlay.alert";
const CHATTER_KIND: &str = "stub.chatter";
const HEADLINE_KEY: &str = "headline";
const SUBLINE_KEY: &str = "subline";
const MESSAGE_TOKEN: &str = "%message_text%";
const VIEWER_TOKEN: &str = "%viewer_count%";
const SHOW_HEADLINE: &str = "A new donation";
const SHOW_SPEECH: &str = "Thanks for the five";
const HANG_GUARD: Duration = Duration::from_secs(60);

struct ChatterTrigger;

impl TriggerKindDescriptor for ChatterTrigger {
    fn id(&self) -> &str {
        CHATTER_KIND
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Chat
    }

    fn label(&self) -> &str {
        "Chat message"
    }

    fn summary(&self) -> &str {
        "Fires on a chat message"
    }

    fn search_text(&self) -> &str {
        "chat"
    }

    fn icon_name(&self) -> &str {
        "chat"
    }

    fn platform_contract(&self) -> KindPlatformContract {
        KindPlatformContract::Universal
    }

    fn default_config(&self) -> TriggerConfig {
        TriggerConfig::new()
    }

    fn config_fields(&self) -> Vec<FormField> {
        Vec::new()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: None,
            kind_prefix: None,
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, _event: &Event) -> bool {
        true
    }

    fn variables(&self) -> Option<TriggerVariables> {
        Some(TriggerVariables::new().message_text(|_| String::new()))
    }
}

#[derive(Default)]
struct RecordingPage {
    contents: Mutex<Vec<serde_json::Value>>,
    reloads: AtomicUsize,
}

impl RecordingPage {
    fn headlines(&self) -> Vec<String> {
        self.contents
            .lock()
            .unwrap()
            .iter()
            .filter_map(|content| content[HEADLINE_KEY].as_str().map(str::to_owned))
            .collect()
    }

    fn reloads(&self) -> usize {
        self.reloads.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl OverlayFrameSink for RecordingPage {
    async fn deliver_content(
        &self,
        _: &OverlayId,
        content: serde_json::Value,
        _: Option<u64>,
    ) -> OverlayReceivers {
        self.contents.lock().unwrap().push(content);
        OverlayReceivers {
            sources: 1,
            preview_tabs: 0,
        }
    }

    async fn deliver_reload(&self, _: &OverlayId) {
        self.reloads.fetch_add(1, Ordering::SeqCst);
    }

    async fn revoke(&self, _: &OverlayId) {}
}

#[derive(Default)]
struct ListeningSpeaker {
    spoken: Mutex<Vec<String>>,
    heard: Notify,
}

impl ListeningSpeaker {
    fn spoken(&self) -> Vec<String> {
        self.spoken.lock().unwrap().clone()
    }
}

#[async_trait]
impl SpeakDispatcher for ListeningSpeaker {
    async fn speak(
        &self,
        _: String,
        _: Option<String>,
        _: SpeechOrigin,
    ) -> Result<(), SpeakDispatchError> {
        panic!("show speech took the global speak path")
    }

    fn check_voice(&self, _: &str) -> Result<(), EngineVoiceError> {
        Ok(())
    }

    async fn speak_for_show(
        &self,
        speech: ShowSpeech,
        _: CancelSignal,
        started: SpeechStartSignal,
    ) -> Result<(), SpeakDispatchError> {
        self.spoken.lock().unwrap().push(speech.text);
        started.mark_started();
        self.heard.notify_one();
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
enum Catalog {
    FeedsTheOverlay,
    FeedsNothing,
    Unreadable,
}

struct Wired {
    _home: TempDir,
    overlay: OverlayId,
    sample_path: std::path::PathBuf,
    page: Arc<RecordingPage>,
    speaker: Arc<ListeningSpeaker>,
    samples: Arc<dyn OverlaySampleSource>,
    service: OverlayServiceHandle,
}

fn overlay_kinds() -> Arc<OverlayKindRegistry> {
    let mut reg = OverlayKindRegistry::new();
    register_builtin_kinds(&mut reg).expect("the builtin overlay kinds register");
    Arc::new(reg)
}

fn sub_actions() -> Arc<SubActionRegistry> {
    common::core_steps_off_any_runtime()
}

fn triggers() -> Arc<TriggerRegistry> {
    let mut reg = TriggerRegistry::new();
    reg.register(Box::new(ChatterTrigger))
        .expect("the stub trigger registers");
    Arc::new(reg)
}

fn definition() -> OverlayDefinition {
    common::overlay_definition(
        OVERLAY,
        "Sub alert",
        ALERT_KIND,
        OverlayConfig::from([
            (
                HEADLINE_KEY.to_owned(),
                Variant::String(MESSAGE_TOKEN.to_owned()),
            ),
            (
                SUBLINE_KEY.to_owned(),
                Variant::String(VIEWER_TOKEN.to_owned()),
            ),
        ]),
    )
}

fn send_step() -> SubActionStep {
    let mut config = SubActionConfig::new();
    config.insert(
        OVERLAY_TARGET_KEY.to_owned(),
        Variant::String(OVERLAY.to_owned()),
    );
    SubActionStep {
        kind_id: OVERLAY_SEND_KIND_ID.to_owned(),
        config,
        enabled: true,
        continue_on_error: false,
        condition: None,
        label: None,
    }
}

fn feeding_action() -> (Action, Vec<TriggerInstance>) {
    let action = common::sequential_action(
        ActionId::new(),
        "announce the chat message",
        QueueId::new(),
        vec![send_step()],
    );
    let instance = TriggerInstance {
        id: TriggerInstanceId::new(),
        kind_id: CHATTER_KIND.to_owned(),
        name: "a chat message".to_owned(),
        overrides: TriggerConfig::new(),
        enabled: true,
        user_defined: true,
        platform_scope: PlatformScope::Any,
        cooldown_secs: 0,
        cooldown_global: true,
        permission_rung: PermissionRung::default(),
    };
    (action, vec![instance])
}

fn actions_service(catalog: Catalog) -> Arc<ActionsService> {
    let (action, instances) = feeding_action();
    let listed = match catalog {
        Catalog::FeedsTheOverlay => vec![action.clone()],
        Catalog::FeedsNothing | Catalog::Unreadable => Vec::new(),
    };
    let linked: HashMap<ActionId, Vec<TriggerInstance>> = HashMap::from([(action.id, instances)]);

    let mut action_repo = MockActionRepo::new();
    action_repo.expect_list().returning(move || match catalog {
        Catalog::Unreadable => Err(StorageError::Connection {
            reason: "action store offline".to_owned(),
        }),
        Catalog::FeedsTheOverlay | Catalog::FeedsNothing => Ok(listed.clone()),
    });

    let mut trigger_repo = MockTriggerInstanceRepo::new();
    trigger_repo
        .expect_list_for_action()
        .returning(move |id| Ok(linked.get(&id).cloned().unwrap_or_default()));

    Arc::new(ActionsService::new(
        Arc::new(action_repo),
        Arc::new(MockQueueRepo::new()),
        Arc::new(MockHistoryRepo::new()),
        Arc::new(trigger_repo),
    ))
}

fn wired(catalog: Catalog) -> Wired {
    let home = TempDir::new().unwrap();
    let root = home.path().join("overlays");
    let stored = definition();

    let mut repo = MockOverlayRepo::new();
    let listed = vec![stored.clone()];
    repo.expect_list().returning(move || Ok(listed.clone()));
    let held = stored.clone();
    repo.expect_get()
        .returning(move |id| Ok((id == &held.id).then(|| held.clone())));
    repo.expect_save().returning(|_| Ok(()));
    repo.expect_get_retained_content().returning(|_| Ok(None));
    repo.expect_set_retained_content().returning(|_, _| Ok(()));

    let settings = MemorySettings::new();
    settings
        .seed(
            &core_settings::SERVER_OVERLAY_ROOT,
            &Some(root.to_string_lossy().into_owned()),
        )
        .expect("overlay root setting");

    let page = Arc::new(RecordingPage::default());
    let speaker = Arc::new(ListeningSpeaker::default());
    let service = OverlayServiceHandle::new(OverlayServiceParts {
        repo: Arc::new(repo) as Arc<dyn OverlayRepo>,
        settings: Arc::new(settings) as Arc<dyn SettingsRepo>,
        kinds: overlay_kinds(),
        bus: EventBus::new(Arc::new(NullEventLogRepo)),
        frames: Arc::clone(&page) as Arc<dyn OverlayFrameSink>,
        media: None,
        speaker: Some(Arc::clone(&speaker) as Arc<dyn SpeakDispatcher>),
        latest: None,
    });
    let samples = Arc::new(OverlaySamples::new(
        actions_service(catalog),
        sub_actions(),
        triggers(),
    ));

    Wired {
        _home: home,
        overlay: stored.id.clone(),
        sample_path: root.join(OVERLAY).join(SAMPLE_FILE),
        page,
        speaker,
        samples,
        service,
    }
}

async fn sample_json_content(wired: &Wired) -> serde_json::Map<String, serde_json::Value> {
    wired
        .service
        .materialize(&wired.overlay, wired.samples.as_ref())
        .await
        .expect("the overlay page is built");
    let raw = fs::read_to_string(&wired.sample_path).expect("the sample document is written");
    let document: serde_json::Value =
        serde_json::from_str(&raw).expect("the sample document is valid JSON");
    document["content"]
        .as_object()
        .expect("the preview page reads content as an object")
        .clone()
}

async fn test_fire_content(wired: &Wired) -> OverlayConfig {
    wired
        .service
        .test_fire(&wired.overlay, wired.samples.as_ref())
        .await
        .expect("the overlay samples its event")
        .content
}

fn text_at(content: &OverlayConfig, key: &str) -> String {
    content
        .get(key)
        .and_then(Variant::as_str)
        .unwrap_or_else(|| panic!("the sample left {key} unfilled"))
        .to_owned()
}

#[tokio::test]
async fn a_wired_overlay_samples_the_feeding_triggers_declarations_in_its_sample_document() {
    let wired = wired(Catalog::FeedsTheOverlay);
    let content = sample_json_content(&wired).await;

    assert_eq!(
        content[SUBLINE_KEY].as_str(),
        Some(VIEWER_TOKEN),
        "the page previewed a variable the feeding trigger never declares",
    );
    assert_ne!(
        content[HEADLINE_KEY].as_str(),
        Some(MESSAGE_TOKEN),
        "the page previewed the feeding trigger's own variable as a raw token",
    );
}

#[tokio::test]
async fn a_wired_overlay_sends_its_test_fire_through_the_same_sample_the_page_reads() {
    let wired = wired(Catalog::FeedsTheOverlay);
    let document = sample_json_content(&wired).await;
    let fired = test_fire_content(&wired).await;

    for key in [HEADLINE_KEY, SUBLINE_KEY] {
        assert_eq!(
            document[key].as_str(),
            Some(text_at(&fired, key).as_str()),
            "the test fire and the sample document disagree about {key}",
        );
    }
}

#[tokio::test]
async fn an_overlay_with_no_readable_feed_samples_the_neutral_vocabulary() {
    for catalog in [Catalog::FeedsNothing, Catalog::Unreadable] {
        let wired = wired(catalog);
        let fired = test_fire_content(&wired).await;

        for (key, token) in [(HEADLINE_KEY, MESSAGE_TOKEN), (SUBLINE_KEY, VIEWER_TOKEN)] {
            assert_ne!(
                text_at(&fired, key),
                token,
                "{catalog:?} previewed {token} as a raw token",
            );
        }
    }
}

#[tokio::test]
async fn a_page_built_from_the_real_samples_then_plays_a_spoken_show_from_the_same_service() {
    let wired = wired(Catalog::FeedsTheOverlay);
    let sample = sample_json_content(&wired).await;
    let show = OverlayConfig::from([
        (
            HEADLINE_KEY.to_owned(),
            Variant::String(SHOW_HEADLINE.to_owned()),
        ),
        (SPEECH.to_owned(), Variant::String(SHOW_SPEECH.to_owned())),
    ]);

    let dispatch = wired
        .service
        .send_to(&wired.overlay, &show, &ArgStack::new(), None, None)
        .await
        .expect("a stored overlay accepts a send");
    assert!(
        matches!(dispatch, OverlayDispatch::Queued(_)),
        "an alert with speech must join the show queue, got {dispatch:?}"
    );
    tokio::time::timeout(HANG_GUARD, wired.speaker.heard.notified())
        .await
        .expect("the queued show never reached the speaker");

    assert_eq!(
        (
            sample[SUBLINE_KEY].as_str(),
            wired.page.reloads(),
            wired.page.headlines(),
            wired.speaker.spoken(),
        ),
        (
            Some(VIEWER_TOKEN),
            1,
            vec![SHOW_HEADLINE.to_owned()],
            vec![SHOW_SPEECH.to_owned()],
        ),
    );
}
