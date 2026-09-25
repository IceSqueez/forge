use std::sync::Arc;

use forge_events::{Event, EventPublisher, EventSource};
use forge_registry::{CancelSignal, ChainSignal, RunContext, SubActionRegistry, effective_config};
use forge_storage::{ActionRepo, ExecutionStatus, HistoryRepo};
use forge_types::{
    Action, ActionId, ArgStack, EventId, ExecutionContext, ExecutionMetadata, ExecutionOutcome,
    SubActionOutcome, SubActionStep, SubActionTelemetry,
};
use serde_json::json;
use time::OffsetDateTime;
use tokio::sync::{mpsc, oneshot, watch};
use tracing::{info, warn};

use crate::action_cancel::ActionCancelRegistry;
use crate::chain::ChainEngine;
use crate::{Config, EventBus};

const EXECUTION_INTAKE_CAPACITY: usize = 256;
const QUICK_ACTION_INTAKE_CAPACITY: usize = 64;
const ACTION_START_KIND: &str = "action.start";
const MAX_CAUSATION_DEPTH: usize = 16;
const CAUSATION_DEPTH_SKIP_REASON: &str = "causation_depth_exceeded";

struct QuickActionRequest {
    step: SubActionStep,
    builtin_id: String,
    label: String,
    caused_by: Option<EventId>,
    outcome: oneshot::Sender<SubActionOutcome>,
}

pub struct PendingQuickAction(oneshot::Receiver<SubActionOutcome>);

impl PendingQuickAction {
    pub async fn outcome(self) -> Result<SubActionOutcome, DispatchError> {
        self.0.await.map_err(|_| DispatchError::NoOutcome)
    }
}

#[derive(Clone)]
pub struct ActionEngineHandle {
    sender: mpsc::Sender<EngineJob>,
    quick_sender: mpsc::Sender<QuickActionRequest>,
    stop: Arc<watch::Sender<bool>>,
}

pub struct ExecutionRequest {
    pub action_id: ActionId,
    pub trigger_event_id: EventId,
    pub trigger_kind: Option<String>,
    pub initial_args: forge_types::ArgStack,
}

struct EngineJob {
    request: ExecutionRequest,
    cancel: CancelSignal,
    on_complete: Option<oneshot::Sender<()>>,
}

#[derive(Debug, thiserror::Error)]
pub enum DispatchError {
    #[error("engine channel closed")]
    ChannelClosed,
    #[error("engine stopped before the step reported an outcome")]
    NoOutcome,
}

impl ActionEngineHandle {
    pub async fn dispatch(&self, req: ExecutionRequest) -> Result<(), DispatchError> {
        self.sender
            .send(EngineJob {
                request: req,
                cancel: CancelSignal::new(),
                on_complete: None,
            })
            .await
            .map_err(|_| DispatchError::ChannelClosed)
    }

    /// Fires `on_complete` once the run terminates, regardless of outcome.
    pub(crate) async fn dispatch_tracked(
        &self,
        req: ExecutionRequest,
        cancel: CancelSignal,
        on_complete: oneshot::Sender<()>,
    ) -> Result<(), DispatchError> {
        self.sender
            .send(EngineJob {
                request: req,
                cancel,
                on_complete: Some(on_complete),
            })
            .await
            .map_err(|_| DispatchError::ChannelClosed)
    }

    /// Dropping the returned handle leaves the step running without reporting its outcome.
    pub async fn execute_quick_action(
        &self,
        step: SubActionStep,
        builtin_id: String,
        label: String,
        caused_by: Option<EventId>,
    ) -> Result<PendingQuickAction, DispatchError> {
        let (outcome, pending) = oneshot::channel();
        self.quick_sender
            .send(QuickActionRequest {
                step,
                builtin_id,
                label,
                caused_by,
                outcome,
            })
            .await
            .map_err(|_| DispatchError::ChannelClosed)?;
        Ok(PendingQuickAction(pending))
    }

    /// Stops intake and cancels every running execution; quick actions already running finish.
    pub fn shutdown(self) {
        self.stop.send_replace(true);
    }
}

struct ActionEngine {
    bus: Arc<EventBus>,
    actions: Arc<dyn ActionRepo>,
    history: Arc<dyn HistoryRepo>,
    chain_engine: Arc<ChainEngine>,
    cancel_registry: Arc<ActionCancelRegistry>,
}

impl ActionEngine {
    pub fn spawn(
        bus: Arc<EventBus>,
        actions: Arc<dyn ActionRepo>,
        history: Arc<dyn HistoryRepo>,
        sub_action_registry: Arc<SubActionRegistry>,
        cancel_registry: Arc<ActionCancelRegistry>,
    ) -> ActionEngineHandle {
        let (tx, rx) = mpsc::channel(EXECUTION_INTAKE_CAPACITY);
        let (quick_tx, quick_rx) = mpsc::channel(QUICK_ACTION_INTAKE_CAPACITY);
        let (stop_tx, stop_rx) = watch::channel(false);
        let publisher: Arc<dyn EventPublisher> = Arc::clone(&bus) as Arc<dyn EventPublisher>;
        let config = Config::default();
        let gate = Arc::new(crate::condition::ConditionGate::new(&config));
        let chain_engine = Arc::new(ChainEngine::new(
            Arc::clone(&sub_action_registry),
            publisher,
            gate,
            config,
        ));
        let engine = Arc::new(Self {
            bus: Arc::clone(&bus),
            actions: Arc::clone(&actions),
            history: Arc::clone(&history),
            chain_engine,
            cancel_registry,
        });
        tokio::spawn(Self::run(engine, rx, stop_rx.clone()));
        tokio::spawn(run_quick_action_loop(
            quick_rx,
            stop_rx,
            bus,
            history,
            sub_action_registry,
        ));
        ActionEngineHandle {
            sender: tx,
            quick_sender: quick_tx,
            stop: Arc::new(stop_tx),
        }
    }

