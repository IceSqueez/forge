use crate::engine::chain::skipped_telemetry;
use crate::error::RuntimeError;
use std::sync::Arc;
use std::time::Duration;

use forge_events::{
    ActionDone, ActionSkipped, ActionStarted, EventPublisher, EventSource, QuickActionDone,
    RunOutcome, SubActionRun, TypedEvent,
};
use forge_net::{TaskOwner, TaskShutdown};
use forge_registry::{
    CancelSignal, ChainSignal, ControlCell, RunContext, RunningAction, StopMark, SubActionRegistry,
    TelemetrySink,
};
use forge_storage::{ActionExecution, ActionRepo, ExecutionStatus, HistoryRepo, RunRefusal};
use forge_types::{
    Action, ActionId, ArgStack, DEFAULT_QUEUE_ID, EventId, ExecutionContext, ExecutionMetadata,
    ExecutionOutcome, MessageRef, NamesLanguage, QueueId, RehearsalKind, SubActionOutcome,
    SubActionStep, UserText, VerbatimText,
};
use time::OffsetDateTime;
use tokio::sync::{mpsc, oneshot};
use tracing::{info, warn};

use crate::core_messages;
use crate::engine::action_cancel::ActionCancelRegistry;
use crate::engine::chain::{ChainEngine, pick_steps};
use crate::engine::run_history::RunHistoryWriter;
use crate::engine::scheduled_runs::ScheduledOrigin;
use crate::event_flow::bus::LineageHold;
use crate::integrations::integration_gate::{GatedLeafExecutor, IntegrationGate};
use crate::services::catalog::Catalog;
use crate::services::date_names::DateNamesHandle;
use crate::{Config, EventBus};

const EXECUTION_INTAKE_CAPACITY: usize = 256;
const CAUSATION_DEPTH_SKIP_REASON: &str = "causation_depth_exceeded";
const QUICK_ACTION_INTAKE_CAPACITY: usize = 64;
const MAX_CAUSATION_DEPTH: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuickActionOrigin {
    pub builtin_id: String,
    pub label: String,
}

struct QuickActionRequest {
    step: SubActionStep,
    builtin_id: String,
    label: String,
    caused_by: Option<EventId>,
    outcome: oneshot::Sender<SubActionOutcome>,
}

pub struct PendingQuickAction(oneshot::Receiver<SubActionOutcome>);

impl PendingQuickAction {
    pub async fn outcome(self) -> Result<SubActionOutcome, RuntimeError> {
        self.0.await.map_err(|_| RuntimeError::EngineNoOutcome)
    }
}

#[derive(Clone)]
pub struct ActionEngineHandle {
    sender: mpsc::Sender<EngineJob>,
    quick_sender: mpsc::Sender<QuickActionRequest>,
    tasks: TaskOwner,
    integrations: IntegrationGate,
}

pub struct ExecutionRequest {
    pub action_id: ActionId,
    pub trigger_event_id: EventId,
    pub trigger_kind: Option<String>,
    pub manual_test: bool,
    pub initial_args: forge_types::ArgStack,
}

struct EngineJob {
    request: ExecutionRequest,
    origin: Option<ScheduledOrigin>,
    cancel: CancelSignal,
    on_complete: Option<oneshot::Sender<()>>,
}

impl ActionEngineHandle {
    pub async fn dispatch(&self, req: ExecutionRequest) -> Result<(), RuntimeError> {
        self.sender
            .send(EngineJob {
                request: req,
                origin: None,
                cancel: CancelSignal::new(),
                on_complete: None,
            })
            .await
            .map_err(|_| RuntimeError::EngineChannelClosed)
    }

    pub(crate) async fn dispatch_tracked(
        &self,
        req: ExecutionRequest,
        cancel: CancelSignal,
        on_complete: oneshot::Sender<()>,
    ) -> Result<(), RuntimeError> {
        self.send_tracked(req, None, cancel, on_complete).await
    }

    pub(crate) async fn dispatch_scheduled_tracked(
        &self,
        req: ExecutionRequest,
        origin: ScheduledOrigin,
        cancel: CancelSignal,
        on_complete: oneshot::Sender<()>,
    ) -> Result<(), RuntimeError> {
        self.send_tracked(req, Some(origin), cancel, on_complete)
            .await
    }

    async fn send_tracked(
        &self,
        req: ExecutionRequest,
        origin: Option<ScheduledOrigin>,
        cancel: CancelSignal,
        on_complete: oneshot::Sender<()>,
    ) -> Result<(), RuntimeError> {
        self.sender
            .send(EngineJob {
                request: req,
                origin,
                cancel,
                on_complete: Some(on_complete),
            })
            .await
            .map_err(|_| RuntimeError::EngineChannelClosed)
    }

    pub async fn execute_quick_action(
        &self,
        step: SubActionStep,
        origin: QuickActionOrigin,
        caused_by: Option<EventId>,
    ) -> Result<PendingQuickAction, RuntimeError> {
        let QuickActionOrigin { builtin_id, label } = origin;
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
            .map_err(|_| RuntimeError::EngineChannelClosed)?;
        Ok(PendingQuickAction(pending))
    }

    pub fn integration_gate(&self) -> &IntegrationGate {
        &self.integrations
    }

