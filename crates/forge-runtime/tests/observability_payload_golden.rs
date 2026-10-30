#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use forge_events::{Event, EventPublisher, EventSource, EventSubscription, EventsError};
use forge_registry::{
    ChatTriggerFamily, EventFilter, FormField, KindPlatformContract, SubActionRegistry,
    TriggerCategory, TriggerKindDescriptor, TriggerRegistry,
};
use forge_runtime::triggers::TimerSchedule;
use forge_runtime::{
    ActionCancelRegistry, Catalog, Config, CoreSubActionDeps, EventBus, NullEventLogRepo,
    OverlayServiceCell, QueueMode, QueueScheduler, QueueSchedulerHandle, SchedulerCell,
    ScriptRegistry, TriggerEvaluatorHandle, spawn_action_engine, spawn_trigger_evaluator,
    sub_action_runners::register_core_sub_actions,
};
use forge_storage::{DataProvider, GlobalsRepo, SettingsRepo, UserGlobalsRepo};
use forge_storage_sqlite::SqliteBackend;
use forge_types::{
    Action, ArgStack, ChatPayload, ChatSegment, ExecutionMode, ModerationMarks, PermissionRung,
    Queue, QueueId, SubActionConfig, SubActionStep, TriggerConfig, TriggerInstance,
    TriggerInstanceId, Variant,
};
use serde_json::{Value, json};
use tempfile::TempDir;

const TEST_KEY: [u8; 32] = [0xab; 32];
const HANG_GUARD: Duration = Duration::from_secs(30);
const COMMAND_DESCRIPTOR_ID: &str = "golden.chat.command";
const COMMAND_EVENT_PREFIX: &str = "golden.chat";
const COMMAND_EVENT_KIND: &str = "golden.chat.message";
const ACTION_ID: &str = "01JBZ8Q4T5N7W3K2M9X6V1R0AA";
const QUEUE_ID: &str = "01JBZ8Q4T5N7W3K2M9X6V1R0QQ";
const TRIGGER_ID: &str = "01JBZ8Q4T5N7W3K2M9X6V1R0TT";
const VOLATILE_DURATION_KEYS: &[&str] = &["duration_ms", "total_ms"];

const ACTION_RUN_KINDS: &[&str] = &[
    "command.matched",
    "action.start",
    "subaction.run",
    "subaction.done",
    "global.set",
    "global.incremented",
    "script.exec",
    "script.log",
    "script.error",
    "action.done",
];

const QUEUE_CONTROL_KINDS: &[&str] = &[
    "queue.paused",
    "queue.held",
    "queue.draining",
    "queue.resumed",
    "queue.cleared",
];

struct CommandDescriptor;

impl TriggerKindDescriptor for CommandDescriptor {
    fn id(&self) -> &str {
        COMMAND_DESCRIPTOR_ID
    }
    fn category(&self) -> TriggerCategory {
        TriggerCategory::Chat
    }
    fn label(&self) -> &str {
        "golden command"
    }
    fn summary(&self) -> &str {
        ""
    }
    fn search_text(&self) -> &str {
        ""
    }
    fn icon_name(&self) -> &str {
        ""
    }
    fn platform_contract(&self) -> KindPlatformContract {
        KindPlatformContract::Universal
    }
    fn default_config(&self) -> TriggerConfig {
        let mut cfg = TriggerConfig::new();
        cfg.insert("phrase".to_owned(), Variant::String("!golden".to_owned()));
        cfg
    }
    fn config_fields(&self) -> Vec<FormField> {
        Vec::new()
    }
    fn condition_display(&self, _: &TriggerConfig) -> String {
        String::new()
    }
    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Twitch),
            kind_prefix: Some(COMMAND_EVENT_PREFIX.to_owned()),
        }
    }
    fn matches_trigger(&self, _: &TriggerConfig, _: &Event) -> bool {
        true
    }
    fn build_arg_stack(&self, _: &Event) -> ArgStack {
        ArgStack::new()
    }
    fn chat_trigger_family(&self) -> Option<ChatTriggerFamily> {
        Some(ChatTriggerFamily::Command)
    }
}

fn config(entries: &[(&str, Variant)]) -> SubActionConfig {
    entries
        .iter()
        .map(|(key, value)| ((*key).to_owned(), value.clone()))
        .collect()
}

fn step(kind_id: &str, entries: &[(&str, Variant)]) -> SubActionStep {
    SubActionStep {
        kind_id: kind_id.to_owned(),
        config: config(entries),
        enabled: true,
        continue_on_error: false,
        condition: None,
        label: None,
    }
}

fn text(value: &str) -> Variant {
    Variant::String(value.to_owned())
}

