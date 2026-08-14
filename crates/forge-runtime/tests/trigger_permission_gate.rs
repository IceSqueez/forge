#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use forge_events::{Event, EventSource};
use forge_registry::{
    ChatTriggerFamily, EventFilter, FormField, KindPlatformContract, SubActionRegistry,
    TriggerCategory, TriggerKindDescriptor, TriggerRegistry,
};
use forge_runtime::{
    EventBus, EventSubscription, NullEventLogRepo, QueueScheduler, ScriptRegistry,
    spawn_action_engine, spawn_trigger_evaluator, sub_action_runners::register_core_sub_actions,
};
use forge_storage::{DataProvider, GlobalsRepo, SettingsRepo, UserGlobalsRepo};
use forge_storage_sqlite::SqliteBackend;
use forge_types::{
    Action, ActionId, ArgStack, ChatPayload, ChatSegment, ExecutionMode, ModerationMarks,
    PermissionRung, Queue, QueueId, SubActionStep, TriggerConfig, TriggerInstance,
    TriggerInstanceId, UserBadge, Variant,
};
use serde_json::json;

const TEST_KEY: [u8; 32] = [0xab; 32];
const CHAT_COMMAND_KIND: &str = "test.chat.command";
const PLAIN_KIND: &str = "test.plain.ping";

struct FakeDescriptor {
    id: &'static str,
    prefix: &'static str,
    family: Option<ChatTriggerFamily>,
}

impl TriggerKindDescriptor for FakeDescriptor {
    fn id(&self) -> &str {
        self.id
    }
    fn category(&self) -> TriggerCategory {
        TriggerCategory::Chat
    }
    fn label(&self) -> &str {
        "fake"
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
        cfg.insert("phrase".to_owned(), Variant::String("!go".to_owned()));
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
            kind_prefix: Some(self.prefix.to_owned()),
        }
    }
    fn matches_trigger(&self, _: &TriggerConfig, _: &Event) -> bool {
        true
    }
    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let user = event
            .payload
            .get("user")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        ArgStack::new().set("user".to_owned(), Variant::String(user))
    }
    fn chat_trigger_family(&self) -> Option<ChatTriggerFamily> {
        self.family
    }
}

fn fake_registry() -> Arc<TriggerRegistry> {
    let mut registry = TriggerRegistry::new();
    registry
        .register(Box::new(FakeDescriptor {
            id: CHAT_COMMAND_KIND,
            prefix: "test.chat",
            family: Some(ChatTriggerFamily::Command),
        }))
        .unwrap();
    registry
        .register(Box::new(FakeDescriptor {
            id: PLAIN_KIND,
            prefix: "test.plain",
            family: None,
        }))
        .unwrap();
    Arc::new(registry)
}

fn instance(kind_id: &str, rung: PermissionRung, cooldown_secs: u32) -> TriggerInstance {
    TriggerInstance {
        id: TriggerInstanceId::new(),
        kind_id: kind_id.to_owned(),
        name: "gated".to_owned(),
        overrides: BTreeMap::new(),
        enabled: true,
        user_defined: true,
        platform_scope: Default::default(),
        cooldown_secs,
        cooldown_global: true,
        permission_rung: rung,
    }
}

fn log_action(id: ActionId, queue_id: QueueId) -> Action {
    Action {
        id,
        name: "gated-action".to_owned(),
        group: None,
        queue_id,
        enabled: true,
        concurrent: false,
        bypass_pause: false,
        execution_mode: ExecutionMode::Sequential,
        description: None,
        sub_actions: vec![SubActionStep {
            kind_id: "core.log.write".to_owned(),
            config: {
                let mut c = BTreeMap::new();
                c.insert("message".to_owned(), Variant::String("fired".to_owned()));
                c
            },
            enabled: true,
            continue_on_error: false,
            condition: None,
            label: None,
        }],
    }
}

fn chat_event(user: &str, badges: Vec<UserBadge>) -> Event {
    let chat = ChatPayload {
        platform_msg_id: "m-1".to_owned(),
        author: user.to_owned(),
        author_color: None,
        segments: vec![ChatSegment::Text {
            text: "!go".to_owned(),
        }],
        badges,
        is_event: false,
        event_detail: None,
        moderation: ModerationMarks::default(),
    };
    Event::new(
        EventSource::Twitch,
        "test.chat.message",
        json!({
            "user": user,
            (ChatPayload::KEY): serde_json::to_value(&chat).unwrap(),
        }),
    )
}