    pub fn shutdown(&self) {
        self.tasks.request_stop();
    }

    pub async fn stopped(&self, grace: Duration) -> TaskShutdown {
        self.tasks.shutdown(grace).await
    }
}

struct ActionEngine {
    bus: Arc<EventBus>,
    catalog: Arc<Catalog>,
    actions: Arc<dyn ActionRepo>,
    history: RunHistoryWriter,
    chain_engine: Arc<ChainEngine>,
    cancel_registry: Arc<ActionCancelRegistry>,
    tasks: TaskOwner,
}

impl ActionEngine {
    fn spawn(parts: ActionEngineParts) -> ActionEngineHandle {
        let ActionEngineParts {
            bus,
            catalog,
            actions,
            history,
            sub_action_registry,
            cancel_registry,
            date_names,
            config,
        } = parts;
        let (tx, rx) = mpsc::channel(EXECUTION_INTAKE_CAPACITY);
        let (quick_tx, quick_rx) = mpsc::channel(QUICK_ACTION_INTAKE_CAPACITY);
        let tasks = TaskOwner::new();
        let gate = Arc::new(crate::engine::condition::ConditionGate::new(&config));
        let chain_engine = Arc::new(ChainEngine::on_bus(
            Arc::clone(&sub_action_registry),
            Arc::clone(&bus),
            gate,
            date_names.clone(),
            config,
        ));
        let integrations = chain_engine.integration_gate().clone();
        let history = RunHistoryWriter::spawn(&bus, history, Arc::clone(&actions));
        let engine = Arc::new(Self {
            bus: Arc::clone(&bus),
            catalog,
            actions,
            history: history.clone(),
            chain_engine,
            cancel_registry,
            tasks: tasks.clone(),
        });
        tasks.spawn(Self::run(engine, rx));
        tasks.spawn(run_quick_action_loop(
            quick_rx,
            tasks.clone(),
            bus,
            history,
            sub_action_registry,
            integrations.clone(),
            date_names,
        ));
        ActionEngineHandle {
            sender: tx,
            quick_sender: quick_tx,
            tasks,
            integrations,
        }
    }

