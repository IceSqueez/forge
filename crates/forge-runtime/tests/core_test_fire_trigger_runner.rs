#![expect(clippy::unwrap_used, reason = "test fixtures")]
mod common;
use common::{code_of, failure_message, verbatim_arg};
use forge_events::test_support::NullPublisher;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use forge_events::EventSubscription;
use forge_registry::{ConfigProblem, ConfigReason, RegistryError, RunContext, SubActionRunner};
use forge_runtime::{
    EventBus, FrozenRuntime, NullEventLogRepo, QueueAdmission, QueueSchedulerHandle,
    SchedulerRequest,
};
use forge_storage::{ActionRepo, DataProvider, GlobalsRepo, TriggerInstanceRepo};
use forge_storage_sqlite::SqliteBackend;
use forge_types::{
    Action, ActionId, ArgStack, EventId, Queue, QueueId, SubActionConfig, SubActionOutcome,
    SubActionStep, TriggerInstanceId, Variant,
};
use tempfile::TempDir;

struct Harness {
    trigger_instances: Arc<dyn TriggerInstanceRepo>,
    actions: Arc<dyn ActionRepo>,
    globals: Arc<dyn GlobalsRepo>,
    runtime: FrozenRuntime,
    handle: QueueSchedulerHandle,
    bus: Arc<EventBus>,
    _media: TempDir,
}

fn nonblocking(id: QueueId) -> Queue {
    Queue {
        id,
        name: "default".to_string(),
        description: String::new(),
        concurrency: std::num::NonZeroU32::new(8).unwrap(),
    }
}

fn blocking(id: QueueId) -> Queue {
    Queue {
        id,
        name: "serial".to_string(),
        description: String::new(),
        concurrency: std::num::NonZeroU32::MIN,
    }
}

async fn backend() -> (Arc<SqliteBackend>, TempDir) {
    let (backend, media) = common::sqlite_backend().await;
    (Arc::new(backend), media)
}

async fn harness(queue: Queue) -> Harness {
    let (backend, media) = backend().await;
    backend.queue_repo().save(&queue).await.unwrap();

    let bus = EventBus::new(Arc::new(NullEventLogRepo));
    let dp: Arc<dyn DataProvider> = Arc::clone(&backend) as Arc<dyn DataProvider>;
    let runtime = common::frozen_runtime(common::wiring_parts(dp, Arc::clone(&bus)).await).await;

    Harness {
        trigger_instances: backend.trigger_instance_repo(),
        actions: backend.action_repo(),
        globals: backend.globals_repo(),
        handle: runtime.scheduler.clone(),
        runtime,
        bus,
        _media: media,
    }
}

fn action(id: ActionId, queue_id: QueueId, enabled: bool, set_global: &str) -> Action {
    let mut config = BTreeMap::new();
    config.insert("name".to_owned(), Variant::String(set_global.to_owned()));
    config.insert("value".to_owned(), Variant::String("%out_key%".to_owned()));
    Action {
        enabled,
        ..common::sequential_action(
            id,
            "bound",
            queue_id,
            vec![SubActionStep {
                kind_id: "core.globals.set".to_owned(),
                config,
                enabled: true,
                continue_on_error: false,
                condition: None,
                label: None,
            }],
        )
    }
}

fn fire_cfg(instance_id: &str, override_outputs: Variant) -> SubActionConfig {
    let mut config = SubActionConfig::new();
    config.insert(
        "trigger_instance_id".to_owned(),
        Variant::String(instance_id.to_owned()),
    );
    config.insert("override_outputs".to_owned(), override_outputs);
    config
}

fn one_output(key: &str, value: &str) -> Variant {
    Variant::Object(BTreeMap::from([(
        key.to_owned(),
        Variant::String(value.to_owned()),
    )]))
}

fn no_outputs() -> Variant {
    Variant::String(String::new())
}

async fn run_outcome(runner: &dyn SubActionRunner, config: &SubActionConfig) -> SubActionOutcome {
    let stack = ArgStack::new();
    let ctx = RunContext::leaf(&stack, 0, EventId::new(), &NullPublisher);
    let (telemetry, _) = runner.execute(config, &ctx).await;
    telemetry.outcome
}

async fn await_action_done(sub: &mut EventSubscription, action_id: ActionId) -> bool {
    let target = action_id.to_string();
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            match sub.recv().await {
                Ok(ev)
                    if ev.kind == "action.done"
                        && ev.payload.get("action_id").and_then(|v| v.as_str())
                            == Some(target.as_str()) =>
                {
                    return true;
                }
                Ok(_) | Err(forge_events::EventsError::LaggingReceiver) => {}
                Err(_) => return false,
            }
        }
    })
    .await
    .unwrap_or(false)
}

fn fire_runner(harness: &Harness) -> &dyn SubActionRunner {
    harness
        .runtime
        .sub_action_registry
        .get("core.test.fire_trigger")
        .unwrap()
}

