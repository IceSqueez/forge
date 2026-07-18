//! End-to-end smoke tests for the Template/Patch trigger flow.
//!
//! Covered invariants:
//! - `effective_config` merges descriptor defaults with instance `overrides` before
//!   `matches_trigger`, making the override visible end-to-end.
//! - Sub-action execution receives the fully-merged config: override wins for keys
//!   that are present; `runner.default_config()` fills the gaps.
//! - `action_trigger_instances` join table is the sole wiring path; an action that
//!   has no `link_action` entry never fires even when a matching instance exists.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use forge_events::{Event, EventSource};
use forge_registry::{
    FormField, RegistryError, RunContext, SubActionCategory, SubActionRegistry, SubActionRunner,
    TriggerRegistry,
};
use forge_runtime::{
    EventBus, EventSubscription, NullEventLogRepo, QueueScheduler, ScriptRegistry,
    spawn_action_engine, spawn_trigger_evaluator, sub_action_runners::register_core_sub_actions,
    triggers::register_core_triggers,
};
use forge_storage::{
    ActionRepo, DataProvider, GlobalsRepo, SettingsRepo, TriggerInstanceRepo, UserGlobalsRepo,
};
use forge_storage_sqlite::SqliteBackend;
use forge_types::{
    Action, ActionId, ArgStack, ExecutionMode, Queue, QueueId, SubActionOutcome, SubActionStep,
    SubActionTelemetry, TriggerInstance, TriggerInstanceId, Variant,
};
use serde_json::json;
use time::OffsetDateTime;

const TEST_KEY: [u8; 32] = [0xab; 32];

async fn make_backend() -> Arc<SqliteBackend> {
    Arc::new(
        SqliteBackend::open_with_key(":memory:", TEST_KEY)
            .await
            .unwrap(),
    )
}

fn make_queue(id: QueueId) -> Queue {
    Queue {
        id,
        name: "test".into(),
        blocking: false,
    }
}

fn log_action(id: ActionId, queue_id: QueueId) -> Action {
    Action {
        id,
        name: "e2e-log-action".to_string(),
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
                c.insert("message".to_owned(), Variant::String("e2e-ok".to_owned()));
                c
            },
            enabled: true,
            label: None,
        }],
    }
}

fn custom_instance(event_name: &str) -> TriggerInstance {
    let mut overrides = BTreeMap::new();
    overrides.insert(
        "event_name".to_owned(),
        Variant::String(event_name.to_owned()),
    );
    TriggerInstance {
        id: TriggerInstanceId::new(),
        kind_id: "script.event.custom".to_owned(),
        name: event_name.to_owned(),
        overrides,
        enabled: true,
        user_defined: true,
        platform_scope: Default::default(),
        global_cooldown_secs: 0,
        user_cooldown_secs: 0,
    }
}

fn build_core_registries(
    globals: Arc<dyn GlobalsRepo>,
    user_globals: Arc<dyn UserGlobalsRepo>,
    settings: Arc<dyn SettingsRepo>,
    trigger_instances: Arc<dyn TriggerInstanceRepo>,
    actions: Arc<dyn ActionRepo>,
) -> (Arc<SubActionRegistry>, Arc<TriggerRegistry>) {
    let scripts = Arc::new(ScriptRegistry::new());
    let bus = EventBus::new(Arc::new(NullEventLogRepo));
    let publisher: Arc<dyn forge_events::EventPublisher> =
        Arc::clone(&bus) as Arc<dyn forge_events::EventPublisher>;

    let mut sub_reg = SubActionRegistry::new();
    register_core_sub_actions(
        &mut sub_reg,
        globals,
        user_globals,
        scripts,
        publisher,
        settings,
        forge_runtime::SchedulerCell::new(),
        trigger_instances,
        actions,
        Arc::new(forge_runtime::ActionCancelRegistry::new()),
        forge_runtime::Config::default(),
    )
    .unwrap();

    let mut trig_reg = TriggerRegistry::new();
    register_core_triggers(&mut trig_reg).unwrap();

    (Arc::new(sub_reg), Arc::new(trig_reg))
}

async fn collect_kind(sub: &mut EventSubscription, target: &str, attempts: usize) -> Option<Event> {
    for _ in 0..attempts {
        match tokio::time::timeout(Duration::from_millis(300), sub.recv()).await {
            Ok(Ok(ev)) if ev.kind == target => return Some(ev),
            Ok(Ok(_)) => {}
            _ => break,
        }
    }
    None
}

async fn drain_no_kind(sub: &mut EventSubscription, forbidden: &str, wait_ms: u64) -> bool {
    let deadline = tokio::time::Instant::now() + Duration::from_millis(wait_ms);
    while tokio::time::Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_millis(50), sub.recv()).await {
            Ok(Ok(ev)) if ev.kind == forbidden => return true,
            Ok(Ok(_)) => {}
            _ => {}
        }
    }
    false
}