fn plain_event(user: &str) -> Event {
    Event::new(
        EventSource::Twitch,
        "test.plain.ping",
        json!({ "user": user }),
    )
}

struct Harness {
    bus: Arc<EventBus>,
    sub: EventSubscription,
    _evaluator: forge_runtime::TriggerEvaluatorHandle,
}

/// Wires action, queue and trigger-instance rows into an in-memory backend and starts the
/// evaluator against a registry that owns nothing but the fake chat / non-chat descriptors.
async fn harness(instances: &[(&TriggerInstance, ActionId)]) -> Harness {
    let backend = Arc::new(
        SqliteBackend::open_with_key(":memory:", TEST_KEY)
            .await
            .unwrap(),
    );
    let dp: Arc<dyn DataProvider> = Arc::clone(&backend) as Arc<dyn DataProvider>;

    let queue_id = QueueId::new();
    let queue = Queue {
        id: queue_id,
        name: "gate".into(),
        description: String::new(),
        concurrency: 8,
    };
    dp.queue_repo().save(&queue).await.unwrap();

    let mut saved_actions = Vec::new();
    for (inst, action_id) in instances {
        if !saved_actions.contains(action_id) {
            dp.action_repo()
                .save(&log_action(*action_id, queue_id))
                .await
                .unwrap();
            saved_actions.push(*action_id);
        }
        dp.trigger_instance_repo().save(inst).await.unwrap();
        dp.trigger_instance_repo()
            .link_action(*action_id, inst.id, 0)
            .await
            .unwrap();
    }

    let scripts = Arc::new(ScriptRegistry::new());
    let publisher_bus = EventBus::new(Arc::new(NullEventLogRepo));
    let publisher: Arc<dyn forge_events::EventPublisher> =
        Arc::clone(&publisher_bus) as Arc<dyn forge_events::EventPublisher>;
    let script_repo = {
        let mut m = forge_storage::script::MockScriptRepo::new();
        m.expect_record_execution().returning(|_, _, _, _| Ok(()));
        Arc::new(m) as Arc<dyn forge_storage::ScriptRepo>
    };
    let mut sub_reg = SubActionRegistry::new();
    register_core_sub_actions(
        &mut sub_reg,
        Arc::clone(&dp) as Arc<dyn GlobalsRepo>,
        Arc::clone(&dp) as Arc<dyn UserGlobalsRepo>,
        scripts,
        publisher,
        Arc::clone(&dp) as Arc<dyn SettingsRepo>,
        forge_runtime::SchedulerCell::new(),
        dp.trigger_instance_repo(),
        dp.action_repo(),
        script_repo,
        Arc::new(forge_runtime::ActionCancelRegistry::new()),
        forge_runtime::OverlayServiceCell::new(),
        forge_runtime::Config::default(),
    )
    .unwrap();

    let bus = EventBus::new(Arc::new(NullEventLogRepo));
    let sub = bus.subscribe();
    let engine = spawn_action_engine(
        Arc::clone(&bus),
        dp.action_repo(),
        dp.history_repo(),
        Arc::new(sub_reg),
        Arc::new(forge_runtime::ActionCancelRegistry::new()),
    );
    let scheduler = QueueScheduler::spawn(engine, Arc::clone(&bus), vec![queue]);
    let evaluator = spawn_trigger_evaluator(
        Arc::clone(&bus),
        fake_registry(),
        dp.action_repo(),
        dp.trigger_instance_repo(),
        scheduler,
        forge_runtime::Config::default(),
    );

    tokio::time::sleep(Duration::from_millis(10)).await;
    Harness {
        bus,
        sub,
        _evaluator: evaluator,
    }
}

async fn next_kind(sub: &mut EventSubscription, target: &str) -> Option<Event> {
    for _ in 0..60 {
        match tokio::time::timeout(Duration::from_millis(300), sub.recv()).await {
            Ok(Ok(ev)) if ev.kind == target => return Some(ev),
            Ok(Ok(_)) => {}
            _ => break,
        }
    }
    None
}