#[tokio::test]
async fn fired_trigger_runs_the_chain_of_its_one_bound_enabled_action() {
    let q_id = QueueId::new();
    let harness = harness(nonblocking(q_id)).await;
    let mut sub = harness.bus.subscribe();

    let a_id = ActionId::new();
    harness
        .actions
        .save(&action(a_id, q_id, true, "captured"))
        .await
        .unwrap();
    let instance_id = harness
        .trigger_instances
        .upsert_default("test.kind", "Test Trigger")
        .await
        .unwrap();
    harness
        .trigger_instances
        .link_action(a_id, instance_id)
        .await
        .unwrap();

    let runner = fire_runner(&harness);
    let outcome = run_outcome(
        runner,
        &fire_cfg(&instance_id.to_string(), one_output("out_key", "ran")),
    )
    .await;

    assert!(matches!(outcome, SubActionOutcome::Success));
    assert!(
        await_action_done(&mut sub, a_id).await,
        "firing the instance must run the bound enabled action's chain to completion"
    );
    harness.handle.shutdown();
}

struct Fired {
    outcome: SubActionOutcome,
    harness: Harness,
    sub: EventSubscription,
    queue_id: QueueId,
    bound: ActionId,
}

async fn fire_bound_action(override_outputs: Variant) -> Fired {
    let queue_id = QueueId::new();
    let harness = harness(blocking(queue_id)).await;
    let sub = harness.bus.subscribe();
    let bound = ActionId::new();
    harness
        .actions
        .save(&action(bound, queue_id, true, "captured"))
        .await
        .unwrap();
    let instance_id = harness
        .trigger_instances
        .upsert_default("test.kind", "Test Trigger")
        .await
        .unwrap();
    harness
        .trigger_instances
        .link_action(bound, instance_id)
        .await
        .unwrap();

    let runner = fire_runner(&harness);
    let outcome = run_outcome(
        runner,
        &fire_cfg(&instance_id.to_string(), override_outputs),
    )
    .await;
    Fired {
        outcome,
        harness,
        sub,
        queue_id,
        bound,
    }
}

