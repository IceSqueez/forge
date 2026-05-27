use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use forge_events::{Event, EventSource};
use forge_registry::{RunContext, SubActionRegistry};
use forge_storage::{ActionRepo, HistoryRepo};
use forge_types::{
    ActionId, ArgStack, EventId, ExecutionContext, ExecutionMetadata, ExecutionOutcome,
    SubActionOutcome, SubActionStep, SubActionTelemetry,
};
use serde_json::json;
use time::OffsetDateTime;
use tokio::sync::mpsc;
use tracing::warn;

use crate::EventBus;

struct QuickActionRequest {
    step: SubActionStep,
    builtin_id: String,
    label: String,
}

#[derive(Clone)]
pub struct ActionEngineHandle {
    sender: mpsc::Sender<ExecutionRequest>,
    quick_sender: mpsc::Sender<QuickActionRequest>,
    cancel: Arc<AtomicBool>,
}

pub struct ExecutionRequest {
    pub action_id: ActionId,
    pub trigger_event_id: EventId,
    pub initial_args: forge_types::ArgStack,
}

#[derive(Debug, thiserror::Error)]
pub enum DispatchError {
    #[error("engine channel closed")]
    ChannelClosed,
}

impl ActionEngineHandle {
    pub async fn dispatch(&self, req: ExecutionRequest) -> Result<(), DispatchError> {
        self.sender
            .send(req)
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
    sub_action_registry: Arc<SubActionRegistry>,
    input: mpsc::Receiver<ExecutionRequest>,
}

impl ActionEngine {
    pub fn spawn(
        bus: Arc<EventBus>,
        actions: Arc<dyn ActionRepo>,
        history: Arc<dyn HistoryRepo>,
        sub_action_registry: Arc<SubActionRegistry>,
    ) -> ActionEngineHandle {
        let (tx, rx) = mpsc::channel(256);
        let (quick_tx, quick_rx) = mpsc::channel(64);
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_clone = Arc::clone(&cancel);
        let engine = Self {
            bus: Arc::clone(&bus),
            actions: Arc::clone(&actions),
            history: Arc::clone(&history),
            sub_action_registry: Arc::clone(&sub_action_registry),
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
                Some(req) => self.handle(req).await,
                None => break,
            }
        }
    }

    async fn handle(&self, req: ExecutionRequest) {
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

        if action.concurrent {
            self.run_concurrent(&pick, &arg_stack, &mut ctx, start_event_id)
                .await;
        } else {
            self.run_sequential(&pick, &arg_stack, &mut ctx, start_event_id)
                .await;
        }

        ctx.completed_at = Some(OffsetDateTime::now_utc());

        let total_ms: u64 = ctx.telemetry.iter().map(|t| t.duration_ms).sum();
        let outcome_label = match &ctx.outcome {
            ExecutionOutcome::Success => "success",
            ExecutionOutcome::Failed(_) => "failed",
            ExecutionOutcome::Cancelled => "cancelled",
        };

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

        if let Err(e) = self.history.save(&ctx).await {
            warn!("history_repo.save failed: {e}");
        }
    }

    async fn run_sequential(
        &self,
        steps: &[SubActionStep],
        arg_stack: &ArgStack,
        ctx: &mut ExecutionContext,
        parent_event_id: EventId,
    ) {
        let publisher: Arc<dyn forge_events::EventPublisher> =
            Arc::clone(&self.bus) as Arc<dyn forge_events::EventPublisher>;
        let mut current_stack = arg_stack.clone();
        for (index, step) in steps.iter().enumerate() {
            if !step.enabled {
                continue;
            }

            let run_event = Event::caused_by(
                EventSource::Core,
                "subaction.run",
                json!({
                    "step_index": index,
                    "kind": step.kind_id,
                }),
                parent_event_id,
            );
            let run_event_id = run_event.id;
            self.bus.publish(run_event);

            let run_ctx = RunContext {
                arg_stack: &current_stack,
                index,
                parent_event_id: run_event_id,
                publisher: publisher.as_ref(),
            };

            let (telemetry, updated_stack) = match self.sub_action_registry.get(&step.kind_id) {
                Some(runner) => runner.execute(&step.config, &run_ctx).await,
                None => {
                    warn!(
                        "unknown sub-action kind_id: {} — skipping step",
                        step.kind_id
                    );
                    (skipped_telemetry(index, &step.kind_id), None)
                }
            };

            if let Some(new_stack) = updated_stack {
                current_stack = new_stack;
            }

            let failure_msg = match &telemetry.outcome {
                SubActionOutcome::Failed(m) => Some(m.clone()),
                _ => None,
            };
            ctx.telemetry.push(telemetry);

            if let Some(msg) = failure_msg {
                ctx.outcome = ExecutionOutcome::Failed(msg);
                return;
            }
        }
    }