async fn drain_kinds(sub: &mut EventSubscription, targets: &[&str], wait_ms: u64) -> Vec<Event> {
    let deadline = tokio::time::Instant::now() + Duration::from_millis(wait_ms);
    let mut seen = Vec::new();
    while tokio::time::Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_millis(50), sub.recv()).await {
            Ok(Ok(ev)) if targets.contains(&ev.kind.as_str()) => seen.push(ev),
            Ok(Ok(_)) => {}
            _ => {}
        }
    }
    seen
}

fn block_reason(event: &Event) -> &str {
    event.payload["reason"].as_str().unwrap_or("")
}

#[tokio::test]
async fn an_unauthorized_invocation_never_consumes_the_cooldown_window() {
    let inst = instance(CHAT_COMMAND_KIND, PermissionRung::Moderator, 30);
    let action_id = ActionId::new();
    let mut h = harness(&[(&inst, action_id)]).await;

    for _ in 0..2 {
        h.bus.publish(chat_event("rando", vec![]));
        let blocked = next_kind(&mut h.sub, "trigger.blocked")
            .await
            .expect("an unauthorized invocation must be recorded as blocked");
        assert_eq!(block_reason(&blocked), "permission");
    }

    h.bus
        .publish(chat_event("mod_user", vec![UserBadge::Moderator]));
    assert!(
        next_kind(&mut h.sub, "action.done").await.is_some(),
        "the permission gate runs first, so denied spam must leave the cooldown window unstamped"
    );
}

#[tokio::test]
async fn two_instances_matching_one_message_each_emit_their_own_command_matched() {
    let first = instance(CHAT_COMMAND_KIND, PermissionRung::Everyone, 0);
    let second = instance(CHAT_COMMAND_KIND, PermissionRung::Everyone, 0);
    let mut h = harness(&[(&first, ActionId::new()), (&second, ActionId::new())]).await;

    h.bus.publish(chat_event("rando", vec![]));
    let matched = drain_kinds(&mut h.sub, &["command.matched"], 400).await;
    assert_eq!(
        matched.len(),
        2,
        "the per-event emission latch is retired: every matching instance reports for itself"
    );
}

#[tokio::test]
async fn one_instance_on_two_actions_records_a_single_refusal_per_message() {
    let inst = instance(CHAT_COMMAND_KIND, PermissionRung::Moderator, 0);
    let mut h = harness(&[(&inst, ActionId::new()), (&inst, ActionId::new())]).await;

    h.bus.publish(chat_event("rando", vec![]));
    let seen = drain_kinds(&mut h.sub, &["command.matched", "trigger.blocked"], 600).await;

    let matched = seen.iter().filter(|e| e.kind == "command.matched").count();
    let blocked: Vec<_> = seen
        .iter()
        .filter(|e| e.kind == "trigger.blocked")
        .collect();

    assert_eq!(
        matched, 1,
        "the decision is per instance, so fanning the instance across actions must not multiply the match record"
    );
    assert_eq!(
        blocked.len(),
        1,
        "one refused chatter on one message is one refusal, however many actions the instance drives"
    );
    assert_eq!(block_reason(blocked[0]), "permission");
}

#[tokio::test]
async fn one_authorized_instance_on_two_actions_matches_once_and_dispatches_twice() {
    let inst = instance(CHAT_COMMAND_KIND, PermissionRung::Everyone, 0);
    let mut h = harness(&[(&inst, ActionId::new()), (&inst, ActionId::new())]).await;

    h.bus.publish(chat_event("rando", vec![]));
    let seen = drain_kinds(&mut h.sub, &["command.matched", "action.done"], 800).await;

    assert_eq!(
        seen.iter().filter(|e| e.kind == "command.matched").count(),
        1,
        "collapsing the duplicate decision must not depend on the gate outcome"
    );
    assert_eq!(
        seen.iter().filter(|e| e.kind == "action.done").count(),
        2,
        "every linked action still runs: deduplication covers the decision, not the dispatch"
    );
}