    async fn run(
        engine: Arc<Self>,
        mut input: mpsc::Receiver<EngineJob>,
        mut stop: watch::Receiver<bool>,
    ) {
        loop {
            tokio::select! {
                biased;
                Ok(_) = stop.wait_for(|stopped| *stopped) => {
                    let cancelled = engine.cancel_registry.cancel_all();
                    info!("action engine stopped: cancelled {cancelled} running executions");
                    return;
                }
                job = input.recv() => match job {
                    Some(job) => engine.start(job),
                    None => return,
                },
            }
        }
    }

    fn start(self: &Arc<Self>, job: EngineJob) {
        let EngineJob {
            request,
            cancel,
            on_complete,
        } = job;
        let cancel_guard = CancelGuard {
            exec_id: self
                .cancel_registry
                .register(request.action_id, cancel.clone()),
            registry: Arc::clone(&self.cancel_registry),
            action_id: request.action_id,
        };
        let engine = Arc::clone(self);
        tokio::spawn(async move {
            engine.run_execution(request, &cancel).await;
            drop(cancel_guard);
            if let Some(done) = on_complete {
                let _ = done.send(());
            }
        });
    }

    async fn refuse_too_deep(
        &self,
        action: &Action,
        trigger_event_id: EventId,
        trigger_kind: Option<String>,
        arg_stack: ArgStack,
        started_at: OffsetDateTime,
    ) {
        warn!(
            action = %action.id,
            action_name = %action.name,
            event = %trigger_event_id,
            max_depth = MAX_CAUSATION_DEPTH,
            "action not started: its triggering event descends from too many action runs, likely an event loop between actions"
        );
        self.bus.publish(Event::caused_by(
            EventSource::Core,
            "action.skipped",
            json!({
                "action_id": action.id.to_string(),
                "reason": CAUSATION_DEPTH_SKIP_REASON,
                "queue_id": action.queue_id.to_string(),
            }),
            trigger_event_id,
        ));
        let ctx = ExecutionContext {
            action_id: action.id,
            metadata: ExecutionMetadata::Trigger {
                event_id: trigger_event_id,
                trigger_kind,
            },
            arg_stack_snapshot: arg_stack.snapshot(),
            started_at,
            completed_at: Some(started_at),
            telemetry: Vec::new(),
            outcome: ExecutionOutcome::Failed(format!(
                "not started: the triggering event is more than {MAX_CAUSATION_DEPTH} action runs deep (event loop between actions?)"
            )),
        };
        if let Err(e) = self.history.save(&ctx).await {
            warn!("history_repo.save failed: {e}");
        }
    }

