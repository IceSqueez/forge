#![allow(clippy::unwrap_used, clippy::expect_used)]
mod common;
use common::runners::{CancelsItsRunRunner, GateRunner};
use forge_events::test_support::NullPublisher;

use std::sync::Arc;
use std::time::Duration;

use forge_registry::{RunContext, SubActionRegistry, SubActionRunner};
use forge_runtime::{
    ActionCancelRegistry, ConditionGate, Config, EventBus, ExecutionRequest, NullEventLogRepo,
    spawn_action_engine,
};
use forge_runtime::{
    CoreActionCancelRunner, CoreActionRunRunner, CoreArgsSetRunner, CoreLogicIfThenElseRunner,
    CoreLogicLoopRunner, CoreLogicSwitchCaseRunner,
};
use forge_storage::DataProvider;
use forge_storage::HistoryRepo;
use forge_storage::MockHistoryRepo;
use forge_types::{
    Action, ActionId, ArgStack, DEFAULT_QUEUE_ID, EventId, ExecutionContext, ExecutionOutcome,
    SubActionConfig, SubActionOutcome, SubActionStep, Variant,
};
use tempfile::TempDir;
use tokio::sync::{Notify, mpsc};

async fn make_dp() -> (Arc<dyn DataProvider>, TempDir) {
    let (backend, media) = common::sqlite_backend().await;
    (Arc::new(backend), media)
}

fn action(id: ActionId, steps: Vec<SubActionStep>) -> Action {
    common::sequential_action(id, "cancel-target", DEFAULT_QUEUE_ID, steps)
}

fn cancel_cfg(action_id: &str) -> SubActionConfig {
    let mut c = SubActionConfig::new();
    c.insert(
        "action_id".to_owned(),
        Variant::String(action_id.to_owned()),
    );
    c
}

const HANG_GUARD: Duration = Duration::from_secs(30);

type SavedRuns = mpsc::UnboundedReceiver<ExecutionContext>;

fn observed_history() -> (Arc<dyn HistoryRepo>, SavedRuns) {
    let (saved, runs) = mpsc::unbounded_channel();
    let mut history = MockHistoryRepo::new();
    history.expect_save_batch().returning(move |contexts| {
        for context in contexts {
            saved.send(context.clone()).unwrap();
        }
        Ok(())
    });
    (Arc::new(history), runs)
}

async fn await_history(runs: &mut SavedRuns, id: ActionId) -> ExecutionOutcome {
    tokio::time::timeout(HANG_GUARD, async {
        loop {
            let run = runs.recv().await.expect("the history writer stopped");
            if run.action_id == id {
                return run.outcome;
            }
        }
    })
    .await
    .expect("a run must record an outcome to history")
}

#[tokio::test]
async fn cancel_runner_aborts_a_live_in_flight_execution() {
    let (dp, _media) = make_dp().await;
    let bus = EventBus::new(Arc::new(NullEventLogRepo));
    let cancel_registry = Arc::new(ActionCancelRegistry::new());

    let running = Arc::new(Notify::new());
    let mut reg = SubActionRegistry::new();
    reg.register(Box::new(GateRunner {
        running: Arc::clone(&running),
    }))
    .unwrap();

    let (history, mut runs) = observed_history();
    let engine = spawn_action_engine(
        Arc::clone(&bus),
        forge_runtime::Catalog::new(
            dp.action_repo(),
            dp.trigger_instance_repo(),
            dp.catalog_revision(),
        ),
        dp.action_repo(),
        history,
        Arc::new(reg),
        Arc::clone(&cancel_registry),
    );

    let target = ActionId::new();
    dp.action_repo()
        .save(&action(
            target,
            vec![SubActionStep {
                kind_id: "test.gate".to_owned(),
                config: SubActionConfig::new(),
                enabled: true,
                continue_on_error: false,
                condition: None,
                label: None,
            }],
        ))
        .await
        .unwrap();

    engine
        .dispatch(ExecutionRequest {
            action_id: target,
            trigger_event_id: EventId::new(),
            trigger_kind: None,
            manual_test: false,
            initial_args: ArgStack::new(),
        })
        .await
        .unwrap();

    tokio::time::timeout(HANG_GUARD, running.notified())
        .await
        .expect("gated action never reached its in-flight point");

    let cancel_runner = CoreActionCancelRunner::new(Arc::clone(&cancel_registry));
    let stack = ArgStack::new();
    let ctx = RunContext::leaf(&stack, 0, EventId::new(), &NullPublisher);
    let (telemetry, _) = cancel_runner
        .execute(&cancel_cfg(&target.to_string()), &ctx)
        .await;
    assert!(matches!(telemetry.outcome, SubActionOutcome::Success));

    assert_eq!(
        await_history(&mut runs, target).await,
        ExecutionOutcome::Cancelled,
        "the runner's cancel must unwind the live execution to Cancelled"
    );
}