#[tokio::test]
async fn two_instances_matching_one_message_get_independent_gate_outcomes() {
    let open = instance(CHAT_COMMAND_KIND, PermissionRung::Everyone, 0);
    let gated = instance(CHAT_COMMAND_KIND, PermissionRung::Moderator, 0);
    let gated_id = gated.id;
    let mut h = harness(&[(&open, ActionId::new()), (&gated, ActionId::new())]).await;

    h.bus.publish(chat_event("rando", vec![]));
    let seen = drain_kinds(&mut h.sub, &["trigger.blocked", "action.done"], 600).await;

    let blocked: Vec<_> = seen
        .iter()
        .filter(|e| e.kind == "trigger.blocked")
        .collect();
    assert_eq!(blocked.len(), 1, "only the gated instance may be refused");
    assert_eq!(
        blocked[0].payload["instance_id"],
        json!(gated_id),
        "the refusal must be attributed to the instance that produced it"
    );
    assert!(
        seen.iter().any(|e| e.kind == "action.done"),
        "the open instance must still dispatch on the same message"
    );
}

#[tokio::test]
async fn a_missing_or_malformed_chat_envelope_resolves_to_the_floor_rung() {
    let inst = instance(CHAT_COMMAND_KIND, PermissionRung::Subscriber, 0);
    let mut h = harness(&[(&inst, ActionId::new())]).await;

    for payload in [
        json!({ "user": "rando" }),
        json!({ "user": "rando", (ChatPayload::KEY): 42 }),
        json!({ "user": "rando", (ChatPayload::KEY): { "badges": ["moderator"] } }),
    ] {
        let event = Event::new(EventSource::Twitch, "test.chat.message", payload.clone());
        let cause = event.id;
        h.bus.publish(event);

        let blocked = next_kind(&mut h.sub, "trigger.blocked")
            .await
            .unwrap_or_else(|| panic!("an unusable role signal must gate: {payload}"));
        assert_eq!(block_reason(&blocked), "permission");
        assert_eq!(blocked.payload["rung_resolved"], json!("everyone"));
        assert_eq!(blocked.payload["rung_required"], json!("subscriber"));
        assert_eq!(blocked.caused_by, Some(cause));
    }
}

#[tokio::test]
async fn a_non_chat_trigger_kind_ignores_the_permission_rung() {
    let inst = instance(PLAIN_KIND, PermissionRung::Broadcaster, 0);
    let mut h = harness(&[(&inst, ActionId::new())]).await;

    h.bus.publish(plain_event("rando"));
    assert!(
        next_kind(&mut h.sub, "action.done").await.is_some(),
        "a kind with no chatter role signal must not be permission-gated"
    );
}

#[tokio::test]
async fn a_non_chat_trigger_kind_is_still_cooldown_gated() {
    let inst = instance(PLAIN_KIND, PermissionRung::Everyone, 30);
    let mut h = harness(&[(&inst, ActionId::new())]).await;

    h.bus.publish(plain_event("rando"));
    assert!(next_kind(&mut h.sub, "action.done").await.is_some());

    h.bus.publish(plain_event("rando"));
    let blocked = next_kind(&mut h.sub, "trigger.blocked")
        .await
        .expect("the second invocation inside the window must be throttled");
    assert_eq!(block_reason(&blocked), "cooldown");
}

#[tokio::test]
async fn the_broadcaster_passes_permission_but_is_still_cooldown_gated() {
    let inst = instance(CHAT_COMMAND_KIND, PermissionRung::Broadcaster, 30);
    let mut h = harness(&[(&inst, ActionId::new())]).await;

    h.bus
        .publish(chat_event("owner", vec![UserBadge::Broadcaster]));
    assert!(
        next_kind(&mut h.sub, "action.done").await.is_some(),
        "the top rung passes the permission gate by definition"
    );

    let second = chat_event("owner", vec![UserBadge::Broadcaster]);
    let cause = second.id;
    h.bus.publish(second);
    let blocked = next_kind(&mut h.sub, "trigger.blocked")
        .await
        .expect("forge ships no broadcaster cooldown exemption");
    assert_eq!(block_reason(&blocked), "cooldown");
    assert!(
        blocked.payload["remaining_ms"]
            .as_u64()
            .is_some_and(|ms| ms > 0),
        "a cooldown refusal must report the remaining window"
    );
    assert!(
        blocked.payload.get("rung_required").is_none()
            && blocked.payload.get("rung_resolved").is_none(),
        "a cooldown refusal makes no rung assertion"
    );
    assert_eq!(blocked.caused_by, Some(cause));
}
