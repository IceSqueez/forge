use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use forge_events::{Event, EventPublisher, EventSource};
use forge_registry::{CancelSignal, ChainSignal, RunContext, SubActionRegistry, effective_config};
use forge_storage::{ActionRepo, ExecutionStatus, HistoryRepo};
use forge_types::{
    ActionId, ArgStack, EventId, ExecutionContext, ExecutionMetadata, ExecutionOutcome,
    SubActionOutcome, SubActionStep, SubActionTelemetry,
};
use serde_json::json;
use time::OffsetDateTime;
use tokio::sync::{mpsc, oneshot};
use tracing::warn;

use crate::action_cancel::ActionCancelRegistry;
use crate::chain::ChainEngine;
use crate::{Config, EventBus};

struct QuickActionRequest {
    step: SubActionStep,
    builtin_id: String,
    label: String,
}

#[derive(Clone)]
pub struct ActionEngineHandle {
    sender: mpsc::Sender<EngineJob>,
    quick_sender: mpsc::Sender<QuickActionRequest>,
    cancel: Arc<AtomicBool>,
}

pub struct ExecutionRequest {
    pub action_id: ActionId,
    pub trigger_event_id: EventId,
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

    /// Drives the execution with the caller-provided `cancel` (so an external
    /// owner can cancel the live chain) and fires `on_complete` once the run
    /// terminates, regardless of outcome.
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

    pub async fn execute_quick_action(
        &self,
        step: SubActionStep,
        builtin_id: String,
        label: String,
    ) -> Result<(), DispatchError> {
        self.quick_sender
            .send(QuickActionRequest {
                step,
                builtin_id,
                label,
            })
            .await
            .map_err(|_| DispatchError::ChannelClosed)
    }

