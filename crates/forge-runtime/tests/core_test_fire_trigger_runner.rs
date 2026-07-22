#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use forge_events::{Event, EventPublisher};
use forge_registry::{RunContext, SubActionRegistry, SubActionRunner};
use forge_runtime::sub_action_runners::{CoreGlobalsSetRunner, CoreTestFireTriggerRunner};
use forge_runtime::{
    EventBus, EventSubscription, NullEventLogRepo, QueueScheduler, QueueSchedulerHandle,
    SchedulerCell, SchedulerRequest, spawn_action_engine,
};
use forge_storage::{ActionRepo, DataProvider, GlobalsRepo, TriggerInstanceRepo};
use forge_storage_sqlite::SqliteBackend;
use forge_types::{
    Action, ActionId, ArgStack, EventId, ExecutionMode, Queue, QueueId, SubActionConfig,
    SubActionOutcome, SubActionStep, TriggerInstanceId, Variant,
};

struct NullPublisher;
impl EventPublisher for NullPublisher {
    fn publish(&self, _event: Event) {}
}

struct Harness {
    trigger_instances: Arc<dyn TriggerInstanceRepo>,
    actions: Arc<dyn ActionRepo>,
    globals: Arc<dyn GlobalsRepo>,
    cell: SchedulerCell,
    handle: QueueSchedulerHandle,
    bus: Arc<EventBus>,
}

fn nonblocking(id: QueueId) -> Queue {
    Queue {
        id,
        name: "default".to_string(),
        description: String::new(),
        concurrency: 8,
        paused: false,
    }
}

fn blocking(id: QueueId) -> Queue {
    Queue {
        id,
        name: "serial".to_string(),
        description: String::new(),
        concurrency: 1,
        paused: false,
    }
}

async fn backend() -> Arc<SqliteBackend> {
    Arc::new(
        SqliteBackend::open_with_key(":memory:", [0x5a; 32])
            .await
            .unwrap(),
    )
}

async fn harness(queue: Queue) -> Harness {
    let backend = backend().await;
    backend.queue_repo().save(&queue).await.unwrap();

    let globals: Arc<dyn GlobalsRepo> = Arc::clone(&backend) as Arc<dyn GlobalsRepo>;
    let mut reg = SubActionRegistry::new();
    reg.register(Box::new(CoreGlobalsSetRunner::new(Arc::clone(&globals))))
        .unwrap();

    let bus = EventBus::new(Arc::new(NullEventLogRepo));
    let engine = spawn_action_engine(
        Arc::clone(&bus),
        backend.action_repo(),
        backend.history_repo(),
        Arc::new(reg),
        Arc::new(forge_runtime::ActionCancelRegistry::new()),
    );
    let handle = QueueScheduler::spawn(engine, Arc::clone(&bus), vec![queue]);
    let cell = SchedulerCell::new();
    cell.set(handle.clone());

    Harness {
        trigger_instances: backend.trigger_instance_repo(),
        actions: backend.action_repo(),
        globals,
        cell,
        handle,
        bus,
    }
}

fn action(id: ActionId, queue_id: QueueId, enabled: bool, set_global: &str) -> Action {
    let mut config = BTreeMap::new();
    config.insert("name".to_owned(), Variant::String(set_global.to_owned()));
    config.insert("value".to_owned(), Variant::String("%out_key%".to_owned()));
    Action {
        id,
        name: "bound".to_owned(),
        group: None,
        queue_id,
        enabled,
        concurrent: false,
        bypass_pause: false,
        execution_mode: ExecutionMode::Sequential,
        description: None,
        sub_actions: vec![SubActionStep {
            kind_id: "core.globals.set".to_owned(),
            config,
            enabled: true,
            continue_on_error: false,
            condition: None,
            label: None,
        }],
    }
}

fn fire_cfg(instance_id: &str, override_outputs: BTreeMap<String, Variant>) -> SubActionConfig {
    let mut c = SubActionConfig::new();
    c.insert(
        "trigger_instance_id".to_owned(),
        Variant::String(instance_id.to_owned()),
    );
    c.insert(
        "override_outputs".to_owned(),
        Variant::Object(override_outputs),
    );
    c
}

fn one_output(key: &str, value: &str) -> BTreeMap<String, Variant> {
    let mut m = BTreeMap::new();
    m.insert(key.to_owned(), Variant::String(value.to_owned()));
    m
}

