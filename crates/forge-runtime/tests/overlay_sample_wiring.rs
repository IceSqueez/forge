#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::HashMap;
use std::fs;
use std::sync::Arc;

use forge_events::Event;
use forge_overlay::{OverlayKindRegistry, SAMPLE_FILE, register_builtin_kinds};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, SubActionRegistry, TriggerCategory,
    TriggerKindDescriptor, TriggerRegistry, TriggerVariables,
};
use forge_runtime::actions::ActionsService;
use forge_runtime::sub_action_runners::OverlaySendRunner;
use forge_runtime::{
    EventBus, NullEventLogRepo, OVERLAY_SEND_KIND_ID, OVERLAY_TARGET_KEY, OverlayServiceCell,
    OverlayServiceHandle,
};
use forge_storage::action::MockActionRepo;
use forge_storage::history::MockHistoryRepo;
use forge_storage::queue::MockQueueRepo;
use forge_storage::settings::MockSettingsRepo;
use forge_storage::soundboard::MockSoundboardClipsRepo;
use forge_storage::trigger_instance::MockTriggerInstanceRepo;
use forge_storage::{
    MockOverlayRepo, OverlayConfig, OverlayCredential, OverlayDefinition, OverlayId, OverlayRepo,
    SettingsRepo, reserved_keys,
};
use forge_types::{
    Action, ActionId, ExecutionMode, PermissionRung, PlatformScope, QueueId, SubActionConfig,
    SubActionStep, TriggerConfig, TriggerInstance, TriggerInstanceId, Variant,
};
use tempfile::TempDir;
use time::OffsetDateTime;

const OVERLAY: &str = "sub-alert";
const ALERT_KIND: &str = "overlay.alert";
const CHATTER_KIND: &str = "stub.chatter";
const HEADLINE_KEY: &str = "headline";
const SUBLINE_KEY: &str = "subline";
const MESSAGE_TOKEN: &str = "%message_text%";
const VIEWER_TOKEN: &str = "%viewer_count%";

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

    fn condition_display(&self, _config: &TriggerConfig) -> String {
        String::new()
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

struct Wired {
    _home: TempDir,
    overlay: OverlayId,
    sample_path: std::path::PathBuf,
    service: OverlayServiceHandle,
}

fn overlay_kinds() -> Arc<OverlayKindRegistry> {
    let mut reg = OverlayKindRegistry::new();
    register_builtin_kinds(&mut reg).expect("the builtin overlay kinds register");
    Arc::new(reg)
}

fn sub_actions() -> Arc<SubActionRegistry> {
    let mut reg = SubActionRegistry::new();
    reg.register(Box::new(OverlaySendRunner::new(OverlayServiceCell::new())))
        .expect("the overlay send runner registers");
    Arc::new(reg)
}

fn triggers() -> Arc<TriggerRegistry> {
    let mut reg = TriggerRegistry::new();
    reg.register(Box::new(ChatterTrigger))
        .expect("the stub trigger registers");
    Arc::new(reg)
}

fn definition() -> OverlayDefinition {
    OverlayDefinition {
        id: OverlayId::new(OVERLAY),
        display_name: "Sub alert".to_owned(),
        kind_id: ALERT_KIND.to_owned(),
        enabled: true,
        position: 0,
        config: OverlayConfig::from([
            (
                HEADLINE_KEY.to_owned(),
                Variant::String(MESSAGE_TOKEN.to_owned()),
            ),
            (
                SUBLINE_KEY.to_owned(),
                Variant::String(VIEWER_TOKEN.to_owned()),
            ),
        ]),
        config_schema_version: 1,
        generator_version: 0,
        source_overrides: Vec::new(),
        credential: OverlayCredential::new("2f8b1d0c9a7e6f5b4c3d2e1f0a9b8c7d"),
        created_at: OffsetDateTime::UNIX_EPOCH,
        updated_at: OffsetDateTime::UNIX_EPOCH,
    }
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
    let action = Action {
        id: ActionId::new(),
        name: "announce the chat message".to_owned(),
        group: None,
        queue_id: QueueId::new(),
        enabled: true,
        concurrent: false,
        bypass_pause: false,
        execution_mode: ExecutionMode::Sequential,
        description: None,
        sub_actions: vec![send_step()],
    };
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

fn actions_service() -> Arc<ActionsService> {
    let (action, instances) = feeding_action();
    let listed = vec![action.clone()];
    let linked: HashMap<ActionId, Vec<TriggerInstance>> = HashMap::from([(action.id, instances)]);

    let mut action_repo = MockActionRepo::new();
    action_repo
        .expect_list()
        .returning(move || Ok(listed.clone()));

    let mut trigger_repo = MockTriggerInstanceRepo::new();
    trigger_repo
        .expect_list_for_action()
        .returning(move |id| Ok(linked.get(&id).cloned().unwrap_or_default()));

    Arc::new(ActionsService::new(
        Arc::new(action_repo),
        Arc::new(MockQueueRepo::new()),
        Arc::new(MockHistoryRepo::new()),
        Arc::new(trigger_repo),
        Arc::new(MockSoundboardClipsRepo::new()),
    ))
}

fn wired(with_event_wiring: bool) -> Wired {
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

    let mut settings = MockSettingsRepo::new();
    let configured = root.to_string_lossy().into_owned();
    settings
        .expect_get_string()
        .returning(move |key| match key {
            reserved_keys::SERVER_OVERLAY_ROOT => Ok(Some(configured.clone())),
            _ => Ok(None),
        });

    let service = OverlayServiceHandle::new(
        Arc::new(repo) as Arc<dyn OverlayRepo>,
        Arc::new(settings) as Arc<dyn SettingsRepo>,
        overlay_kinds(),
        EventBus::new(Arc::new(NullEventLogRepo)),
        None,
    );
    let service = if with_event_wiring {
        service.with_event_wiring(actions_service(), sub_actions(), triggers())
    } else {
        service
    };

    Wired {
        _home: home,
        overlay: stored.id.clone(),
        sample_path: root.join(OVERLAY).join(SAMPLE_FILE),
        service,
    }
}

async fn sample_json_content(wired: &Wired) -> serde_json::Map<String, serde_json::Value> {
    wired
        .service
        .materialize(&wired.overlay)
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
        .test_fire(&wired.overlay)
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
    let wired = wired(true);
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
    let wired = wired(true);
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
async fn an_overlay_service_built_without_event_wiring_samples_the_neutral_vocabulary() {
    let wired = wired(false);
    let fired = test_fire_content(&wired).await;

    for (key, token) in [(HEADLINE_KEY, MESSAGE_TOKEN), (SUBLINE_KEY, VIEWER_TOKEN)] {
        assert_ne!(
            text_at(&fired, key),
            token,
            "an unwired overlay previewed {token} as a raw token",
        );
    }
}