struct RecordingRunner {
    captured: Arc<Mutex<Option<BTreeMap<String, Variant>>>>,
}

#[async_trait]
impl SubActionRunner for RecordingRunner {
    fn id(&self) -> &str {
        "test.record"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Util
    }

    fn label(&self) -> &str {
        "Recording"
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

    fn default_config(&self) -> forge_registry::SubActionConfig {
        let mut c = BTreeMap::new();
        c.insert("a".to_owned(), Variant::Int(1));
        c.insert("b".to_owned(), Variant::Int(2));
        c
    }

    fn config_fields(&self) -> Vec<FormField> {
        Vec::new()
    }

    fn validate_config(&self, _: &forge_registry::SubActionConfig) -> Result<(), RegistryError> {
        Ok(())
    }

    async fn execute(
        &self,
        config: &forge_registry::SubActionConfig,
        _ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        *self.captured.lock().unwrap() = Some(config.clone());
        (
            SubActionTelemetry {
                index: 0,
                kind: "test.record".to_owned(),
                started_at: OffsetDateTime::now_utc(),
                duration_ms: 0,
                outcome: SubActionOutcome::Success,
            },
            None,
        )
    }
}

/// A `TriggerInstance` with `overrides` that changes `event_name` must cause
/// `effective_config` to route the event correctly: the matching name fires,
/// a different name does not.
#[tokio::test]
async fn trigger_evaluator_applies_effective_config_overrides() {
    let backend = make_backend().await;
    let dp: Arc<dyn DataProvider> = Arc::clone(&backend) as Arc<dyn DataProvider>;

    let q_id = QueueId::new();
    let a_id = ActionId::new();

    dp.queue_repo().save(&make_queue(q_id)).await.unwrap();
    dp.action_repo()
        .save(&log_action(a_id, q_id))
        .await
        .unwrap();

    let instance = custom_instance("premium_event");
    dp.trigger_instance_repo().save(&instance).await.unwrap();
    dp.trigger_instance_repo()
        .link_action(a_id, instance.id, 0)
        .await
        .unwrap();

    let (sub_reg, trig_reg) = build_core_registries(
        Arc::clone(&dp) as Arc<dyn GlobalsRepo>,
        Arc::clone(&dp) as Arc<dyn UserGlobalsRepo>,
        Arc::clone(&dp) as Arc<dyn SettingsRepo>,
        dp.trigger_instance_repo(),
        dp.action_repo(),
    );

    let bus = EventBus::new(Arc::new(NullEventLogRepo));
    let mut sub = bus.subscribe();

    let engine = spawn_action_engine(
        Arc::clone(&bus),
        dp.action_repo(),
        dp.history_repo(),
        sub_reg,
        Arc::new(forge_runtime::ActionCancelRegistry::new()),
    );
    let sched = QueueScheduler::spawn(engine, Arc::clone(&bus), vec![make_queue(q_id)]);
    let _eval = spawn_trigger_evaluator(
        Arc::clone(&bus),
        trig_reg,
        dp.action_repo(),
        dp.trigger_instance_repo(),
        sched,
    );

    tokio::time::sleep(Duration::from_millis(10)).await;

    bus.publish(Event::new(
        EventSource::Server,
        "custom.premium_event",
        json!({}),
    ));

    let done = collect_kind(&mut sub, "action.done", 30).await;
    assert!(
        done.is_some(),
        "action.done must fire: effective_config must carry the premium_event override \
         so matches_trigger returns true"
    );

    tokio::time::sleep(Duration::from_millis(50)).await;

    bus.publish(Event::new(
        EventSource::Server,
        "custom.basic_event",
        json!({}),
    ));

    let fired = drain_no_kind(&mut sub, "action.done", 300).await;
    assert!(
        !fired,
        "action.done must not fire for basic_event: effective_config holds \
         event_name=premium_event so matches_trigger must return false for basic_event"
    );
}

/// A sub-action step with a partial `config` override must receive the full
/// merged config: the override value wins and the runner's default fills the rest.
#[tokio::test]
async fn sub_action_runner_sees_merged_default_and_override() {
    let backend = make_backend().await;
    let dp: Arc<dyn DataProvider> = Arc::clone(&backend) as Arc<dyn DataProvider>;

    let q_id = QueueId::new();
    let a_id = ActionId::new();
    let captured: Arc<Mutex<Option<BTreeMap<String, Variant>>>> = Arc::new(Mutex::new(None));

    let action = Action {
        id: a_id,
        name: "recording-action".to_string(),
        group: None,
        queue_id: q_id,
        enabled: true,
        concurrent: false,
        bypass_pause: false,
        execution_mode: ExecutionMode::Sequential,
        description: None,
        sub_actions: vec![SubActionStep {
            kind_id: "test.record".to_owned(),
            config: {
                let mut c = BTreeMap::new();
                c.insert("a".to_owned(), Variant::Int(99));
                c
            },
            enabled: true,
            label: None,
        }],
    };

    dp.queue_repo().save(&make_queue(q_id)).await.unwrap();
    dp.action_repo().save(&action).await.unwrap();

    let instance = custom_instance("go");
    dp.trigger_instance_repo().save(&instance).await.unwrap();
    dp.trigger_instance_repo()
        .link_action(a_id, instance.id, 0)
        .await
        .unwrap();

    let mut sub_reg = SubActionRegistry::new();
    sub_reg
        .register(Box::new(RecordingRunner {
            captured: Arc::clone(&captured),
        }))
        .unwrap();

    let mut trig_reg = TriggerRegistry::new();
    register_core_triggers(&mut trig_reg).unwrap();

    let bus = EventBus::new(Arc::new(NullEventLogRepo));
    let mut sub = bus.subscribe();

    let engine = spawn_action_engine(
        Arc::clone(&bus),
        dp.action_repo(),
        dp.history_repo(),
        Arc::new(sub_reg),
        Arc::new(forge_runtime::ActionCancelRegistry::new()),
    );
    let sched = QueueScheduler::spawn(engine, Arc::clone(&bus), vec![make_queue(q_id)]);
    let _eval = spawn_trigger_evaluator(
        Arc::clone(&bus),
        Arc::new(trig_reg),
        dp.action_repo(),
        dp.trigger_instance_repo(),
        sched,
    );

    tokio::time::sleep(Duration::from_millis(10)).await;

    bus.publish(Event::new(EventSource::Server, "custom.go", json!({})));

    let done = collect_kind(&mut sub, "action.done", 30).await;
    assert!(
        done.is_some(),
        "action.done must fire when custom.go is published and the instance is linked"
    );

    let config = captured
        .lock()
        .unwrap()
        .clone()
        .expect("RecordingRunner must have been called");

    assert_eq!(
        config.get("a"),
        Some(&Variant::Int(99)),
        "key 'a' must reflect the SubActionStep override (99), not the runner default (1)"
    );
    assert_eq!(
        config.get("b"),
        Some(&Variant::Int(2)),
        "key 'b' must be inherited from RecordingRunner::default_config because it is absent from the step config"
    );
}

/// An action that has no `link_action` row must never be dispatched, even when a
/// matching `TriggerInstance` exists in the DB.  The join table is the sole wiring path.
#[tokio::test]
async fn linked_action_executes_via_join_table_only() {
    let backend = make_backend().await;
    let dp: Arc<dyn DataProvider> = Arc::clone(&backend) as Arc<dyn DataProvider>;

    let q_id = QueueId::new();
    let a_id = ActionId::new();

    dp.queue_repo().save(&make_queue(q_id)).await.unwrap();
    dp.action_repo()
        .save(&log_action(a_id, q_id))
        .await
        .unwrap();

    let instance = custom_instance("go");
    dp.trigger_instance_repo().save(&instance).await.unwrap();
    // link_action intentionally NOT called - join table has no row.

    let (sub_reg, trig_reg) = build_core_registries(
        Arc::clone(&dp) as Arc<dyn GlobalsRepo>,
        Arc::clone(&dp) as Arc<dyn UserGlobalsRepo>,
        Arc::clone(&dp) as Arc<dyn SettingsRepo>,
        dp.trigger_instance_repo(),
        dp.action_repo(),
    );

    let bus = EventBus::new(Arc::new(NullEventLogRepo));
    let mut sub = bus.subscribe();

    let engine = spawn_action_engine(
        Arc::clone(&bus),
        dp.action_repo(),
        dp.history_repo(),
        sub_reg,
        Arc::new(forge_runtime::ActionCancelRegistry::new()),
    );
    let sched = QueueScheduler::spawn(engine, Arc::clone(&bus), vec![make_queue(q_id)]);
    let _eval = spawn_trigger_evaluator(
        Arc::clone(&bus),
        trig_reg,
        dp.action_repo(),
        dp.trigger_instance_repo(),
        sched,
    );

    tokio::time::sleep(Duration::from_millis(10)).await;

    bus.publish(Event::new(EventSource::Server, "custom.go", json!({})));

    let fired = drain_no_kind(&mut sub, "action.done", 300).await;
    assert!(
        !fired,
        "action.done must not fire: the join table has no link_action row, \
         so list_for_action returns empty and the evaluator dispatches nothing"
    );
}