async fn run_outcome(runner: &dyn SubActionRunner, config: &SubActionConfig) -> SubActionOutcome {
    let stack = ArgStack::new();
    let ctx = RunContext::leaf(&stack, 0, EventId::new(), &NullPublisher);
    let (telemetry, _) = runner.execute(config, &ctx).await;
    telemetry.outcome
}

async fn await_action_done(
    sub: &mut EventSubscription,
    action_id: ActionId,
    attempts: usize,
) -> bool {
    let target = action_id.to_string();
    for _ in 0..attempts {
        match tokio::time::timeout(Duration::from_millis(200), sub.recv()).await {
            Ok(Ok(ev))
                if ev.kind == "action.done"
                    && ev.payload.get("action_id").and_then(|v| v.as_str())
                        == Some(target.as_str()) =>
            {
                return true;
            }
            Ok(Ok(_)) => {}
            _ => {}
        }
    }
    false
}

fn fire_runner(h: &Harness) -> CoreTestFireTriggerRunner {
    CoreTestFireTriggerRunner::new(
        Arc::clone(&h.trigger_instances),
        Arc::clone(&h.actions),
        h.cell.clone(),
    )
}

#[tokio::test]
async fn fired_trigger_runs_the_chain_of_its_one_bound_enabled_action() {
    let q_id = QueueId::new();
    let h = harness(nonblocking(q_id)).await;
    let mut sub = h.bus.subscribe();

    let a_id = ActionId::new();
    h.actions
        .save(&action(a_id, q_id, true, "captured"))
        .await
        .unwrap();
    let instance_id = h
        .trigger_instances
        .upsert_default("test.kind", "Test Trigger")
        .await
        .unwrap();
    h.trigger_instances
        .link_action(a_id, instance_id, 0)
        .await
        .unwrap();

    let runner = fire_runner(&h);
    let outcome = run_outcome(
        &runner,
        &fire_cfg(&instance_id.to_string(), one_output("out_key", "ran")),
    )
    .await;

    assert!(matches!(outcome, SubActionOutcome::Success));
    assert!(
        await_action_done(&mut sub, a_id, 20).await,
        "firing the instance must run the bound enabled action's chain to completion"
    );
    h.handle.shutdown();
}

#[tokio::test]
async fn override_outputs_reach_the_fired_actions_execution_context() {
    let q_id = QueueId::new();
    let h = harness(nonblocking(q_id)).await;
    let mut sub = h.bus.subscribe();

    let a_id = ActionId::new();
    h.actions
        .save(&action(a_id, q_id, true, "captured"))
        .await
        .unwrap();
    let instance_id = h
        .trigger_instances
        .upsert_default("test.kind", "Test Trigger")
        .await
        .unwrap();
    h.trigger_instances
        .link_action(a_id, instance_id, 0)
        .await
        .unwrap();

    let runner = fire_runner(&h);
    let cfg = fire_cfg(
        &instance_id.to_string(),
        one_output("out_key", "synthetic-output-marker"),
    );
    let outcome = run_outcome(&runner, &cfg).await;
    assert!(matches!(outcome, SubActionOutcome::Success));

    assert!(
        await_action_done(&mut sub, a_id, 20).await,
        "the bound action must complete before its global is read"
    );
    let captured = h.globals.get("captured").await.unwrap();
    assert!(
        matches!(captured, Some(Variant::String(ref s)) if s == "synthetic-output-marker"),
        "override_outputs must arrive as the fired action's ArgStack, got {captured:?}"
    );
    h.handle.shutdown();
}

#[tokio::test]
async fn unknown_trigger_instance_id_fails() {
    let h = harness(nonblocking(QueueId::new())).await;
    let runner = fire_runner(&h);

    let missing = TriggerInstanceId::new();
    let outcome = run_outcome(&runner, &fire_cfg(&missing.to_string(), BTreeMap::new())).await;

    assert!(
        matches!(&outcome, SubActionOutcome::Failed(m) if m.contains("unknown trigger instance")),
        "an unknown instance id must fail without dispatching, got {outcome:?}"
    );
    h.handle.shutdown();
}

#[tokio::test]
async fn fire_with_empty_scheduler_cell_reports_scheduler_not_ready() {
    let backend = backend().await;
    let trigger_instances = backend.trigger_instance_repo();
    let instance_id = trigger_instances
        .upsert_default("test.kind", "Test Trigger")
        .await
        .unwrap();

    let runner = CoreTestFireTriggerRunner::new(
        trigger_instances,
        backend.action_repo(),
        SchedulerCell::new(),
    );
    let outcome = run_outcome(
        &runner,
        &fire_cfg(&instance_id.to_string(), BTreeMap::new()),
    )
    .await;

    assert!(
        matches!(&outcome, SubActionOutcome::Failed(m) if m.contains("queue scheduler not ready")),
        "an empty scheduler cell must fail with the not-ready message, got {outcome:?}"
    );
}