#[tokio::test]
async fn override_outputs_reach_the_fired_actions_execution_context() {
    for outputs in [
        one_output("out_key", "synthetic-output-marker"),
        Variant::String(r#"{"out_key": "synthetic-output-marker"}"#.to_owned()),
    ] {
        let mut fired = fire_bound_action(outputs.clone()).await;
        assert!(
            matches!(fired.outcome, SubActionOutcome::Success),
            "{outputs:?}"
        );
        assert!(
            await_action_done(&mut fired.sub, fired.bound).await,
            "the bound action must complete before its global is read"
        );
        let captured = fired.harness.globals.get("captured").await.unwrap();
        assert!(
            matches!(captured, Some(Variant::String(ref s)) if s == "synthetic-output-marker"),
            "{outputs:?}: override_outputs must arrive as the fired action's ArgStack, got {captured:?}"
        );
        fired.harness.handle.shutdown();
    }
}

#[tokio::test]
async fn invalid_override_outputs_fail_naming_the_field_and_dispatch_nothing() {
    for outputs in ["object{0}", "[1]", "null"] {
        let mut fired = fire_bound_action(Variant::String(outputs.to_owned())).await;
        assert!(
            matches!(&fired.outcome, SubActionOutcome::Failed(message) if common::rejected_field(message) == Some("override_outputs")),
            "{outputs}: {:?}",
            fired.outcome
        );

        let barrier = ActionId::new();
        fired
            .harness
            .actions
            .save(&action(barrier, fired.queue_id, true, "barrier_ran"))
            .await
            .unwrap();
        fired
            .harness
            .handle
            .dispatch(SchedulerRequest {
                queue_id: fired.queue_id,
                action_id: barrier,
                trigger_event_id: EventId::new(),
                trigger_kind: None,
                manual_test: false,
                initial_args: ArgStack::new(),
                admission: QueueAdmission::default(),
            })
            .unwrap();
        assert!(
            await_action_done(&mut fired.sub, barrier).await,
            "barrier must complete so the FIFO queue is fully drained"
        );
        assert!(
            fired
                .harness
                .globals
                .get("captured")
                .await
                .unwrap()
                .is_none(),
            "{outputs}: a rejected override must not dispatch the bound action"
        );
        fired.harness.handle.shutdown();
    }
}

#[tokio::test]
async fn validate_config_rejects_non_object_override_outputs_as_invalid_format() {
    let harness = harness(nonblocking(QueueId::new())).await;
    let runner = fire_runner(&harness);
    let err = runner
        .validate_config(&fire_cfg(
            &TriggerInstanceId::new().to_string(),
            Variant::String("object{0}".to_owned()),
        ))
        .unwrap_err();
    assert!(
        matches!(
            &err,
            RegistryError::InvalidConfig(ConfigProblem {
                field: Some(field),
                reason: ConfigReason::Coded(message),
            }) if field == "override_outputs" && message.code().to_string() == "config.not_json"
        ),
        "{err:?}"
    );
}

#[tokio::test]
async fn unknown_trigger_instance_id_fails() {
    let harness = harness(nonblocking(QueueId::new())).await;
    let runner = fire_runner(&harness);

    let missing = TriggerInstanceId::new();
    let outcome = run_outcome(runner, &fire_cfg(&missing.to_string(), no_outputs())).await;

    let refusal = failure_message(&outcome);
    assert_eq!(code_of(refusal), "core.unknown_trigger_instance");
    assert_eq!(
        verbatim_arg(refusal, "instance"),
        Some(missing.to_string().as_str())
    );
    harness.handle.shutdown();
}

#[tokio::test]
async fn unparseable_trigger_instance_id_fails() {
    const SENTINEL: &str = "not-an-instance-id-a-viewer-typed-this";
    let harness = harness(nonblocking(QueueId::new())).await;
    let runner = fire_runner(&harness);

    let outcome = run_outcome(runner, &fire_cfg(SENTINEL, no_outputs())).await;

    let SubActionOutcome::Failed(msg) = outcome else {
        panic!("an unparseable trigger_instance_id must fail before any lookup, got {outcome:?}");
    };
    assert_eq!(
        msg,
        common::plain_field_problem(
            "core.test.fire_trigger",
            "trigger_instance_id",
            "config.unknown_entry"
        ),
        "the failure must name the field without echoing the value"
    );
}

#[tokio::test]
async fn instance_with_no_bound_actions_succeeds_and_dispatches_nothing() {
    let q_id = QueueId::new();
    let harness = harness(blocking(q_id)).await;
    let mut sub = harness.bus.subscribe();

    let trap = ActionId::new();
    harness
        .actions
        .save(&action(trap, q_id, true, "trap_ran"))
        .await
        .unwrap();
    let instance_id = harness
        .trigger_instances
        .upsert_default("test.kind", "Test Trigger")
        .await
        .unwrap();

    let runner = fire_runner(&harness);
    let outcome = run_outcome(runner, &fire_cfg(&instance_id.to_string(), no_outputs())).await;
    assert!(
        matches!(outcome, SubActionOutcome::Success),
        "an instance found with no bound actions must succeed, got {outcome:?}"
    );

    let barrier = ActionId::new();
    harness
        .actions
        .save(&action(barrier, q_id, true, "barrier_ran"))
        .await
        .unwrap();
    harness
        .handle
        .dispatch(SchedulerRequest {
            queue_id: q_id,
            action_id: barrier,
            trigger_event_id: EventId::new(),
            trigger_kind: None,
            manual_test: false,
            initial_args: ArgStack::new(),
            admission: QueueAdmission::default(),
        })
        .unwrap();
    assert!(
        await_action_done(&mut sub, barrier).await,
        "barrier must complete so the FIFO queue is fully drained"
    );

    assert!(
        harness.globals.get("barrier_ran").await.unwrap().is_some(),
        "the barrier's own write proves the step that would expose the trap works"
    );
    assert!(
        harness.globals.get("trap_ran").await.unwrap().is_none(),
        "firing an instance with no bound actions must dispatch nothing"
    );
    harness.handle.shutdown();
}

#[tokio::test]
async fn fired_trigger_dispatches_disabled_action_but_engine_skips_it() {
    let q_id = QueueId::new();
    let harness = harness(blocking(q_id)).await;
    let mut sub = harness.bus.subscribe();

    let disabled = ActionId::new();
    harness
        .actions
        .save(&action(disabled, q_id, false, "disabled_ran"))
        .await
        .unwrap();
    let instance_id = harness
        .trigger_instances
        .upsert_default("test.kind", "Test Trigger")
        .await
        .unwrap();
    harness
        .trigger_instances
        .link_action(disabled, instance_id)
        .await
        .unwrap();

    let runner = fire_runner(&harness);
    let outcome = run_outcome(
        runner,
        &fire_cfg(&instance_id.to_string(), one_output("out_key", "x")),
    )
    .await;
    assert!(
        matches!(outcome, SubActionOutcome::Success),
        "the runner dispatches the disabled action and returns Success, got {outcome:?}"
    );

    let barrier = ActionId::new();
    harness
        .actions
        .save(&action(barrier, q_id, true, "barrier_ran"))
        .await
        .unwrap();
    harness
        .handle
        .dispatch(SchedulerRequest {
            queue_id: q_id,
            action_id: barrier,
            trigger_event_id: EventId::new(),
            trigger_kind: None,
            manual_test: false,
            initial_args: ArgStack::new(),
            admission: QueueAdmission::default(),
        })
        .unwrap();
    assert!(
        await_action_done(&mut sub, barrier).await,
        "barrier must complete so the disabled action is known to have been processed"
    );

    assert!(
        harness.globals.get("disabled_ran").await.unwrap().is_none(),
        "the engine must skip the disabled action: no execution effect"
    );
    assert!(
        harness.globals.get("barrier_ran").await.unwrap().is_some(),
        "positive control: the barrier action must have executed"
    );
    harness.handle.shutdown();
}
