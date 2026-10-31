#![allow(clippy::unwrap_used)]
mod common;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use forge_events::EventSubscription;
use forge_events::{Event, EventSource};
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_runtime::{
    EventBus, FrozenRuntime, NullEventLogRepo, RuntimeWiring, triggers::register_core_triggers,
};
use forge_storage::DataProvider;
use forge_storage_sqlite::SqliteBackend;
use forge_types::{
    Action, ActionId, ArgStack, ExecutionMode, PermissionRung, Queue, QueueId, SubActionOutcome,
    SubActionStep, SubActionTelemetry, TriggerInstance, TriggerInstanceId, Variant,
};
use serde_json::json;
use tempfile::TempDir;
use time::OffsetDateTime;

async fn make_backend() -> (Arc<SqliteBackend>, TempDir) {
    let (backend, media) = common::sqlite_backend().await;
    (Arc::new(backend), media)
}

fn make_queue(id: QueueId) -> Queue {
    Queue {
        id,
        name: "test".into(),
        description: String::new(),
        concurrency: std::num::NonZeroU32::new(8).unwrap(),
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
            continue_on_error: false,
            condition: None,
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
        cooldown_secs: 0,
        cooldown_global: true,
        permission_rung: PermissionRung::Everyone,
    }
}

async fn runtime_with_core_steps(dp: Arc<dyn DataProvider>, bus: Arc<EventBus>) -> FrozenRuntime {
    let mut wiring = RuntimeWiring::new(common::wiring_parts(dp, bus).await);
    wiring.register_core_sub_actions().unwrap();
    register_core_triggers(wiring.triggers_mut()).unwrap();
    wiring.freeze().await
}

async fn collect_kind(sub: &mut EventSubscription, target: &str) -> Option<Event> {
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            match sub.recv().await {
                Ok(ev) if ev.kind == target => return Some(ev),
                Ok(_) | Err(forge_events::EventsError::LaggingReceiver) => {}
                Err(_) => return None,
            }
        }
    })
    .await
    .ok()
    .flatten()
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
                args_in: ::std::collections::BTreeMap::new(),
                produced: ::std::collections::BTreeMap::new(),
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

#[tokio::test]
async fn trigger_evaluator_applies_effective_config_overrides() {
    let (backend, _media) = make_backend().await;
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
        .link_action(a_id, instance.id)
        .await
        .unwrap();

    let bus = EventBus::new(Arc::new(NullEventLogRepo));
    let mut sub = bus.subscribe();
    let _runtime = runtime_with_core_steps(Arc::clone(&dp), Arc::clone(&bus)).await;

    bus.publish(Event::new(
        EventSource::Server,
        "custom.premium_event",
        json!({}),
    ));

    let done = collect_kind(&mut sub, "action.done").await;
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

#[tokio::test]
async fn sub_action_runner_sees_merged_default_and_override() {
    let (backend, _media) = make_backend().await;
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
            continue_on_error: false,
            condition: None,
            label: None,
        }],
    };

    dp.queue_repo().save(&make_queue(q_id)).await.unwrap();
    dp.action_repo().save(&action).await.unwrap();

    let instance = custom_instance("go");
    dp.trigger_instance_repo().save(&instance).await.unwrap();
    dp.trigger_instance_repo()
        .link_action(a_id, instance.id)
        .await
        .unwrap();

    let bus = EventBus::new(Arc::new(NullEventLogRepo));
    let mut sub = bus.subscribe();
    let mut wiring =
        RuntimeWiring::new(common::wiring_parts(Arc::clone(&dp), Arc::clone(&bus)).await);
    wiring
        .sub_actions_mut()
        .register(Box::new(RecordingRunner {
            captured: Arc::clone(&captured),
        }))
        .unwrap();
    register_core_triggers(wiring.triggers_mut()).unwrap();
    let _runtime = wiring.freeze().await;

    bus.publish(Event::new(EventSource::Server, "custom.go", json!({})));

    let done = collect_kind(&mut sub, "action.done").await;
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

#[tokio::test]
async fn linked_action_executes_via_join_table_only() {
    let (backend, _media) = make_backend().await;
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

    let bus = EventBus::new(Arc::new(NullEventLogRepo));
    let mut sub = bus.subscribe();
    let _runtime = runtime_with_core_steps(Arc::clone(&dp), Arc::clone(&bus)).await;

    bus.publish(Event::new(EventSource::Server, "custom.go", json!({})));

    let fired = drain_no_kind(&mut sub, "action.done", 300).await;
    assert!(
        !fired,
        "action.done must not fire: the join table has no link_action row, \
         so list_for_action returns empty and the evaluator dispatches nothing"
    );
}