    async fn run(engine: Arc<Self>, mut input: mpsc::Receiver<EngineJob>) {
        loop {
            tokio::select! {
                biased;
                () = engine.tasks.stopped() => {
                    let cancelled = engine.cancel_registry.cancel_all();
                    info!(cancelled, "action engine stopped");
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
            origin,
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
        self.tasks.spawn(async move {
            engine.run_execution(request, origin, &cancel).await;
            drop(cancel_guard);
            if let Some(done) = on_complete {
                #[expect(
                    clippy::let_underscore_must_use,
                    reason = "the caller may stop waiting for completion"
                )]
                let _ = done.send(());
            }
        });
    }

    async fn refuse_unrunnable(&self, req: ExecutionRequest, origin: Option<ScheduledOrigin>) {
        let (reason, queue_id, failure) = match self.actions.get(req.action_id).await {
            Ok(Some(action)) => (
                RunRefusal::ActionDisabled,
                action.queue_id,
                core_messages::action_disabled(),
            ),
            Ok(None) => (
                RunRefusal::ActionNotFound,
                DEFAULT_QUEUE_ID,
                core_messages::unknown_action(req.action_id),
            ),
            Err(e) => (
                RunRefusal::ActionsUnavailable,
                DEFAULT_QUEUE_ID,
                core_messages::storage_failed(e),
            ),
        };
        self.refuse(req, origin, reason, queue_id, failure);
    }

    fn refuse(
        &self,
        req: ExecutionRequest,
        origin: Option<ScheduledOrigin>,
        reason: RunRefusal,
        queue_id: QueueId,
        failure: MessageRef,
    ) {
        warn!(action = %req.action_id, %reason, "action run refused");
        self.bus.publish(
            ActionSkipped {
                action_id: req.action_id,
                reason: reason.code().to_owned(),
                queue_id,
            }
            .into_event(EventSource::Core, Some(req.trigger_event_id)),
        );
        let started_at = OffsetDateTime::now_utc();
        let ctx = ExecutionContext {
            action_id: req.action_id,
            metadata: self.run_metadata(origin, &req),
            arg_stack_snapshot: req.initial_args.snapshot(),
            started_at,
            completed_at: Some(started_at),
            telemetry: Vec::new(),
            outcome: ExecutionOutcome::Failed(failure.into()),
        };
        self.history.record(ctx, None);
    }

    fn refuse_too_deep(
        &self,
        action: &Action,
        trigger_event_id: EventId,
        metadata: ExecutionMetadata,
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
        self.bus.publish(
            ActionSkipped {
                action_id: action.id,
                reason: CAUSATION_DEPTH_SKIP_REASON.to_owned(),
                queue_id: action.queue_id,
            }
            .into_event(EventSource::Core, Some(trigger_event_id)),
        );
        let ctx = ExecutionContext {
            action_id: action.id,
            metadata,
            arg_stack_snapshot: arg_stack.snapshot(),
            started_at,
            completed_at: Some(started_at),
            telemetry: Vec::new(),
            outcome: ExecutionOutcome::Failed(
                core_messages::run_depth_exceeded(MAX_CAUSATION_DEPTH).into(),
            ),
        };
        self.history.record(ctx, None);
    }

    async fn run_execution(
        &self,
        req: ExecutionRequest,
        origin: Option<ScheduledOrigin>,
        cancel: &CancelSignal,
    ) {
        let action = match self.catalog.current().await {
            Ok(catalog) => match catalog.action(req.action_id) {
                Some(action) => Arc::clone(action),
                None => {
                    self.refuse_unrunnable(req, origin).await;
                    return;
                }
            },
            Err(e) => {
                warn!(error = %e, "action catalog rebuild failed");
                self.refuse(
                    req,
                    origin,
                    RunRefusal::ActionsUnavailable,
                    DEFAULT_QUEUE_ID,
                    core_messages::storage_failed(e),
                );
                return;
            }
        };

        let metadata = self.run_metadata(origin, &req);
        let arg_stack = req.initial_args;
        let started_at = OffsetDateTime::now_utc();
        let manual_test = req.manual_test;

        let trigger_depth = self.bus.causation_depth(req.trigger_event_id);
        if usize::from(trigger_depth) >= MAX_CAUSATION_DEPTH {
            self.refuse_too_deep(
                &action,
                req.trigger_event_id,
                metadata,
                arg_stack,
                started_at,
            );
            return;
        }

        let mut ctx = ExecutionContext {
            action_id: req.action_id,
            metadata,
            arg_stack_snapshot: arg_stack.snapshot(),
            started_at,
            completed_at: None,
            telemetry: Vec::new(),
            outcome: ExecutionOutcome::Success,
        };

        let (start_event_id, _lineage) =
            self.announce_start(&action, req.trigger_event_id, trigger_depth);

        let pick = pick_steps(&action);

        let running = RunningAction {
            action_id: action.id,
            start_event_id,
            trigger_event_id: req.trigger_event_id,
        };
        let run = self
            .chain_engine
            .run_action(running, &pick, &arg_stack, cancel)
            .await;

        ctx.telemetry = run.telemetry;
        ctx.outcome = if cancel.is_cancelled() {
            ExecutionOutcome::Cancelled
        } else {
            run_outcome(run.signal)
        };
        ctx.completed_at = Some(OffsetDateTime::now_utc());

        self.finish_run(&action, ctx, manual_test, start_event_id);
    }

    fn run_metadata(
        &self,
        origin: Option<ScheduledOrigin>,
        req: &ExecutionRequest,
    ) -> ExecutionMetadata {
        let event_id = req.trigger_event_id;
        let trigger_kind = req.trigger_kind.clone();
        if let Some(origin) = origin {
            return origin.into_metadata(event_id);
        }
        match self.rehearsal_of(req) {
            Some(rehearsal) => ExecutionMetadata::Rehearsal {
                event_id,
                trigger_kind,
                rehearsal,
            },
            None => ExecutionMetadata::Trigger {
                event_id,
                trigger_kind,
            },
        }
    }

    fn rehearsal_of(&self, req: &ExecutionRequest) -> Option<RehearsalKind> {
        if self.bus.is_replay(req.trigger_event_id) {
            Some(RehearsalKind::Replay)
        } else if req.manual_test
            || self
                .bus
                .lookup(req.trigger_event_id)
                .is_some_and(|event| event.is_rehearsal())
        {
            Some(RehearsalKind::TestRun)
        } else {
            None
        }
    }

    fn announce_start(
        &self,
        action: &Action,
        trigger_event_id: EventId,
        trigger_depth: u16,
    ) -> (EventId, LineageHold<'_>) {
        let mut start_event = ActionStarted {
            action_id: action.id,
            action_name: action.name.clone(),
            sub_action_count: action.sub_actions.len(),
        }
        .into_event(EventSource::Core, Some(trigger_event_id));
        start_event.causation_depth = trigger_depth.saturating_add(1);
        let start_event_id = start_event.id;
        let lineage = self.bus.hold_lineage(&start_event);
        self.bus.publish(start_event);
        (start_event_id, lineage)
    }

    fn finish_run(
        &self,
        action: &Action,
        ctx: ExecutionContext,
        manual_test: bool,
        start_event_id: EventId,
    ) {
        let total_ms: u64 = ctx
            .telemetry
            .iter()
            .filter(|t| !t.is_nested())
            .map(|t| t.duration_ms)
            .sum();

        let execution = match &ctx.outcome {
            ExecutionOutcome::Success => Some(ExecutionStatus::Success),
            ExecutionOutcome::Failed(_) => Some(ExecutionStatus::Error),
            ExecutionOutcome::Cancelled => None,
        }
        .filter(|_| !ctx.metadata.is_rehearsal())
        .map(|status| ActionExecution {
            action_id: action.id,
            started_at: ctx.started_at,
            duration_ms: total_ms,
            status,
        });
        let done = match &ctx.outcome {
            ExecutionOutcome::Success => Some((RunOutcome::Success, None)),
            ExecutionOutcome::Failed(reason) => Some((RunOutcome::Failed, Some(reason.clone()))),
            ExecutionOutcome::Cancelled => None,
        }
        .map(|(outcome, reason)| ActionDone {
            action_id: action.id,
            action_name: action.name.clone(),
            outcome,
            total_ms,
            manual_test,
            reason: reason.as_ref().map(UserText::code_or_text),
            reason_text: reason,
        });
        self.history.record(ctx, execution);

        if let Some(done) = done {
            self.bus
                .publish(done.into_event(EventSource::Core, Some(start_event_id)));
        }
    }
}

fn run_outcome(signal: ChainSignal) -> ExecutionOutcome {
    match signal {
        ChainSignal::Completed
        | ChainSignal::Break
        | ChainSignal::Continue
        | ChainSignal::Stop(StopMark::Completed) => ExecutionOutcome::Success,
        ChainSignal::Stop(StopMark::Failed { reason }) => ExecutionOutcome::Failed(match reason {
            Some(reason) => VerbatimText::new(reason).into(),
            None => core_messages::action_stopped().into(),
        }),
        ChainSignal::Error(msg) => ExecutionOutcome::Failed(msg),
        ChainSignal::Aborted => ExecutionOutcome::Cancelled,
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
    tasks: TaskOwner,
    bus: Arc<EventBus>,
    history: RunHistoryWriter,
    sub_action_registry: Arc<SubActionRegistry>,
    integrations: IntegrationGate,
    date_names: DateNamesHandle,
) {
    let cancel = CancelSignal::new();
    loop {
        tokio::select! {
            biased;
            () = tasks.stopped() => {
                cancel.cancel();
                return;
            }
            req = rx.recv() => match req {
                Some(req) => {
                    tasks.spawn(run_quick_action(
                        req,
                        cancel.clone(),
                        Arc::clone(&bus),
                        history.clone(),
                        Arc::clone(&sub_action_registry),
                        integrations.clone(),
                        date_names.language(),
                    ));
                }
                None => return,
            },
        }
    }
}

async fn run_quick_action(
    req: QuickActionRequest,
    cancel: CancelSignal,
    bus: Arc<EventBus>,
    history: RunHistoryWriter,
    sub_action_registry: Arc<SubActionRegistry>,
    integrations: IntegrationGate,
    language: NamesLanguage,
) {
    let publisher: Arc<dyn EventPublisher> = Arc::clone(&bus) as Arc<dyn EventPublisher>;
    let run_event = SubActionRun {
        step_index: 0,
        kind: req.step.kind_id.clone(),
    }
    .into_event(EventSource::Core, req.caused_by);
    let run_event_id = run_event.id;
    let _step_lineage = bus.publish_held(run_event);

    let stack = ArgStack::new().with_names_language(language);
    let executor = GatedLeafExecutor::new(integrations.clone(), cancel.clone());
    let run_ctx = RunContext {
        arg_stack: &stack,
        index: 0,
        parent_event_id: run_event_id,
        publisher: publisher.as_ref(),
        executor: &executor,
        cancel,
        control: ControlCell::new(),
        telemetry: TelemetrySink::new(),
    };

    let started_at = OffsetDateTime::now_utc();
    let (mut telemetry, produced_stack) = match sub_action_registry.get(&req.step.kind_id) {
        Some(runner) => {
            crate::engine::chain::execute_step(
                &sub_action_registry,
                &integrations,
                runner,
                &req.step,
                &run_ctx,
            )
            .await
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

    telemetry.args_in =
        crate::engine::chain::capture_args_in(&sub_action_registry, &req.step, &stack);
    telemetry.produced = match &produced_stack {
        Some(after) => crate::engine::chain::capture_produced(&stack, after),
        None => ::std::collections::BTreeMap::new(),
    };

    crate::engine::chain::publish_subaction_done(publisher.as_ref(), run_event_id, 0, &telemetry);

    bus.publish(
        QuickActionDone {
            kind: telemetry.kind.clone(),
            outcome: RunOutcome::from(&telemetry.outcome),
            label: req.label.clone(),
            builtin_id: req.builtin_id.clone(),
        }
        .into_event(EventSource::Core, Some(run_event_id)),
    );

    let run_outcome = match telemetry.outcome.failure_reason() {
        Some(reason) => ExecutionOutcome::Failed(reason),
        None => ExecutionOutcome::Success,
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
    history.record(ctx, None);
    #[expect(
        clippy::let_underscore_must_use,
        reason = "the requester may stop waiting for the reply"
    )]
    let _ = req.outcome.send(reported);
}

pub struct ActionEngineParts {
    pub bus: Arc<EventBus>,
    pub catalog: Arc<Catalog>,
    pub actions: Arc<dyn ActionRepo>,
    pub history: Arc<dyn HistoryRepo>,
    pub sub_action_registry: Arc<SubActionRegistry>,
    pub cancel_registry: Arc<ActionCancelRegistry>,
    pub date_names: DateNamesHandle,
    pub config: Config,
}

pub fn spawn_action_engine(parts: ActionEngineParts) -> ActionEngineHandle {
    ActionEngine::spawn(parts)
}

#[cfg(test)]
mod tests {
    use forge_types::SubActionTelemetry;
    use std::collections::BTreeMap;
    use std::sync::Mutex;
    use std::time::Duration;

    use async_trait::async_trait;
    use forge_registry::{FormField, RegistryError, SubActionCategory};
    use forge_storage::DataProvider;
    use forge_types::Variant;

    use super::*;
    use crate::NullEventLogRepo;
    use crate::test_support::{Sandboxed, sandboxed_backend};
    use forge_events::Event;

    const HISTORY_HANG_GUARD: Duration = Duration::from_secs(30);

    async fn engine_provider() -> Sandboxed<Arc<dyn DataProvider>> {
        sandboxed_backend()
            .await
            .map(|backend| Arc::new(backend) as Arc<dyn DataProvider>)
    }

    fn engine_over(
        dp: &Arc<dyn DataProvider>,
        bus: &Arc<EventBus>,
        registry: SubActionRegistry,
    ) -> ActionEngineHandle {
        engine_recording_to(dp, bus, registry, dp.history_repo())
    }

    fn engine_recording_to(
        dp: &Arc<dyn DataProvider>,
        bus: &Arc<EventBus>,
        registry: SubActionRegistry,
        history: Arc<dyn HistoryRepo>,
    ) -> ActionEngineHandle {
        spawn_action_engine(crate::ActionEngineParts {
            bus: Arc::clone(bus),
            catalog: crate::Catalog::from_provider(dp.as_ref()),
            actions: dp.action_repo(),
            history,
            sub_action_registry: Arc::new(registry),
            cancel_registry: Arc::new(crate::engine::action_cancel::ActionCancelRegistry::new()),
            date_names: crate::DateNamesHandle::default(),
            config: crate::Config::default(),
        })
    }

    struct RecordingRunner {
        last_config: Arc<Mutex<Option<forge_registry::SubActionConfig>>>,
    }

    impl forge_registry::StepCatalogEntry for RecordingRunner {
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
    }

    #[async_trait]
    impl forge_registry::SubActionRunner for RecordingRunner {
        fn id(&self) -> &str {
            "test.record"
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
                    position: forge_types::StepPosition::TopLevel(0),
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

    impl forge_registry::StepCatalogEntry for FixedOutcomeRunner {
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
    }

    #[async_trait]
    impl forge_registry::SubActionRunner for FixedOutcomeRunner {
        fn id(&self) -> &str {
            &self.id
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
                    position: forge_types::StepPosition::TopLevel(ctx.index),
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
        let dp = engine_provider().await;
        let bus = EventBus::new(Arc::new(NullEventLogRepo));

        let mut reg = SubActionRegistry::new();
        reg.register(Box::new(FixedOutcomeRunner {
            id: "map.success".to_owned(),
            outcome: SubActionOutcome::Success,
        }))
        .unwrap();
        reg.register(Box::new(FixedOutcomeRunner {
            id: "map.failure".to_owned(),
            outcome: SubActionOutcome::Failed(VerbatimText::new("kaboom").into()),
        }))
        .unwrap();
        let engine = engine_over(&dp, &bus, reg);

        let cases = [
            ("map.success", ExecutionOutcome::Success),
            (
                "map.failure",
                ExecutionOutcome::Failed(VerbatimText::new("kaboom").into()),
            ),
        ];

        let default_queue = forge_types::DEFAULT_QUEUE_ID;

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
                    manual_test: false,
                    initial_args: ArgStack::new(),
                })
                .await
                .unwrap();

            let mut recorded = None;
            let deadline = tokio::time::Instant::now() + HISTORY_HANG_GUARD;
            while tokio::time::Instant::now() < deadline {
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
        let dp = engine_provider().await;
        let bus = EventBus::new(Arc::new(NullEventLogRepo));

        let last = Arc::new(Mutex::new(None));
        let runner = Box::new(RecordingRunner {
            last_config: Arc::clone(&last),
        });
        let mut reg = SubActionRegistry::new();
        reg.register(runner).unwrap();
        let engine = engine_over(&dp, &bus, reg);

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
            .execute_quick_action(
                step,
                QuickActionOrigin {
                    builtin_id: "test.record".to_owned(),
                    label: "Test".to_owned(),
                },
                None,
            )
            .await
            .unwrap();

        forge_test_support::settle_until("the quick action reached its runner", || {
            last.lock().unwrap().is_some()
        })
        .await;

        let captured = last.lock().unwrap().clone().expect("runner not called");
        assert_eq!(captured.get("a"), Some(&Variant::Int(99)));
        assert_eq!(captured.get("b"), Some(&Variant::Int(2)));
    }

    #[tokio::test]
    async fn quick_path_emits_action_quick_done_and_links_subaction_run_causation() {
        let dp = engine_provider().await;
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let engine = engine_over(&dp, &bus, SubActionRegistry::new());

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
                    QuickActionOrigin {
                        builtin_id: "twitch".to_owned(),
                        label: "Probe".to_owned(),
                    },
                    expected_parent,
                )
                .await
                .unwrap();

            let mut run_event = None;
            let mut done_event = None;
            for _ in 0..40 {
                match tokio::time::timeout(HISTORY_HANG_GUARD, sub.recv()).await {
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
        let mut mock = forge_storage::MockHistoryRepo::new();
        mock.expect_save_batch()
            .times(1..)
            .returning(move |contexts| {
                for ctx in contexts {
                    #[expect(
                        clippy::let_underscore_must_use,
                        reason = "the receiver may already be gone"
                    )]
                    let _ = tx.send(ctx.clone());
                }
                Ok(())
            });
        (Arc::new(mock), rx)
    }

    async fn recv_ctx(
        rx: &mut tokio::sync::mpsc::UnboundedReceiver<ExecutionContext>,
    ) -> ExecutionContext {
        tokio::time::timeout(HISTORY_HANG_GUARD, rx.recv())
            .await
            .expect("history save did not arrive in time")
            .expect("history channel closed")
    }

    fn action_step(kind_id: &str, config: BTreeMap<String, Variant>) -> SubActionStep {
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
        let dp = engine_provider().await;
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
            outcome: SubActionOutcome::Failed(VerbatimText::new("boom").into()),
        }))
        .unwrap();
        reg.register(Box::new(FixedOutcomeRunner {
            id: "quick.skip".to_owned(),
            outcome: SubActionOutcome::Skipped(VerbatimText::new("nope").into()),
        }))
        .unwrap();

        let engine = engine_recording_to(&dp, &bus, reg, history);

        let cases = [
            ("quick.ok", ExecutionOutcome::Success),
            (
                "quick.fail",
                ExecutionOutcome::Failed(VerbatimText::new("boom").into()),
            ),
            ("quick.skip", ExecutionOutcome::Success),
        ];
        for (kind, expected) in cases {
            engine
                .execute_quick_action(
                    action_step(kind, BTreeMap::new()),
                    QuickActionOrigin {
                        builtin_id: "obs".to_owned(),
                        label: kind.to_owned(),
                    },
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
        let dp = engine_provider().await;
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let (history, mut rx) = capturing_history();
        let engine = spawn_action_engine(crate::ActionEngineParts {
            bus: Arc::clone(&bus),
            catalog: crate::Catalog::new(
                dp.action_repo(),
                dp.trigger_instance_repo(),
                dp.catalog_revision(),
            ),
            actions: dp.action_repo(),
            history,
            sub_action_registry: Arc::new(SubActionRegistry::new()),
            cancel_registry: Arc::new(crate::engine::action_cancel::ActionCancelRegistry::new()),
            date_names: crate::DateNamesHandle::default(),
            config: crate::Config::default(),
        });

        engine
            .execute_quick_action(
                action_step("quick.probe", BTreeMap::new()),
                QuickActionOrigin {
                    builtin_id: "twitch".to_owned(),
                    label: "Set title".to_owned(),
                },
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
        let dp = engine_provider().await;
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let (history, mut rx) = capturing_history();
        let engine = spawn_action_engine(crate::ActionEngineParts {
            bus: Arc::clone(&bus),
            catalog: crate::Catalog::new(
                dp.action_repo(),
                dp.trigger_instance_repo(),
                dp.catalog_revision(),
            ),
            actions: dp.action_repo(),
            history,
            sub_action_registry: Arc::new(SubActionRegistry::new()),
            cancel_registry: Arc::new(crate::engine::action_cancel::ActionCancelRegistry::new()),
            date_names: crate::DateNamesHandle::default(),
            config: crate::Config::default(),
        });

        engine
            .execute_quick_action(
                action_step("quick.probe", BTreeMap::new()),
                QuickActionOrigin {
                    builtin_id: "obs".to_owned(),
                    label: "Run".to_owned(),
                },
                None,
            )
            .await
            .unwrap();
        let first = recv_ctx(&mut rx).await;
        engine
            .execute_quick_action(
                action_step("quick.probe", BTreeMap::new()),
                QuickActionOrigin {
                    builtin_id: "obs".to_owned(),
                    label: "Run".to_owned(),
                },
                None,
            )
            .await
            .unwrap();
        let second = recv_ctx(&mut rx).await;

        assert_ne!(first.action_id, second.action_id);
    }

    #[tokio::test]
    async fn quick_action_history_captures_merged_config_as_args_in() {
        let dp = engine_provider().await;
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let (history, mut rx) = capturing_history();

        let mut reg = SubActionRegistry::new();
        reg.register(Box::new(RecordingRunner {
            last_config: Arc::new(Mutex::new(None)),
        }))
        .unwrap();
        let engine = engine_recording_to(&dp, &bus, reg, history);

        let mut overrides = BTreeMap::new();
        overrides.insert("a".to_owned(), Variant::Int(99));
        engine
            .execute_quick_action(
                action_step("test.record", overrides),
                QuickActionOrigin {
                    builtin_id: "obs".to_owned(),
                    label: "Rec".to_owned(),
                },
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

    const EMIT_KIND: &str = "test.emit";
    const FLOOD_KIND: &str = "test.flood";
    const FLOOD_THEN_EMIT_KIND: &str = "test.flood_then_emit";
    const CANCEL_KIND: &str = "test.cancel";
    const SMALL_RING: usize = 16;

    struct EmitRunner {
        id: &'static str,
        fillers: usize,
        emits: bool,
        cancels: bool,
    }

    impl EmitRunner {
        fn all() -> [Self; 4] {
            [
                Self {
                    id: EMIT_KIND,
                    fillers: 0,
                    emits: true,
                    cancels: false,
                },
                Self {
                    id: FLOOD_KIND,
                    fillers: SMALL_RING,
                    emits: false,
                    cancels: false,
                },
                Self {
                    id: FLOOD_THEN_EMIT_KIND,
                    fillers: SMALL_RING,
                    emits: true,
                    cancels: false,
                },
                Self {
                    id: CANCEL_KIND,
                    fillers: 0,
                    emits: false,
                    cancels: true,
                },
            ]
        }
    }

    impl forge_registry::StepCatalogEntry for EmitRunner {
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
    }

    #[async_trait]
    impl forge_registry::SubActionRunner for EmitRunner {
        fn id(&self) -> &str {
            self.id
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
            for _ in 0..self.fillers {
                ctx.publisher.publish(Event::new(
                    EventSource::Twitch,
                    "twitch.chat",
                    serde_json::Value::Null,
                ));
            }
            if self.cancels {
                ctx.cancel.cancel();
            }
            if self.emits {
                ctx.publisher.publish(Event::child_of(
                    EventSource::Core,
                    LOOP_EVENT_KIND,
                    serde_json::Value::Null,
                    ctx.parent_event_id,
                ));
            }
            (
                SubActionTelemetry {
                    args_in: BTreeMap::new(),
                    produced: BTreeMap::new(),
                    position: forge_types::StepPosition::TopLevel(ctx.index),
                    kind: self.id.to_owned(),
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
        self_loop_rig_with(
            EventBus::new(Arc::new(NullEventLogRepo)),
            vec![action_step(EMIT_KIND, BTreeMap::new())],
        )
        .await
    }

    async fn small_ring_self_loop_rig(steps: &[&str]) -> SelfLoopRig {
        small_ring_rig(
            steps
                .iter()
                .map(|kind_id| action_step(kind_id, BTreeMap::new()))
                .collect(),
        )
        .await
    }

    fn small_ring_bus() -> Arc<EventBus> {
        let config = Config {
            bus_ring_retention: SMALL_RING,
            ..Config::default()
        };
        EventBus::with_config(Arc::new(NullEventLogRepo), &config)
    }

    async fn small_ring_rig(steps: Vec<SubActionStep>) -> SelfLoopRig {
        self_loop_rig_with(small_ring_bus(), steps).await
    }

    async fn self_loop_rig_with(bus: Arc<EventBus>, steps: Vec<SubActionStep>) -> SelfLoopRig {
        let dp = engine_provider().await;
        let mut reg = SubActionRegistry::new();
        for runner in EmitRunner::all() {
            reg.register(Box::new(runner)).unwrap();
        }
        let gate = Arc::new(crate::ConditionGate::new(&Config {
            condition_wall_time_ms: 30_000,
            ..Config::default()
        }));
        reg.register(Box::new(
            crate::sub_action_runners::CoreLogicIfThenElseRunner::new(Arc::clone(&gate)),
        ))
        .unwrap();
        reg.register(Box::new(
            crate::sub_action_runners::CoreLogicLoopRunner::new(gate),
        ))
        .unwrap();
        let engine = engine_over(&dp, &bus, reg);
        let action_id = ActionId::new();
        let action = forge_types::Action {
            id: action_id,
            name: "echo".to_owned(),
            group: None,
            queue_id: forge_types::DEFAULT_QUEUE_ID,
            enabled: true,
            concurrent: false,
            bypass_pause: false,
            execution_mode: forge_types::ExecutionMode::Sequential,
            description: None,
            sub_actions: steps,
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
                    manual_test: false,
                    initial_args: ArgStack::new(),
                },
                CancelSignal::new(),
                done_tx,
            )
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(30), done_rx)
            .await
            .expect("run did not finish")
            .unwrap();
    }

    struct LoopTrace {
        starts: usize,
        skipped: Option<Event>,
        last_trigger: EventId,
    }

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
                    ActionStarted::KIND => trace.starts += 1,
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
    async fn self_loop_stops_at_the_depth_limit_for_each_flooding_shape() {
        for (label, input) in [
            (
                "self_loop_stops_at_the_depth_limit_after_an_earlier_step_floods_its_start_out_of_the_ring",
                &[FLOOD_KIND, EMIT_KIND][..],
            ),
            (
                "self_loop_stops_at_the_depth_limit_when_its_emitting_step_floods_its_own_run_event_out_of_the_ring",
                &[FLOOD_THEN_EMIT_KIND][..],
            ),
        ] {
            let rig = small_ring_self_loop_rig(input).await;
            let root = platform_event(&rig.bus);

            let trace = drive_self_loop(&rig, root).await;

            assert_eq!(trace.starts, MAX_CAUSATION_DEPTH, "{label}");
        }
    }

    fn body_of(kind_id: &str) -> Variant {
        Variant::Array(vec![Variant::Object(BTreeMap::from([(
            "kind_id".to_owned(),
            Variant::String(kind_id.to_owned()),
        )]))])
    }

    fn taken_branch_around(kind_id: &str) -> SubActionStep {
        action_step(
            "core.logic.if_then_else",
            BTreeMap::from([
                ("condition".to_owned(), Variant::String("1 == 1".to_owned())),
                ("then_chain".to_owned(), body_of(kind_id)),
            ]),
        )
    }

    fn single_pass_loop_around(kind_id: &str) -> SubActionStep {
        action_step(
            "core.logic.loop",
            BTreeMap::from([
                ("mode".to_owned(), Variant::String("count".to_owned())),
                ("count".to_owned(), Variant::Int(1)),
                ("body".to_owned(), body_of(kind_id)),
            ]),
        )
    }

    fn drain_lineage_ids(sub: &mut forge_events::EventSubscription) -> Vec<EventId> {
        let mut ids = Vec::new();
        while let Ok(Some(event)) = sub.try_recv() {
            if event.kind == ActionStarted::KIND || event.kind == "subaction.run" {
                ids.push(event.id);
            }
        }
        ids
    }

    fn depths_after_flooding_the_ring(bus: &EventBus, ids: &[EventId]) -> Vec<u16> {
        for _ in 0..SMALL_RING {
            bus.publish(Event::new(
                EventSource::Twitch,
                "twitch.chat",
                serde_json::Value::Null,
            ));
        }
        ids.iter().map(|id| bus.causation_depth(*id)).collect()
    }

    #[tokio::test]
    async fn self_loop_stops_at_the_depth_limit_when_a_nested_body_step_floods_its_run_event_out_of_the_ring()
     {
        for (container, nested) in [
            ("if", taken_branch_around(FLOOD_THEN_EMIT_KIND)),
            ("loop", single_pass_loop_around(FLOOD_THEN_EMIT_KIND)),
        ] {
            let rig = small_ring_rig(vec![nested]).await;
            let root = platform_event(&rig.bus);

            let trace = drive_self_loop(&rig, root).await;

            assert_eq!(trace.starts, MAX_CAUSATION_DEPTH, "{container}");
        }
    }

    #[tokio::test]
    async fn finished_run_releases_every_lineage_it_held() {
        let rig = small_ring_rig(vec![
            action_step(EMIT_KIND, BTreeMap::new()),
            taken_branch_around(EMIT_KIND),
            single_pass_loop_around(EMIT_KIND),
        ])
        .await;
        let mut sub = rig.bus.subscribe();

        run_to_completion(&rig, platform_event(&rig.bus)).await;
        let held = drain_lineage_ids(&mut sub);

        assert_eq!(depths_after_flooding_the_ring(&rig.bus, &held), vec![0; 6]);
    }

    #[tokio::test]
    async fn cancelled_run_releases_every_lineage_it_held() {
        let rig = small_ring_rig(vec![
            taken_branch_around(CANCEL_KIND),
            action_step(EMIT_KIND, BTreeMap::new()),
        ])
        .await;
        let mut sub = rig.bus.subscribe();

        run_to_completion(&rig, platform_event(&rig.bus)).await;
        let held = drain_lineage_ids(&mut sub);

        assert_eq!(depths_after_flooding_the_ring(&rig.bus, &held), vec![0; 3]);
    }

    #[tokio::test]
    async fn finished_quick_action_releases_its_run_lineage() {
        let rig = small_ring_rig(Vec::new()).await;
        let mut parent = Event::new(EventSource::Core, "action.start", serde_json::Value::Null);
        parent.causation_depth = 3;
        let parent_id = parent.id;
        rig.bus.publish(parent);
        let mut sub = rig.bus.subscribe();

        rig.engine
            .execute_quick_action(
                action_step(EMIT_KIND, BTreeMap::new()),
                QuickActionOrigin {
                    builtin_id: "obs".to_owned(),
                    label: "Run".to_owned(),
                },
                Some(parent_id),
            )
            .await
            .unwrap()
            .outcome()
            .await
            .unwrap();
        let held = drain_lineage_ids(&mut sub);

        assert_eq!(depths_after_flooding_the_ring(&rig.bus, &held), vec![0]);
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
        rig.bus.shutdown();
        rig.bus.await_flush().await;

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
            started |= event.kind == ActionStarted::KIND;
        }
        assert!(
            started,
            "an unrelated platform event must not inherit the refused loop's depth"
        );
    }

    #[tokio::test]
    async fn a_run_started_after_an_action_edit_returns_executes_the_edited_action() {
        let dp = engine_provider().await;
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let engine = engine_over(&dp, &bus, SubActionRegistry::new());
        let default_queue = forge_types::DEFAULT_QUEUE_ID;
        let mut action = forge_types::Action {
            id: ActionId::new(),
            name: "before".to_owned(),
            group: None,
            queue_id: default_queue,
            enabled: true,
            concurrent: false,
            bypass_pause: false,
            execution_mode: forge_types::ExecutionMode::Sequential,
            description: None,
            sub_actions: vec![],
        };

        let mut started = Vec::new();
        for name in ["before", "after"] {
            action.name = name.to_owned();
            dp.action_repo().save(&action).await.unwrap();
            let mut sub = bus.subscribe();
            engine
                .dispatch(ExecutionRequest {
                    action_id: action.id,
                    trigger_event_id: EventId::new(),
                    trigger_kind: None,
                    manual_test: false,
                    initial_args: ArgStack::new(),
                })
                .await
                .unwrap();
            let start = loop {
                let event = tokio::time::timeout(Duration::from_secs(30), sub.recv())
                    .await
                    .expect("the dispatched run never started")
                    .unwrap();
                if event.kind == ActionStarted::KIND {
                    break event;
                }
            };
            started.push(start.payload["action_name"].as_str().unwrap().to_owned());
        }

        assert_eq!(started, ["before", "after"]);
    }
}