fn golden_action(queue_id: QueueId) -> Action {
    Action {
        id: ACTION_ID.parse().unwrap(),
        name: "Golden run".to_owned(),
        group: None,
        queue_id,
        enabled: true,
        concurrent: false,
        bypass_pause: false,
        execution_mode: ExecutionMode::Sequential,
        description: None,
        sub_actions: vec![
            step(
                "core.globals.set",
                &[("name", text("golden_set")), ("value", text("42"))],
            ),
            step(
                "core.globals.increment",
                &[("name", text("golden_set")), ("amount", Variant::Int(3))],
            ),
            step(
                "script.run.inline",
                &[("body", text(r#"forge::log("golden line")"#))],
            ),
            step(
                "script.run.inline",
                &[("body", text(r#"throw "golden boom""#))],
            ),
        ],
    }
}

fn command_event() -> Event {
    let chat = ChatPayload {
        platform_msg_id: "golden-msg".to_owned(),
        author: "viewer".to_owned(),
        author_color: None,
        segments: vec![ChatSegment::Text {
            text: "!golden".to_owned(),
        }],
        badges: Vec::new(),
        is_event: false,
        event_detail: None,
        moderation: ModerationMarks::default(),
    };
    Event::new(
        EventSource::Twitch,
        COMMAND_EVENT_KIND,
        json!({ (ChatPayload::KEY): serde_json::to_value(&chat).unwrap() }),
    )
}

struct Rig {
    bus: Arc<EventBus>,
    sub: EventSubscription,
    scheduler: QueueSchedulerHandle,
    queue_id: QueueId,
    _evaluator: TriggerEvaluatorHandle,
    _media: TempDir,
}

async fn rig() -> Rig {
    let media = tempfile::tempdir().unwrap();
    let backend =
        SqliteBackend::open_for_test(":memory:", TEST_KEY, media.path().join("media"), None)
            .await
            .unwrap();
    let dp: Arc<dyn DataProvider> = Arc::new(backend);

    let queue_id: QueueId = QUEUE_ID.parse().unwrap();
    let queue = Queue {
        id: queue_id,
        name: "golden".to_owned(),
        description: String::new(),
        concurrency: 1,
    };
    dp.queue_repo().save(&queue).await.unwrap();
    let action = golden_action(queue_id);
    dp.action_repo().save(&action).await.unwrap();
    let instance = TriggerInstance {
        id: TRIGGER_ID.parse::<TriggerInstanceId>().unwrap(),
        kind_id: COMMAND_DESCRIPTOR_ID.to_owned(),
        name: "golden".to_owned(),
        overrides: BTreeMap::new(),
        enabled: true,
        user_defined: true,
        platform_scope: Default::default(),
        cooldown_secs: 0,
        cooldown_global: true,
        permission_rung: PermissionRung::Everyone,
    };
    dp.trigger_instance_repo().save(&instance).await.unwrap();
    dp.trigger_instance_repo()
        .link_action(action.id, instance.id)
        .await
        .unwrap();

    SettingsRepo::set_string(
        dp.as_ref(),
        forge_storage::reserved_keys::SCRIPT_TIMEOUT_MS,
        "10000",
    )
    .await
    .unwrap();
    let bus = EventBus::new(Arc::new(NullEventLogRepo));
    let sub = bus.subscribe();
    let publisher: Arc<dyn EventPublisher> = Arc::clone(&bus) as Arc<dyn EventPublisher>;
    let script_repo = {
        let mut m = forge_storage::script::MockScriptRepo::new();
        m.expect_record_execution().returning(|_, _, _, _| Ok(()));
        Arc::new(m) as Arc<dyn forge_storage::ScriptRepo>
    };
    let mut sub_actions = SubActionRegistry::new();
    register_core_sub_actions(
        &mut sub_actions,
        CoreSubActionDeps {
            globals: Arc::clone(&dp) as Arc<dyn GlobalsRepo>,
            user_globals: Arc::clone(&dp) as Arc<dyn UserGlobalsRepo>,
            scripts: Arc::new(ScriptRegistry::new()),
            publisher,
            settings: Arc::clone(&dp) as Arc<dyn SettingsRepo>,
            scheduler: SchedulerCell::new(),
            trigger_instances: dp.trigger_instance_repo(),
            actions: dp.action_repo(),
            script_repo,
            cancel_registry: Arc::new(ActionCancelRegistry::new()),
            overlays: OverlayServiceCell::new(),
            config: Config::default(),
        },
    )
    .unwrap();

    let catalog = || {
        Catalog::new(
            dp.action_repo(),
            dp.trigger_instance_repo(),
            dp.catalog_revision(),
        )
    };
    let engine = spawn_action_engine(
        Arc::clone(&bus),
        catalog(),
        dp.action_repo(),
        dp.history_repo(),
        Arc::new(sub_actions),
        Arc::new(ActionCancelRegistry::new()),
    );
    let scheduler = QueueScheduler::spawn(engine, Arc::clone(&bus), vec![queue]);
    let mut triggers = TriggerRegistry::new();
    triggers.register(Box::new(CommandDescriptor)).unwrap();
    let evaluator = spawn_trigger_evaluator(
        Arc::clone(&bus),
        Arc::new(triggers),
        catalog(),
        scheduler.clone(),
        Config::default(),
    );

    Rig {
        bus,
        sub,
        scheduler,
        queue_id,
        _evaluator: evaluator,
        _media: media,
    }
}

async fn collect_until(
    sub: &mut EventSubscription,
    kinds: &[&str],
    settled: impl Fn(&[Event]) -> bool,
) -> Vec<Event> {
    let mut seen = Vec::new();
    tokio::time::timeout(HANG_GUARD, async {
        while !settled(&seen) {
            match sub.recv().await {
                Ok(ev) if kinds.contains(&ev.kind.as_str()) => seen.push(ev),
                Ok(_) | Err(EventsError::LaggingReceiver) => {}
                Err(e) => panic!("bus failed: {e:?}"),
            }
        }
    })
    .await
    .expect("the producers never emitted every pinned kind");
    seen
}

fn without_durations(mut payload: Value) -> Value {
    if let Some(fields) = payload.as_object_mut() {
        for key in VOLATILE_DURATION_KEYS {
            if let Some(value) = fields.get_mut(*key) {
                assert!(value.is_u64(), "{key} must stay an unsigned integer");
                *value = json!(0);
            }
        }
    }
    payload
}

fn with_keys_sorted(value: Value) -> Value {
    match value {
        Value::Object(fields) => {
            let sorted: BTreeMap<String, Value> = fields
                .into_iter()
                .map(|(key, value)| (key, with_keys_sorted(value)))
                .collect();
            Value::Object(sorted.into_iter().collect())
        }
        Value::Array(items) => Value::Array(items.into_iter().map(with_keys_sorted).collect()),
        other => other,
    }
}

fn rendered(events: &[Event], kind: &str) -> String {
    let emitted: Vec<Value> = events
        .iter()
        .filter(|event| event.kind == kind)
        .map(|event| {
            json!({
                "source": event.source,
                "payload": without_durations(event.payload.clone()),
            })
        })
        .collect();
    format!(
        "{}\n",
        serde_json::to_string_pretty(&with_keys_sorted(Value::Array(emitted))).unwrap()
    )
}

fn golden_path(kind: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/observability")
        .join(format!("{kind}.json"))
}

fn drift(events: &[Event], kinds: &[&str]) -> Vec<String> {
    kinds
        .iter()
        .filter_map(|kind| {
            let actual = rendered(events, kind);
            let golden = std::fs::read_to_string(golden_path(kind)).unwrap_or_default();
            (golden != actual)
                .then(|| format!("{kind}:\n--- golden\n{golden}--- emitted\n{actual}"))
        })
        .collect()
}

#[tokio::test]
async fn action_run_observability_payloads_match_the_per_kind_goldens() {
    let mut rig = rig().await;

    rig.bus.publish(command_event());
    let events = collect_until(&mut rig.sub, ACTION_RUN_KINDS, |seen| {
        seen.iter().any(|event| event.kind == "action.done")
    })
    .await;

    let drifted = drift(&events, ACTION_RUN_KINDS);
    assert!(
        drifted.is_empty(),
        "payloads drifted:\n{}",
        drifted.join("\n")
    );
}

#[tokio::test]
async fn queue_control_payloads_match_the_per_kind_goldens() {
    let mut rig = rig().await;

    for mode in [
        QueueMode::PAUSED,
        QueueMode::HOLDING,
        QueueMode::DRAINING,
        QueueMode::RUNNING,
    ] {
        rig.scheduler.set_mode(rig.queue_id, mode).await.unwrap();
    }
    rig.scheduler.clear(rig.queue_id, true).await.unwrap();
    let events = collect_until(&mut rig.sub, QUEUE_CONTROL_KINDS, |seen| {
        let kinds: BTreeSet<&str> = seen.iter().map(|event| event.kind.as_str()).collect();
        QUEUE_CONTROL_KINDS.iter().all(|kind| kinds.contains(kind))
    })
    .await;

    let drifted = drift(&events, QUEUE_CONTROL_KINDS);
    assert!(
        drifted.is_empty(),
        "payloads drifted:\n{}",
        drifted.join("\n")
    );
}

#[test]
fn timer_tick_payload_matches_the_golden() {
    let mut overrides = TriggerConfig::new();
    overrides.insert("interval_minutes".to_owned(), Variant::Int(15));
    let schedule = TimerSchedule::from_config(&overrides);

    let tick = schedule.tick_event(TRIGGER_ID.parse().unwrap(), 7);

    let drifted = drift(&[tick], &["timer.tick"]);
    assert!(
        drifted.is_empty(),
        "payloads drifted:\n{}",
        drifted.join("\n")
    );
}