    async fn run_execution(&self, req: ExecutionRequest, cancel: &CancelSignal) {
        let action = match self.actions.get(req.action_id).await {
            Ok(Some(a)) if a.enabled => a,
            Ok(_) => return,
            Err(e) => {
                warn!("action_repo.get failed: {e}");
                return;
            }
        };

        let arg_stack = req.initial_args;
        let trigger_kind = req.trigger_kind;
        let started_at = OffsetDateTime::now_utc();

        let ancestor_runs = self.bus.count_in_lineage(
            req.trigger_event_id,
            |event| event.kind == ACTION_START_KIND,
            MAX_CAUSATION_DEPTH,
        );
        if ancestor_runs >= MAX_CAUSATION_DEPTH {
            self.refuse_too_deep(
                &action,
                req.trigger_event_id,
                trigger_kind,
                arg_stack,
                started_at,
            )
            .await;
            return;
        }

        let mut ctx = ExecutionContext {
            action_id: req.action_id,
            metadata: ExecutionMetadata::Trigger {
                event_id: req.trigger_event_id,
                trigger_kind,
            },
            arg_stack_snapshot: arg_stack.snapshot(),
            started_at,
            completed_at: None,
            telemetry: Vec::new(),
            outcome: ExecutionOutcome::Success,
        };

        let start_event = Event::caused_by(
            EventSource::Core,
            ACTION_START_KIND,
            json!({
                "action_id": action.id.to_string(),
                "action_name": action.name,
                "sub_action_count": action.sub_actions.len(),
            }),
            req.trigger_event_id,
        );
        let start_event_id = start_event.id;
        self.bus.publish(start_event);

        let pick: Vec<SubActionStep> = if matches!(
            action.execution_mode,
            forge_types::ExecutionMode::RandomPick
        ) && !action.sub_actions.is_empty()
        {
            use rand::RngExt;
            let idx = rand::rng().random_range(0..action.sub_actions.len());
            vec![action.sub_actions[idx].clone()]
        } else {
            action.sub_actions.clone()
        };

        let run = if action.concurrent {
            self.chain_engine
                .run_concurrent(&pick, &arg_stack, start_event_id, cancel)
                .await
        } else {
            self.chain_engine
                .run_sequential(&pick, &arg_stack, start_event_id, cancel)
                .await
        };

        ctx.telemetry = run.telemetry;
        ctx.outcome = match run.signal {
            ChainSignal::Completed | ChainSignal::Break | ChainSignal::Continue => {
                ExecutionOutcome::Success
            }
            ChainSignal::Stop(mark) if mark.failed => {
                ExecutionOutcome::Failed(mark.reason.unwrap_or_else(|| "stopped".to_owned()))
            }
            ChainSignal::Stop(_) => ExecutionOutcome::Success,
            ChainSignal::Error(msg) => ExecutionOutcome::Failed(msg),
            ChainSignal::Aborted => ExecutionOutcome::Cancelled,
        };

        // A cancel landing after the chain's last boundary check still makes this a cancelled run.
        if cancel.is_cancelled() {
            ctx.outcome = ExecutionOutcome::Cancelled;
        }

        ctx.completed_at = Some(OffsetDateTime::now_utc());

        let total_ms: u64 = ctx
            .telemetry
            .iter()
            .filter(|t| !t.is_nested())
            .map(|t| t.duration_ms)
            .sum();
        let outcome_label = match &ctx.outcome {
            ExecutionOutcome::Success => "success",
            ExecutionOutcome::Failed(_) => "failed",
            ExecutionOutcome::Cancelled => "cancelled",
        };

        // A cancelled run records to history but emits no completion event onto the bus.
        if !matches!(ctx.outcome, ExecutionOutcome::Cancelled) {
            self.bus.publish(Event::caused_by(
                EventSource::Core,
                "action.done",
                json!({
                    "action_id": action.id.to_string(),
                    "outcome": outcome_label,
                    "total_ms": total_ms,
                }),
                start_event_id,
            ));
        }

        if let Err(e) = self.history.save(&ctx).await {
            warn!("history_repo.save failed: {e}");
        }

        let telemetry_status = match &ctx.outcome {
            ExecutionOutcome::Success => Some(ExecutionStatus::Success),
            ExecutionOutcome::Failed(_) => Some(ExecutionStatus::Error),
            ExecutionOutcome::Cancelled => None,
        };
        if let Some(status) = telemetry_status
            && let Err(e) = self
                .actions
                .record_execution(action.id, started_at, total_ms, status)
                .await
        {
            warn!("action_repo.record_execution failed: {e}");
        }
    }
}

struct CancelGuard {
    registry: Arc<ActionCancelRegistry>,
    action_id: ActionId,
    exec_id: u64,
}

impl Drop for CancelGuard {
    fn drop(&mut self) {
        self.registry.deregister(self.action_id, self.exec_id);
    }
}

async fn run_quick_action_loop(
    mut rx: mpsc::Receiver<QuickActionRequest>,
    mut stop: watch::Receiver<bool>,
    bus: Arc<EventBus>,
    history: Arc<dyn HistoryRepo>,
    sub_action_registry: Arc<SubActionRegistry>,
) {
    loop {
        tokio::select! {
            biased;
            Ok(_) = stop.wait_for(|stopped| *stopped) => return,
            req = rx.recv() => match req {
                Some(req) => {
                    tokio::spawn(run_quick_action(
                        req,
                        Arc::clone(&bus),
                        Arc::clone(&history),
                        Arc::clone(&sub_action_registry),
                    ));
                }
                None => return,
            },
        }
    }
}