    async fn run_concurrent(
        &self,
        steps: &[SubActionStep],
        arg_stack: &ArgStack,
        ctx: &mut ExecutionContext,
        parent_event_id: EventId,
    ) {
        use futures_util::future::join_all;

        let publisher: Arc<dyn forge_events::EventPublisher> =
            Arc::clone(&self.bus) as Arc<dyn forge_events::EventPublisher>;

        let futures: Vec<_> = steps
            .iter()
            .enumerate()
            .filter(|(_, step)| step.enabled)
            .map(|(index, step)| {
                let run_event = Event::caused_by(
                    EventSource::Core,
                    "subaction.run",
                    json!({
                        "step_index": index,
                        "kind": step.kind_id,
                    }),
                    parent_event_id,
                );
                let run_event_id = run_event.id;
                self.bus.publish(run_event);

                let run_ctx = RunContext {
                    arg_stack,
                    index,
                    parent_event_id: run_event_id,
                    publisher: publisher.as_ref(),
                };

                async move {
                    match self.sub_action_registry.get(&step.kind_id) {
                        Some(runner) => runner.execute(&step.config, &run_ctx).await,
                        None => {
                            warn!(
                                "unknown sub-action kind_id: {} — skipping step",
                                step.kind_id
                            );
                            (skipped_telemetry(index, &step.kind_id), None)
                        }
                    }
                }
            })
            .collect();

        let results = join_all(futures).await;

        let mut first_failure: Option<String> = None;
        for (telemetry, _) in results {
            if first_failure.is_none()
                && let SubActionOutcome::Failed(msg) = &telemetry.outcome
            {
                first_failure = Some(msg.clone());
            }
            ctx.telemetry.push(telemetry);
        }

        if let Some(msg) = first_failure {
            ctx.outcome = ExecutionOutcome::Failed(msg);
        }
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
        let run_ctx = RunContext {
            arg_stack: &stack,
            index: 0,
            parent_event_id: run_event_id,
            publisher: publisher.as_ref(),
        };

        let (telemetry, _) = match sub_action_registry.get(&req.step.kind_id) {
            Some(runner) => runner.execute(&req.step.config, &run_ctx).await,
            None => {
                warn!(
                    "unknown sub-action kind_id: {} — skipping step",
                    req.step.kind_id
                );
                (skipped_telemetry(0, &req.step.kind_id), None)
            }
        };

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

fn skipped_telemetry(index: usize, kind_id: &str) -> SubActionTelemetry {
    SubActionTelemetry {
        index,
        kind: kind_id.to_owned(),
        started_at: OffsetDateTime::now_utc(),
        duration_ms: 0,
        outcome: SubActionOutcome::Skipped(format!("unknown kind_id: {kind_id}")),
    }
}

pub fn spawn_action_engine(
    bus: Arc<EventBus>,
    actions: Arc<dyn ActionRepo>,
    history: Arc<dyn HistoryRepo>,
    sub_action_registry: Arc<SubActionRegistry>,
) -> ActionEngineHandle {
    ActionEngine::spawn(bus, actions, history, sub_action_registry)
}