#[tokio::test]
async fn unparseable_trigger_instance_id_fails() {
    let backend = backend().await;
    let runner = CoreTestFireTriggerRunner::new(
        backend.trigger_instance_repo(),
        backend.action_repo(),
        SchedulerCell::new(),
    );

    let outcome = run_outcome(&runner, &fire_cfg("not-an-instance-id", BTreeMap::new())).await;

    assert!(
        matches!(&outcome, SubActionOutcome::Failed(m) if m.contains("invalid trigger_instance_id")),
        "an unparseable trigger_instance_id must fail before any lookup, got {outcome:?}"
    );
}

#[tokio::test]
async fn instance_with_no_bound_actions_succeeds_and_dispatches_nothing() {
    let q_id = QueueId::new();
    let h = harness(blocking(q_id)).await;
    let mut sub = h.bus.subscribe();

    let trap = ActionId::new();
    h.actions
        .save(&action(trap, q_id, true, "trap_ran"))
        .await
        .unwrap();
    let instance_id = h
        .trigger_instances
        .upsert_default("test.kind", "Test Trigger")
        .await
        .unwrap();

    let runner = fire_runner(&h);
    let outcome = run_outcome(
        &runner,
        &fire_cfg(&instance_id.to_string(), BTreeMap::new()),
    )
    .await;
    assert!(
        matches!(outcome, SubActionOutcome::Success),
        "an instance found with no bound actions must succeed, got {outcome:?}"
    );

    let barrier = ActionId::new();
    h.actions
        .save(&action(barrier, q_id, true, "barrier_ran"))
        .await
        .unwrap();
    h.handle
        .dispatch(SchedulerRequest {
            queue_id: q_id,
            action_id: barrier,
            trigger_event_id: EventId::new(),
            trigger_kind: None,
            initial_args: ArgStack::new(),
            bypass_pause: false,
        })
        .await
        .unwrap();
    assert!(
        await_action_done(&mut sub, barrier, 30).await,
        "barrier must complete so the FIFO queue is fully drained"
    );

    assert!(
        h.globals.get("trap_ran").await.unwrap().is_none(),
        "firing an instance with no bound actions must dispatch nothing"
    );
    h.handle.shutdown();
}

#[tokio::test]
async fn fired_trigger_dispatches_disabled_action_but_engine_skips_it() {
    let q_id = QueueId::new();
    let h = harness(blocking(q_id)).await;
    let mut sub = h.bus.subscribe();

    let disabled = ActionId::new();
    h.actions
        .save(&action(disabled, q_id, false, "disabled_ran"))
        .await
        .unwrap();
    let instance_id = h
        .trigger_instances
        .upsert_default("test.kind", "Test Trigger")
        .await
        .unwrap();
    h.trigger_instances
        .link_action(disabled, instance_id, 0)
        .await
        .unwrap();

    let runner = fire_runner(&h);
    let outcome = run_outcome(
        &runner,
        &fire_cfg(&instance_id.to_string(), one_output("out_key", "x")),
    )
    .await;
    assert!(
        matches!(outcome, SubActionOutcome::Success),
        "the runner dispatches the disabled action and returns Success, got {outcome:?}"
    );

    let barrier = ActionId::new();
    h.actions
        .save(&action(barrier, q_id, true, "barrier_ran"))
        .await
        .unwrap();
    h.handle
        .dispatch(SchedulerRequest {
            queue_id: q_id,
            action_id: barrier,
            trigger_event_id: EventId::new(),
            trigger_kind: None,
            initial_args: ArgStack::new(),
            bypass_pause: false,
        })
        .await
        .unwrap();
    assert!(
        await_action_done(&mut sub, barrier, 30).await,
        "barrier must complete so the disabled action is known to have been processed"
    );

    assert!(
        h.globals.get("disabled_ran").await.unwrap().is_none(),
        "the engine must skip the disabled action: no execution effect"
    );
    assert!(
        h.globals.get("barrier_ran").await.unwrap().is_some(),
        "positive control: the barrier action must have executed"
    );
    h.handle.shutdown();
}