async fn run_quick_action(
    req: QuickActionRequest,
    bus: Arc<EventBus>,
    history: Arc<dyn HistoryRepo>,
    sub_action_registry: Arc<SubActionRegistry>,
) {
    let publisher: Arc<dyn forge_events::EventPublisher> =
        Arc::clone(&bus) as Arc<dyn forge_events::EventPublisher>;
    let run_payload = json!({ "step_index": 0, "kind": req.step.kind_id });
    let run_event = match req.caused_by {
        Some(parent) => Event::caused_by(EventSource::Core, "subaction.run", run_payload, parent),
        None => Event::new(EventSource::Core, "subaction.run", run_payload),
    };
    let run_event_id = run_event.id;
    bus.publish(run_event);

    let stack = ArgStack::new();
    let run_ctx = RunContext::leaf(&stack, 0, run_event_id, publisher.as_ref());

    let started_at = OffsetDateTime::now_utc();
    let (mut telemetry, produced_stack) = match sub_action_registry.get(&req.step.kind_id) {
        Some(runner) => {
            let resolved = effective_config(&runner.default_config(), &req.step.config);
            runner.execute(&resolved, &run_ctx).await
        }
        None => {
            warn!(
                "unknown sub-action kind_id: {} - skipping step",
                req.step.kind_id
            );
            (skipped_telemetry(0, &req.step.kind_id), None)
        }
    };
    let completed_at = OffsetDateTime::now_utc();

    telemetry.args_in = crate::chain::capture_args_in(&sub_action_registry, &req.step, &stack);
    telemetry.produced = match &produced_stack {
        Some(after) => crate::chain::capture_produced(&stack, after),
        None => ::std::collections::BTreeMap::new(),
    };

    crate::chain::publish_subaction_done(publisher.as_ref(), run_event_id, &telemetry);

    let outcome = match &telemetry.outcome {
        SubActionOutcome::Success => "success",
        SubActionOutcome::Failed(_) => "failed",
        SubActionOutcome::Skipped(_) => "skipped",
    };

    bus.publish(Event::caused_by(
        EventSource::Core,
        "action.quick.done",
        json!({
            "kind": telemetry.kind,
            "outcome": outcome,
            "label": req.label,
            "builtin_id": req.builtin_id,
        }),
        run_event_id,
    ));

    let run_outcome = match &telemetry.outcome {
        SubActionOutcome::Success | SubActionOutcome::Skipped(_) => ExecutionOutcome::Success,
        SubActionOutcome::Failed(message) => ExecutionOutcome::Failed(message.clone()),
    };
    let reported = telemetry.outcome.clone();
    let ctx = ExecutionContext {
        action_id: ActionId::new(),
        metadata: ExecutionMetadata::QuickAction {
            builtin_id: req.builtin_id.clone(),
            label: req.label.clone(),
        },
        arg_stack_snapshot: stack.snapshot(),
        started_at,
        completed_at: Some(completed_at),
        telemetry: vec![telemetry],
        outcome: run_outcome,
    };
    if let Err(e) = history.save(&ctx).await {
        warn!("history_repo.save failed: {e}");
    }
    let _ = req.outcome.send(reported);
}

pub(crate) fn skipped_telemetry(index: usize, kind_id: &str) -> SubActionTelemetry {
    SubActionTelemetry {
        args_in: ::std::collections::BTreeMap::new(),
        produced: ::std::collections::BTreeMap::new(),
        index,
        kind: kind_id.to_owned(),
        started_at: OffsetDateTime::now_utc(),
        duration_ms: 0,
        outcome: SubActionOutcome::Skipped(format!("unknown kind_id: {kind_id}")),
    }
}

pub(crate) fn disabled_telemetry(index: usize, kind_id: &str) -> SubActionTelemetry {
    SubActionTelemetry {
        args_in: ::std::collections::BTreeMap::new(),
        produced: ::std::collections::BTreeMap::new(),
        index,
        kind: kind_id.to_owned(),
        started_at: OffsetDateTime::now_utc(),
        duration_ms: 0,
        outcome: SubActionOutcome::Skipped("disabled".to_owned()),
    }
}

pub(crate) fn condition_skipped_telemetry(index: usize, kind_id: &str) -> SubActionTelemetry {
    SubActionTelemetry {
        args_in: ::std::collections::BTreeMap::new(),
        produced: ::std::collections::BTreeMap::new(),
        index,
        kind: kind_id.to_owned(),
        started_at: OffsetDateTime::now_utc(),
        duration_ms: 0,
        outcome: SubActionOutcome::Skipped("condition".to_owned()),
    }
}

pub(crate) fn condition_failed_telemetry(
    index: usize,
    kind_id: &str,
    message: String,
) -> SubActionTelemetry {
    SubActionTelemetry {
        args_in: ::std::collections::BTreeMap::new(),
        produced: ::std::collections::BTreeMap::new(),
        index,
        kind: kind_id.to_owned(),
        started_at: OffsetDateTime::now_utc(),
        duration_ms: 0,
        outcome: SubActionOutcome::Failed(message),
    }
}