    pub fn shutdown(self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

struct ActionEngine {
    bus: Arc<EventBus>,
    actions: Arc<dyn ActionRepo>,
    history: Arc<dyn HistoryRepo>,
    chain_engine: Arc<ChainEngine>,
    cancel_registry: Arc<ActionCancelRegistry>,
    input: mpsc::Receiver<EngineJob>,
}

impl ActionEngine {
    pub fn spawn(
        bus: Arc<EventBus>,
        actions: Arc<dyn ActionRepo>,
        history: Arc<dyn HistoryRepo>,
        sub_action_registry: Arc<SubActionRegistry>,
        cancel_registry: Arc<ActionCancelRegistry>,
    ) -> ActionEngineHandle {
        let (tx, rx) = mpsc::channel(256);
        let (quick_tx, quick_rx) = mpsc::channel(64);
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_clone = Arc::clone(&cancel);
        let publisher: Arc<dyn EventPublisher> = Arc::clone(&bus) as Arc<dyn EventPublisher>;
        let chain_engine = Arc::new(ChainEngine::new(
            Arc::clone(&sub_action_registry),
            publisher,
            Config::default(),
        ));
        let engine = Self {
            bus: Arc::clone(&bus),
            actions: Arc::clone(&actions),
            history: Arc::clone(&history),
            chain_engine,
            cancel_registry,
            input: rx,
        };
        tokio::spawn(async move { engine.run(cancel_clone).await });
        tokio::spawn(run_quick_action_loop(quick_rx, bus, sub_action_registry));
        ActionEngineHandle {
            sender: tx,
            quick_sender: quick_tx,
            cancel,
        }
    }

    async fn run(mut self, cancel: Arc<AtomicBool>) {
        while !cancel.load(Ordering::Relaxed) {
            match self.input.recv().await {
                Some(job) => self.handle(job).await,
                None => break,
            }
        }
    }

    async fn handle(&self, job: EngineJob) {
        let EngineJob {
            request,
            cancel,
            on_complete,
        } = job;
        self.run_execution(request, &cancel).await;
        if let Some(done) = on_complete {
            let _ = done.send(());
        }
    }

    async fn run_execution(&self, req: ExecutionRequest, cancel: &CancelSignal) {
        let exec_id = self.cancel_registry.register(req.action_id, cancel.clone());
        let _cancel_guard = CancelGuard {
            registry: Arc::clone(&self.cancel_registry),
            action_id: req.action_id,
            exec_id,
        };

        let action = match self.actions.get(req.action_id).await {
            Ok(Some(a)) if a.enabled => a,
            Ok(_) => return,
            Err(e) => {
                warn!("action_repo.get failed: {e}");
                return;
            }
        };

        let arg_stack = req.initial_args;
        let started_at = OffsetDateTime::now_utc();

        let mut ctx = ExecutionContext {
            action_id: req.action_id,
            metadata: ExecutionMetadata::Trigger {
                event_id: req.trigger_event_id,
            },
            arg_stack_snapshot: arg_stack.snapshot(),
            started_at,
            completed_at: None,
            telemetry: Vec::new(),
            outcome: ExecutionOutcome::Success,
        };

        let start_event = Event::caused_by(
            EventSource::Core,
            "action.start",
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

        // An external cancel landing after the chain's last boundary check still
        // makes this a cancelled run; the leaf may have finished without observing it.
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

        // A cancelled run was killed mid-flight, not completed: it records to
        // history but emits no completion event onto the bus.
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
    bus: Arc<EventBus>,
    sub_action_registry: Arc<SubActionRegistry>,
) {
    let publisher: Arc<dyn forge_events::EventPublisher> =
        Arc::clone(&bus) as Arc<dyn forge_events::EventPublisher>;
    while let Some(req) = rx.recv().await {
        let run_event = Event::new(
            EventSource::Core,
            "subaction.run",
            json!({ "step_index": 0, "kind": req.step.kind_id }),
        );
        let run_event_id = run_event.id;
        bus.publish(run_event);

        let stack = ArgStack::new();
        let run_ctx = RunContext::leaf(&stack, 0, run_event_id, publisher.as_ref());

        let (telemetry, _) = match sub_action_registry.get(&req.step.kind_id) {
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

        crate::chain::publish_subaction_done(publisher.as_ref(), run_event_id, &telemetry);

        let outcome = match &telemetry.outcome {
            SubActionOutcome::Success => "success",
            SubActionOutcome::Failed(_) => "failed",
            SubActionOutcome::Skipped(_) => "skipped",
        };

        bus.publish(Event::caused_by(
            EventSource::Core,
            "quick_action.done",
            json!({
                "kind": telemetry.kind,
                "outcome": outcome,
                "label": req.label,
                "builtin_id": req.builtin_id,
            }),
            run_event_id,
        ));
    }
}

pub(crate) fn skipped_telemetry(index: usize, kind_id: &str) -> SubActionTelemetry {
    SubActionTelemetry {
        index,
        kind: kind_id.to_owned(),
        started_at: OffsetDateTime::now_utc(),
        duration_ms: 0,
        outcome: SubActionOutcome::Skipped(format!("unknown kind_id: {kind_id}")),
    }
}

pub(crate) fn disabled_telemetry(index: usize, kind_id: &str) -> SubActionTelemetry {
    SubActionTelemetry {
        index,
        kind: kind_id.to_owned(),
        started_at: OffsetDateTime::now_utc(),
        duration_ms: 0,
        outcome: SubActionOutcome::Skipped("disabled".to_owned()),
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
    use forge_storage_sqlite::SqliteBackend;
    use forge_types::Variant;

    use super::*;
    use crate::NullEventLogRepo;

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

    /// Drives the private ChainSignal -> ExecutionOutcome mapping through the
    /// public dispatch path and reads the recorded outcome back from history.
    /// Only Completed->Success and Error->Failed are reachable here: the chain
    /// foundation never emits Stop/Break/Continue, and the per-run cancel signal
    /// is internal and unreachable by a runner, so Aborted->Cancelled cannot be
    /// driven via ActionEngine (Aborted itself is covered at the chain level).
    #[tokio::test]
    async fn chain_signal_maps_to_recorded_execution_outcome() {
        let dp: Arc<dyn DataProvider> = Arc::new(
            SqliteBackend::open_with_key(":memory:", [0xcd; 32])
                .await
                .unwrap(),
        );
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

        // The migrations seed exactly one queue (the nil ULID); the actions FK
        // references it, so reuse that row rather than inventing a queue id.
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
                    label: None,
                }],
            };
            dp.action_repo().save(&action).await.unwrap();

            engine
                .dispatch(ExecutionRequest {
                    action_id,
                    trigger_event_id: EventId::new(),
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
        let dp: Arc<dyn DataProvider> = Arc::new(
            SqliteBackend::open_with_key(":memory:", [0xab; 32])
                .await
                .unwrap(),
        );
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
            label: None,
        };

        engine
            .execute_quick_action(step, "test.record".to_owned(), "Test".to_owned())
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
}
