#![allow(clippy::unwrap_used, clippy::expect_used)]
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use forge_events::{Event, EventPublisher};
use forge_registry::{
    FormField, RegistryError, RunContext, StepTimer, SubActionCategory, SubActionRegistry,
    SubActionRunner,
};
use forge_runtime::sub_action_runners::{
    CoreActionCancelRunner, CoreActionRunRunner, CoreArgsSetRunner, CoreLogicIfThenElseRunner,
    CoreLogicLoopRunner, CoreLogicSwitchCaseRunner,
};
use forge_runtime::{
    ActionCancelRegistry, ConditionGate, Config, EventBus, ExecutionRequest, NullEventLogRepo,
    spawn_action_engine,
};
use forge_storage::DataProvider;
use forge_storage_sqlite::SqliteBackend;
use forge_types::{
    Action, ActionId, ArgStack, EventId, ExecutionMode, ExecutionOutcome, QueueId, SubActionConfig,
    SubActionOutcome, SubActionStep, SubActionTelemetry, Variant,
};
use tempfile::TempDir;
use time::OffsetDateTime;
use tokio::sync::Notify;

struct NullPublisher;
impl EventPublisher for NullPublisher {
    fn publish(&self, _event: Event) {}
}

struct GateRunner {
    running: Arc<Notify>,
}

#[async_trait]
impl SubActionRunner for GateRunner {
    fn id(&self) -> &str {
        "test.gate"
    }
    fn category(&self) -> SubActionCategory {
        SubActionCategory::Util
    }
    fn label(&self) -> &str {
        ""
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
    fn default_config(&self) -> SubActionConfig {
        SubActionConfig::new()
    }
    fn config_fields(&self) -> Vec<FormField> {
        Vec::new()
    }
    fn validate_config(&self, _: &SubActionConfig) -> Result<(), RegistryError> {
        Ok(())
    }
    async fn execute(
        &self,
        _: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        self.running.notify_one();
        while !ctx.cancel.is_cancelled() {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
        (
            SubActionTelemetry {
                args_in: ::std::collections::BTreeMap::new(),
                produced: ::std::collections::BTreeMap::new(),
                index: ctx.index,
                kind: "test.gate".to_owned(),
                started_at: OffsetDateTime::now_utc(),
                duration_ms: 0,
                outcome: SubActionOutcome::Success,
            },
            None,
        )
    }
}

async fn make_dp() -> (Arc<dyn DataProvider>, TempDir) {
    let media = tempfile::tempdir().unwrap();
    let backend =
        SqliteBackend::open_for_test(":memory:", [0xcd; 32], media.path().join("media"), None)
            .await
            .unwrap();
    (Arc::new(backend), media)
}

fn default_queue() -> QueueId {
    serde_json::from_str("\"00000000000000000000000000\"").unwrap()
}

fn action(id: ActionId, steps: Vec<SubActionStep>) -> Action {
    Action {
        id,
        name: "cancel-target".to_owned(),
        group: None,
        queue_id: default_queue(),
        enabled: true,
        concurrent: false,
        bypass_pause: false,
        execution_mode: ExecutionMode::Sequential,
        description: None,
        sub_actions: steps,
    }
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

async fn await_history(dp: &Arc<dyn DataProvider>, id: ActionId) -> ExecutionOutcome {
    tokio::time::timeout(HANG_GUARD, async {
        loop {
            let recent = dp.history_repo().recent_for_action(id, 1).await.unwrap();
            if let Some(ctx) = recent.into_iter().next() {
                return ctx.outcome;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
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

    let engine = spawn_action_engine(
        Arc::clone(&bus),
        forge_runtime::Catalog::new(
            dp.action_repo(),
            dp.trigger_instance_repo(),
            dp.catalog_revision(),
        ),
        dp.action_repo(),
        dp.history_repo(),
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
        await_history(&dp, target).await,
        ExecutionOutcome::Cancelled,
        "the runner's cancel must unwind the live execution to Cancelled"
    );
}

#[tokio::test]
async fn cancel_guard_deregisters_after_a_run_completes() {
    let (dp, _media) = make_dp().await;
    let bus = EventBus::new(Arc::new(NullEventLogRepo));
    let cancel_registry = Arc::new(ActionCancelRegistry::new());

    let engine = spawn_action_engine(
        Arc::clone(&bus),
        forge_runtime::Catalog::new(
            dp.action_repo(),
            dp.trigger_instance_repo(),
            dp.catalog_revision(),
        ),
        dp.action_repo(),
        dp.history_repo(),
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

    assert_eq!(await_history(&dp, id).await, ExecutionOutcome::Success);
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

struct CancelsItsRunRunner;

#[async_trait]
impl SubActionRunner for CancelsItsRunRunner {
    fn id(&self) -> &str {
        "test.cancels_its_run"
    }
    fn category(&self) -> SubActionCategory {
        SubActionCategory::Util
    }
    fn label(&self) -> &str {
        ""
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
    fn default_config(&self) -> SubActionConfig {
        SubActionConfig::new()
    }
    fn config_fields(&self) -> Vec<FormField> {
        Vec::new()
    }
    fn validate_config(&self, _config: &SubActionConfig) -> Result<(), RegistryError> {
        Ok(())
    }
    async fn execute(
        &self,
        _config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        ctx.cancel.cancel();
        (
            StepTimer::start(ctx, "test.cancels_its_run").success(),
            None,
        )
    }
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
    let engine = spawn_action_engine(
        Arc::clone(&bus),
        forge_runtime::Catalog::new(
            dp.action_repo(),
            dp.trigger_instance_repo(),
            dp.catalog_revision(),
        ),
        dp.action_repo(),
        dp.history_repo(),
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
            await_history(&dp, target).await,
            ExecutionOutcome::Cancelled,
            "{label}"
        );
    }
}
