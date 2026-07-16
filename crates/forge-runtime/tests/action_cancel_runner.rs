//! End-to-end wiring for `core.action.cancel`. The runner reaches a *live*
//! `ActionEngine` through the shared `ActionCancelRegistry` the engine registers
//! every in-flight run into. These tests prove the feature's whole point - the
//! runner aborts a run that is actually executing - and the RAII `CancelGuard`
//! cleanup on the engine's normal exit path.
//!
//! Registry-internal semantics (per-execution keying, count returns) live in
//! `action_cancel`'s own unit tests; these do NOT re-exercise them.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use forge_events::{Event, EventPublisher};
use forge_registry::{
    FormField, RegistryError, RunContext, SubActionCategory, SubActionRegistry, SubActionRunner,
};
use forge_runtime::sub_action_runners::CoreActionCancelRunner;
use forge_runtime::{
    ActionCancelRegistry, EventBus, ExecutionRequest, NullEventLogRepo, spawn_action_engine,
};
use forge_storage::DataProvider;
use forge_storage_sqlite::SqliteBackend;
use forge_types::{
    Action, ActionId, ArgStack, EventId, ExecutionMode, ExecutionOutcome, QueueId, SubActionConfig,
    SubActionOutcome, SubActionStep, SubActionTelemetry, Variant,
};
use time::OffsetDateTime;
use tokio::sync::Notify;

struct NullPublisher;
impl EventPublisher for NullPublisher {
    fn publish(&self, _event: Event) {}
}

/// Blocks the chain in-flight: announces it is running, then cooperatively polls
/// the execution's cancel signal until tripped, mirroring how real long-running
/// leaves observe cancellation between awaits.
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
        for _ in 0..400 {
            if ctx.cancel.is_cancelled() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
        (
            SubActionTelemetry {
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

async fn make_dp() -> Arc<dyn DataProvider> {
    Arc::new(
        SqliteBackend::open_with_key(":memory:", [0xcd; 32])
            .await
            .unwrap(),
    )
}

/// The migrations seed exactly one queue (the nil ULID); the actions FK references
/// it, so reuse that row rather than inventing a queue id.
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

async fn await_history(dp: &Arc<dyn DataProvider>, id: ActionId) -> ExecutionOutcome {
    let mut found = None;
    for _ in 0..80 {
        let recent = dp.history_repo().recent_for_action(id, 1).await.unwrap();
        if let Some(ctx) = recent.into_iter().next() {
            found = Some(ctx.outcome);
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    found.expect("a run must record an outcome to history")
}

#[tokio::test]
async fn cancel_runner_aborts_a_live_in_flight_execution() {
    let dp = make_dp().await;
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
                label: None,
            }],
        ))
        .await
        .unwrap();

    engine
        .dispatch(ExecutionRequest {
            action_id: target,
            trigger_event_id: EventId::new(),
            initial_args: ArgStack::new(),
        })
        .await
        .unwrap();

    // Only proceed once the run is provably in-flight.
    tokio::time::timeout(Duration::from_secs(5), running.notified())
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
    // Why: the per-execution CancelGuard must deregister on the engine's normal
    // exit path. Once a run is in history it has returned, so a later cancel of
    // that action finds nothing registered and trips zero signals.
    let dp = make_dp().await;
    let bus = EventBus::new(Arc::new(NullEventLogRepo));
    let cancel_registry = Arc::new(ActionCancelRegistry::new());

    let engine = spawn_action_engine(
        Arc::clone(&bus),
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
            initial_args: ArgStack::new(),
        })
        .await
        .unwrap();

    assert_eq!(await_history(&dp, id).await, ExecutionOutcome::Success);
    assert_eq!(
        cancel_registry.cancel(id),
        0,
        "the guard must have deregistered the finished run"
    );
}