#[tokio::test]
async fn cancel_guard_deregisters_after_a_run_completes() {
    let (dp, _media) = make_dp().await;
    let bus = EventBus::new(Arc::new(NullEventLogRepo));
    let cancel_registry = Arc::new(ActionCancelRegistry::new());

    let (history, mut runs) = observed_history();
    let engine = spawn_action_engine(
        Arc::clone(&bus),
        forge_runtime::Catalog::new(
            dp.action_repo(),
            dp.trigger_instance_repo(),
            dp.catalog_revision(),
        ),
        dp.action_repo(),
        history,
        Arc::new(SubActionRegistry::new()),
        Arc::clone(&cancel_registry),
    );

    let id = ActionId::new();
    dp.action_repo()
        .save(&action(id, Vec::new()))
        .await
        .unwrap();

    engine
        .dispatch(ExecutionRequest {
            action_id: id,
            trigger_event_id: EventId::new(),
            trigger_kind: None,
            manual_test: false,
            initial_args: ArgStack::new(),
        })
        .await
        .unwrap();

    assert_eq!(
        await_history(&mut runs, id).await,
        ExecutionOutcome::Success
    );
    let deregistered = tokio::time::timeout(HANG_GUARD, async {
        while cancel_registry.cancel(id) != 0 {
            tokio::task::yield_now().await;
        }
    })
    .await;
    assert!(
        deregistered.is_ok(),
        "the guard must have deregistered the finished run"
    );
}

fn plain_step(kind: &str, config: SubActionConfig) -> SubActionStep {
    SubActionStep {
        kind_id: kind.to_owned(),
        config,
        enabled: true,
        continue_on_error: false,
        condition: None,
        label: None,
    }
}

fn chain_entry(kind: &str) -> Variant {
    let mut entry = SubActionConfig::new();
    entry.insert("kind_id".to_owned(), Variant::String(kind.to_owned()));
    entry.insert("config".to_owned(), Variant::Object(SubActionConfig::new()));
    Variant::Object(entry)
}

fn interrupted_chain() -> Variant {
    Variant::Array(vec![
        chain_entry("test.cancels_its_run"),
        chain_entry("core.args.set"),
    ])
}

fn composite_config(entries: &[(&str, Variant)]) -> SubActionConfig {
    entries
        .iter()
        .map(|(key, value)| ((*key).to_owned(), value.clone()))
        .collect()
}

fn interrupted_composites() -> [(&'static str, SubActionStep); 3] {
    [
        (
            "if",
            plain_step(
                "core.logic.if_then_else",
                composite_config(&[
                    ("condition", Variant::String("1 == 1".to_owned())),
                    ("then_chain", interrupted_chain()),
                    ("else_chain", Variant::Array(Vec::new())),
                ]),
            ),
        ),
        (
            "switch",
            plain_step(
                "core.logic.switch_case",
                composite_config(&[
                    ("expression", Variant::String("zzz".to_owned())),
                    ("cases", Variant::Array(Vec::new())),
                    ("default_chain", interrupted_chain()),
                ]),
            ),
        ),
        (
            "loop",
            plain_step(
                "core.logic.loop",
                composite_config(&[
                    ("mode", Variant::String("count".to_owned())),
                    ("count", Variant::Int(3)),
                    ("body", interrupted_chain()),
                ]),
            ),
        ),
    ]
}

#[tokio::test]
async fn a_run_cancelled_inside_a_composite_step_is_recorded_as_cancelled() {
    let (dp, _media) = make_dp().await;
    let config = Config::default();
    let gate = Arc::new(ConditionGate::new(&config));
    let mut reg = SubActionRegistry::new();
    reg.register(Box::new(CancelsItsRunRunner)).unwrap();
    reg.register(Box::new(CoreArgsSetRunner)).unwrap();
    reg.register(Box::new(CoreLogicSwitchCaseRunner)).unwrap();
    reg.register(Box::new(CoreLogicIfThenElseRunner::new(Arc::clone(&gate))))
        .unwrap();
    reg.register(Box::new(CoreLogicLoopRunner::new(gate)))
        .unwrap();
    reg.register(Box::new(CoreActionRunRunner::new(dp.action_repo())))
        .unwrap();
    let bus = EventBus::new(Arc::new(NullEventLogRepo));
    let (history, mut runs) = observed_history();
    let engine = spawn_action_engine(
        Arc::clone(&bus),
        forge_runtime::Catalog::new(
            dp.action_repo(),
            dp.trigger_instance_repo(),
            dp.catalog_revision(),
        ),
        dp.action_repo(),
        history,
        Arc::new(reg),
        Arc::new(ActionCancelRegistry::new()),
    );
    let child = ActionId::new();
    dp.action_repo()
        .save(&action(
            child,
            vec![
                plain_step("test.cancels_its_run", SubActionConfig::new()),
                plain_step("core.args.set", SubActionConfig::new()),
            ],
        ))
        .await
        .unwrap();
    let nested_run = (
        "run action",
        plain_step("core.action.run", cancel_cfg(&child.to_string())),
    );

    for (label, composite) in interrupted_composites().into_iter().chain([nested_run]) {
        let target = ActionId::new();
        dp.action_repo()
            .save(&action(target, vec![composite]))
            .await
            .unwrap();

        engine
            .dispatch(ExecutionRequest {
                action_id: target,
                trigger_event_id: EventId::new(),
                trigger_kind: None,
                manual_test: false,
                initial_args: ArgStack::new(),
            })
            .await
            .unwrap();

        assert_eq!(
            await_history(&mut runs, target).await,
            ExecutionOutcome::Cancelled,
            "{label}"
        );
    }
}