pub fn spawn_action_engine(
    bus: Arc<EventBus>,
    actions: Arc<dyn ActionRepo>,
    history: Arc<dyn HistoryRepo>,
    sub_action_registry: Arc<SubActionRegistry>,
    cancel_registry: Arc<ActionCancelRegistry>,
) -> ActionEngineHandle {
    ActionEngine::spawn(bus, actions, history, sub_action_registry, cancel_registry)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Mutex;
    use std::time::Duration;

    use async_trait::async_trait;
    use forge_registry::{FormField, RegistryError, SubActionCategory};
    use forge_storage::DataProvider;
    use forge_types::Variant;

    use super::*;
    use crate::NullEventLogRepo;
    use crate::test_support::sandboxed_backend;

    struct RecordingRunner {
        last_config: Arc<Mutex<Option<forge_registry::SubActionConfig>>>,
    }

    #[async_trait]
    impl forge_registry::SubActionRunner for RecordingRunner {
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
        fn validate_config(
            &self,
            _: &forge_registry::SubActionConfig,
        ) -> Result<(), RegistryError> {
            Ok(())
        }
        async fn execute(
            &self,
            config: &forge_registry::SubActionConfig,
            _ctx: &RunContext<'_>,
        ) -> (SubActionTelemetry, Option<ArgStack>) {
            *self.last_config.lock().unwrap() = Some(config.clone());
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

    struct FixedOutcomeRunner {
        id: String,
        outcome: SubActionOutcome,
    }

    #[async_trait]
    impl forge_registry::SubActionRunner for FixedOutcomeRunner {
        fn id(&self) -> &str {
            &self.id
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
        fn default_config(&self) -> forge_registry::SubActionConfig {
            BTreeMap::new()
        }
        fn config_fields(&self) -> Vec<FormField> {
            Vec::new()
        }
        fn validate_config(
            &self,
            _: &forge_registry::SubActionConfig,
        ) -> Result<(), RegistryError> {
            Ok(())
        }
        async fn execute(
            &self,
            _: &forge_registry::SubActionConfig,
            ctx: &RunContext<'_>,
        ) -> (SubActionTelemetry, Option<ArgStack>) {
            (
                SubActionTelemetry {
                    args_in: ::std::collections::BTreeMap::new(),
                    produced: ::std::collections::BTreeMap::new(),
                    index: ctx.index,
                    kind: self.id.clone(),
                    started_at: OffsetDateTime::now_utc(),
                    duration_ms: 0,
                    outcome: self.outcome.clone(),
                },
                None,
            )
        }
    }

    #[tokio::test]
    async fn chain_signal_maps_to_recorded_execution_outcome() {
        let dp = sandboxed_backend([0xcd; 32])
            .await
            .map(|backend| Arc::new(backend) as Arc<dyn DataProvider>);
        let bus = EventBus::new(Arc::new(NullEventLogRepo));

        let mut reg = SubActionRegistry::new();
        reg.register(Box::new(FixedOutcomeRunner {
            id: "map.success".to_owned(),
            outcome: SubActionOutcome::Success,
        }))
        .unwrap();
        reg.register(Box::new(FixedOutcomeRunner {
            id: "map.failure".to_owned(),
            outcome: SubActionOutcome::Failed("kaboom".to_owned()),
        }))
        .unwrap();
        let engine = spawn_action_engine(
            Arc::clone(&bus),
            dp.action_repo(),
            dp.history_repo(),
            Arc::new(reg),
            Arc::new(crate::action_cancel::ActionCancelRegistry::new()),
        );

        let cases = [
            ("map.success", ExecutionOutcome::Success),
            ("map.failure", ExecutionOutcome::Failed("kaboom".to_owned())),
        ];

        let default_queue: forge_types::QueueId =
            serde_json::from_str("\"00000000000000000000000000\"").unwrap();

        for (kind, expected) in cases {
            let action_id = ActionId::new();
            let action = forge_types::Action {
                id: action_id,
                name: "map".to_owned(),
                group: None,
                queue_id: default_queue,
                enabled: true,
                concurrent: false,
                bypass_pause: false,
                execution_mode: forge_types::ExecutionMode::Sequential,
                description: None,
                sub_actions: vec![SubActionStep {
                    kind_id: kind.to_owned(),
                    config: BTreeMap::new(),
                    enabled: true,
                    continue_on_error: false,
                    condition: None,
                    label: None,
                }],
            };
            dp.action_repo().save(&action).await.unwrap();

            engine
                .dispatch(ExecutionRequest {
                    action_id,
                    trigger_event_id: EventId::new(),
                    trigger_kind: None,
                    initial_args: ArgStack::new(),
                })
                .await
                .unwrap();

            let mut recorded = None;
            for _ in 0..40 {
                let recent = dp
                    .history_repo()
                    .recent_for_action(action_id, 1)
                    .await
                    .unwrap();
                if let Some(ctx) = recent.into_iter().next() {
                    assert!(
                        matches!(ctx.metadata, ExecutionMetadata::Trigger { .. }),
                        "trigger-dispatched runs must record Trigger metadata, got {:?}",
                        ctx.metadata
                    );
                    recorded = Some(ctx.outcome);
                    break;
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
            assert_eq!(
                recorded.expect("no history recorded"),
                expected,
                "kind={kind}"
            );
        }
    }

    #[tokio::test]
    async fn quick_action_resolves_effective_config_before_execute() {
        let dp = sandboxed_backend([0xab; 32])
            .await
            .map(|backend| Arc::new(backend) as Arc<dyn DataProvider>);
        let bus = EventBus::new(Arc::new(NullEventLogRepo));

        let last = Arc::new(Mutex::new(None));
        let runner = Box::new(RecordingRunner {
            last_config: Arc::clone(&last),
        });
        let mut reg = SubActionRegistry::new();
        reg.register(runner).unwrap();
        let engine = spawn_action_engine(
            Arc::clone(&bus),
            dp.action_repo(),
            dp.history_repo(),
            Arc::new(reg),
            Arc::new(crate::action_cancel::ActionCancelRegistry::new()),
        );

        let mut overrides = BTreeMap::new();
        overrides.insert("a".to_owned(), Variant::Int(99));

        let step = SubActionStep {
            kind_id: "test.record".to_owned(),
            config: overrides,
            enabled: true,
            continue_on_error: false,
            condition: None,
            label: None,
        };

        engine
            .execute_quick_action(step, "test.record".to_owned(), "Test".to_owned(), None)
            .await
            .unwrap();

        for _ in 0..40 {
            if last.lock().unwrap().is_some() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }

        let captured = last.lock().unwrap().clone().expect("runner not called");
        assert_eq!(captured.get("a"), Some(&Variant::Int(99)));
        assert_eq!(captured.get("b"), Some(&Variant::Int(2)));
    }

    #[tokio::test]
    async fn quick_path_emits_action_quick_done_and_links_subaction_run_causation() {
        let dp = sandboxed_backend([0x3c; 32])
            .await
            .map(|backend| Arc::new(backend) as Arc<dyn DataProvider>);
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let engine = spawn_action_engine(
            Arc::clone(&bus),
            dp.action_repo(),
            dp.history_repo(),
            Arc::new(SubActionRegistry::new()),
            Arc::new(crate::action_cancel::ActionCancelRegistry::new()),
        );

        let parent = EventId::new();
        for expected_parent in [Some(parent), None] {
            let mut sub = bus.subscribe();
            let step = SubActionStep {
                kind_id: "quick.probe".to_owned(),
                config: BTreeMap::new(),
                enabled: true,
                continue_on_error: false,
                condition: None,
                label: None,
            };

            engine
                .execute_quick_action(
                    step,
                    "twitch".to_owned(),
                    "Probe".to_owned(),
                    expected_parent,
                )
                .await
                .unwrap();

            let mut run_event = None;
            let mut done_event = None;
            for _ in 0..40 {
                match tokio::time::timeout(Duration::from_millis(50), sub.recv()).await {
                    Ok(Ok(ev)) if ev.kind == "subaction.run" => run_event = Some(ev),
                    Ok(Ok(ev)) if ev.kind == "action.quick.done" => done_event = Some(ev),
                    Ok(Ok(_)) => {}
                    _ => break,
                }
                if run_event.is_some() && done_event.is_some() {
                    break;
                }
            }

            let run_event = run_event.expect("quick path must emit subaction.run");
            let done_event = done_event.expect("quick path must emit action.quick.done");

            assert_eq!(
                run_event.caused_by, expected_parent,
                "subaction.run must link caused_by to the quick-action parent"
            );
            assert_eq!(
                done_event.caused_by,
                Some(run_event.id),
                "action.quick.done must chain from its subaction.run"
            );
        }
    }

    fn capturing_history() -> (
        Arc<dyn HistoryRepo>,
        tokio::sync::mpsc::UnboundedReceiver<ExecutionContext>,
    ) {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let mut mock = forge_storage::history::MockHistoryRepo::new();
        mock.expect_save().times(1..).returning(move |ctx| {
            let _ = tx.send(ctx.clone());
            Ok(())
        });
        (Arc::new(mock), rx)
    }

    async fn recv_ctx(
        rx: &mut tokio::sync::mpsc::UnboundedReceiver<ExecutionContext>,
    ) -> ExecutionContext {
        tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .expect("history save did not arrive in time")
            .expect("history channel closed")
    }

    fn quick_step(kind_id: &str, config: BTreeMap<String, Variant>) -> SubActionStep {
        SubActionStep {
            kind_id: kind_id.to_owned(),
            config,
            enabled: true,
            continue_on_error: false,
            condition: None,
            label: None,
        }
    }

    #[tokio::test]
    async fn quick_action_run_maps_subaction_outcome_to_execution_outcome() {
        let dp = sandboxed_backend([0x51; 32])
            .await
            .map(|backend| Arc::new(backend) as Arc<dyn DataProvider>);
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let (history, mut rx) = capturing_history();

        let mut reg = SubActionRegistry::new();
        reg.register(Box::new(FixedOutcomeRunner {
            id: "quick.ok".to_owned(),
            outcome: SubActionOutcome::Success,
        }))
        .unwrap();
        reg.register(Box::new(FixedOutcomeRunner {
            id: "quick.fail".to_owned(),
            outcome: SubActionOutcome::Failed("boom".to_owned()),
        }))
        .unwrap();
        reg.register(Box::new(FixedOutcomeRunner {
            id: "quick.skip".to_owned(),
            outcome: SubActionOutcome::Skipped("nope".to_owned()),
        }))
        .unwrap();

        let engine = spawn_action_engine(
            Arc::clone(&bus),
            dp.action_repo(),
            history,
            Arc::new(reg),
            Arc::new(crate::action_cancel::ActionCancelRegistry::new()),
        );

        let cases = [
            ("quick.ok", ExecutionOutcome::Success),
            ("quick.fail", ExecutionOutcome::Failed("boom".to_owned())),
            ("quick.skip", ExecutionOutcome::Success),
        ];
        for (kind, expected) in cases {
            engine
                .execute_quick_action(
                    quick_step(kind, BTreeMap::new()),
                    "obs".to_owned(),
                    kind.to_owned(),
                    None,
                )
                .await
                .unwrap();
            let ctx = recv_ctx(&mut rx).await;
            assert_eq!(ctx.outcome, expected, "kind={kind}");
        }
    }

    #[tokio::test]
    async fn quick_action_run_records_quick_action_metadata() {
        let dp = sandboxed_backend([0x52; 32])
            .await
            .map(|backend| Arc::new(backend) as Arc<dyn DataProvider>);
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let (history, mut rx) = capturing_history();
        let engine = spawn_action_engine(
            Arc::clone(&bus),
            dp.action_repo(),
            history,
            Arc::new(SubActionRegistry::new()),
            Arc::new(crate::action_cancel::ActionCancelRegistry::new()),
        );

        engine
            .execute_quick_action(
                quick_step("quick.probe", BTreeMap::new()),
                "twitch".to_owned(),
                "Set title".to_owned(),
                None,
            )
            .await
            .unwrap();

        let ctx = recv_ctx(&mut rx).await;
        assert!(
            matches!(
                &ctx.metadata,
                ExecutionMetadata::QuickAction { builtin_id, label }
                    if builtin_id == "twitch" && label == "Set title"
            ),
            "expected QuickAction(twitch, 'Set title') metadata, got {:?}",
            ctx.metadata
        );
    }

    #[tokio::test]
    async fn quick_action_runs_receive_distinct_action_ids() {
        let dp = sandboxed_backend([0x53; 32])
            .await
            .map(|backend| Arc::new(backend) as Arc<dyn DataProvider>);
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let (history, mut rx) = capturing_history();
        let engine = spawn_action_engine(
            Arc::clone(&bus),
            dp.action_repo(),
            history,
            Arc::new(SubActionRegistry::new()),
            Arc::new(crate::action_cancel::ActionCancelRegistry::new()),
        );

        engine
            .execute_quick_action(
                quick_step("quick.probe", BTreeMap::new()),
                "obs".to_owned(),
                "Run".to_owned(),
                None,
            )
            .await
            .unwrap();
        let first = recv_ctx(&mut rx).await;
        engine
            .execute_quick_action(
                quick_step("quick.probe", BTreeMap::new()),
                "obs".to_owned(),
                "Run".to_owned(),
                None,
            )
            .await
            .unwrap();
        let second = recv_ctx(&mut rx).await;

        assert_ne!(first.action_id, second.action_id);
    }

    #[tokio::test]
    async fn quick_action_history_captures_merged_config_as_args_in() {
        let dp = sandboxed_backend([0x54; 32])
            .await
            .map(|backend| Arc::new(backend) as Arc<dyn DataProvider>);
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let (history, mut rx) = capturing_history();

        let mut reg = SubActionRegistry::new();
        reg.register(Box::new(RecordingRunner {
            last_config: Arc::new(Mutex::new(None)),
        }))
        .unwrap();
        let engine = spawn_action_engine(
            Arc::clone(&bus),
            dp.action_repo(),
            history,
            Arc::new(reg),
            Arc::new(crate::action_cancel::ActionCancelRegistry::new()),
        );

        let mut overrides = BTreeMap::new();
        overrides.insert("a".to_owned(), Variant::Int(99));
        engine
            .execute_quick_action(
                quick_step("test.record", overrides),
                "obs".to_owned(),
                "Rec".to_owned(),
                None,
            )
            .await
            .unwrap();

        let ctx = recv_ctx(&mut rx).await;
        let args_in = &ctx.telemetry[0].args_in;
        assert_eq!(args_in.get("a"), Some(&"99".to_owned()));
        assert_eq!(args_in.get("b"), Some(&"2".to_owned()));
    }

    const LOOP_EVENT_KIND: &str = "test.loop_event";

    struct EmitRunner;

    #[async_trait]
    impl forge_registry::SubActionRunner for EmitRunner {
        fn id(&self) -> &str {
            "test.emit"
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
        fn default_config(&self) -> forge_registry::SubActionConfig {
            BTreeMap::new()
        }
        fn config_fields(&self) -> Vec<FormField> {
            Vec::new()
        }
        fn validate_config(
            &self,
            _: &forge_registry::SubActionConfig,
        ) -> Result<(), RegistryError> {
            Ok(())
        }
        async fn execute(
            &self,
            _: &forge_registry::SubActionConfig,
            ctx: &RunContext<'_>,
        ) -> (SubActionTelemetry, Option<ArgStack>) {
            ctx.publisher.publish(Event::caused_by(
                EventSource::Core,
                LOOP_EVENT_KIND,
                serde_json::Value::Null,
                ctx.parent_event_id,
            ));
            (
                SubActionTelemetry {
                    args_in: BTreeMap::new(),
                    produced: BTreeMap::new(),
                    index: ctx.index,
                    kind: "test.emit".to_owned(),
                    started_at: OffsetDateTime::now_utc(),
                    duration_ms: 0,
                    outcome: SubActionOutcome::Success,
                },
                None,
            )
        }
    }

    struct SelfLoopRig {
        dp: crate::test_support::Sandboxed<Arc<dyn DataProvider>>,
        bus: Arc<EventBus>,
        engine: ActionEngineHandle,
        action_id: ActionId,
    }

    async fn self_loop_rig() -> SelfLoopRig {
        let dp = sandboxed_backend([0x16; 32])
            .await
            .map(|backend| Arc::new(backend) as Arc<dyn DataProvider>);
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let mut reg = SubActionRegistry::new();
        reg.register(Box::new(EmitRunner)).unwrap();
        let engine = spawn_action_engine(
            Arc::clone(&bus),
            dp.action_repo(),
            dp.history_repo(),
            Arc::new(reg),
            Arc::new(crate::action_cancel::ActionCancelRegistry::new()),
        );
        let action_id = ActionId::new();
        let action = forge_types::Action {
            id: action_id,
            name: "echo".to_owned(),
            group: None,
            queue_id: serde_json::from_str("\"00000000000000000000000000\"").unwrap(),
            enabled: true,
            concurrent: false,
            bypass_pause: false,
            execution_mode: forge_types::ExecutionMode::Sequential,
            description: None,
            sub_actions: vec![SubActionStep {
                kind_id: "test.emit".to_owned(),
                config: BTreeMap::new(),
                enabled: true,
                continue_on_error: false,
                condition: None,
                label: None,
            }],
        };
        dp.action_repo().save(&action).await.unwrap();
        SelfLoopRig {
            dp,
            bus,
            engine,
            action_id,
        }
    }

    fn platform_event(bus: &EventBus) -> EventId {
        let event = Event::new(EventSource::Twitch, "twitch.chat", serde_json::Value::Null);
        let id = event.id;
        bus.publish(event);
        id
    }

    async fn run_to_completion(rig: &SelfLoopRig, trigger_event_id: EventId) {
        let (done_tx, done_rx) = oneshot::channel();
        rig.engine
            .dispatch_tracked(
                ExecutionRequest {
                    action_id: rig.action_id,
                    trigger_event_id,
                    trigger_kind: None,
                    initial_args: ArgStack::new(),
                },
                CancelSignal::new(),
                done_tx,
            )
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(5), done_rx)
            .await
            .expect("run did not finish")
            .unwrap();
    }

    struct LoopTrace {
        starts: usize,
        skipped: Option<Event>,
        last_trigger: EventId,
    }

    // Plays the trigger evaluator: every loop event the action emits dispatches the same action again.
    async fn drive_self_loop(rig: &SelfLoopRig, root: EventId) -> LoopTrace {
        let mut sub = rig.bus.subscribe();
        let mut trace = LoopTrace {
            starts: 0,
            skipped: None,
            last_trigger: root,
        };
        for _ in 0..(MAX_CAUSATION_DEPTH * 2) {
            run_to_completion(rig, trace.last_trigger).await;
            let mut next = None;
            while let Ok(Some(event)) = sub.try_recv() {
                match event.kind.as_str() {
                    ACTION_START_KIND => trace.starts += 1,
                    LOOP_EVENT_KIND => next = Some(event.id),
                    "action.skipped" => trace.skipped = Some(event),
                    _ => {}
                }
            }
            match next {
                Some(id) if trace.skipped.is_none() => trace.last_trigger = id,
                _ => break,
            }
        }
        trace
    }

    #[tokio::test]
    async fn action_retriggered_by_its_own_event_runs_exactly_the_depth_limit_times() {
        let rig = self_loop_rig().await;
        let root = platform_event(&rig.bus);

        let trace = drive_self_loop(&rig, root).await;

        assert_eq!(trace.starts, MAX_CAUSATION_DEPTH);
    }

    #[tokio::test]
    async fn run_refused_for_depth_publishes_a_skip_linked_to_its_trigger() {
        let rig = self_loop_rig().await;
        let root = platform_event(&rig.bus);

        let trace = drive_self_loop(&rig, root).await;

        let skipped = trace.skipped.expect("no action.skipped published");
        assert_eq!(
            (
                skipped.payload["reason"].as_str(),
                skipped.payload["action_id"].as_str(),
                skipped.caused_by,
            ),
            (
                Some(CAUSATION_DEPTH_SKIP_REASON),
                Some(rig.action_id.to_string().as_str()),
                Some(trace.last_trigger),
            )
        );
    }

    #[tokio::test]
    async fn run_refused_for_depth_is_recorded_as_one_failed_history_entry() {
        let rig = self_loop_rig().await;
        let root = platform_event(&rig.bus);

        let trace = drive_self_loop(&rig, root).await;

        let history = rig
            .dp
            .history_repo()
            .recent_for_action(rig.action_id, 100)
            .await
            .unwrap();
        let failed: Vec<_> = history
            .iter()
            .filter(|ctx| matches!(ctx.outcome, ExecutionOutcome::Failed(_)))
            .map(|ctx| match &ctx.metadata {
                ExecutionMetadata::Trigger { event_id, .. } => Some(*event_id),
                _ => None,
            })
            .collect();
        assert_eq!(
            (history.len(), failed),
            (MAX_CAUSATION_DEPTH + 1, vec![Some(trace.last_trigger)])
        );
    }

    #[tokio::test]
    async fn fresh_platform_event_after_a_refused_loop_starts_the_action_again() {
        let rig = self_loop_rig().await;
        let first_root = platform_event(&rig.bus);
        drive_self_loop(&rig, first_root).await;
        let mut sub = rig.bus.subscribe();

        run_to_completion(&rig, platform_event(&rig.bus)).await;

        let mut started = false;
        while let Ok(Some(event)) = sub.try_recv() {
            started |= event.kind == ACTION_START_KIND;
        }
        assert!(
            started,
            "an unrelated platform event must not inherit the refused loop's depth"
        );
    }
}
