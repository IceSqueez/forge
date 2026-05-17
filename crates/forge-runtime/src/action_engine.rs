use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use forge_events::{Event, EventSource};
use forge_storage::DataProvider;
use forge_types::{ActionId, EventId, ExecutionContext, ExecutionOutcome, SubActionOutcome};
use serde_json::json;
use time::OffsetDateTime;
use tokio::sync::mpsc;
use tracing::warn;

use crate::EventBus;
use crate::sub_actions::dispatch;

pub struct ActionEngineHandle {
    sender: mpsc::Sender<ExecutionRequest>,
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

    pub fn shutdown(self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

struct ActionEngine {
    bus: Arc<EventBus>,
    dp: Arc<dyn DataProvider>,
    input: mpsc::Receiver<ExecutionRequest>,
}

impl ActionEngine {
    pub fn spawn(bus: Arc<EventBus>, dp: Arc<dyn DataProvider>) -> ActionEngineHandle {
        let (tx, rx) = mpsc::channel(256);
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_clone = Arc::clone(&cancel);
        let engine = Self { bus, dp, input: rx };
        tokio::spawn(async move { engine.run(cancel_clone).await });
        ActionEngineHandle { sender: tx, cancel }
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
        let action = match self.dp.action_repo().get(req.action_id).await {
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
            trigger_event_id: req.trigger_event_id,
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

        if action.concurrent {
            self.run_concurrent(&action.sub_actions, &arg_stack, &mut ctx, start_event_id)
                .await;
        } else {
            self.run_sequential(&action.sub_actions, &arg_stack, &mut ctx, start_event_id)
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

        if let Err(e) = self.dp.history_repo().save(&ctx).await {
            warn!("history_repo.save failed: {e}");
        }
    }

    async fn run_sequential(
        &self,
        specs: &[forge_types::SubActionSpec],
        arg_stack: &forge_types::ArgStack,
        ctx: &mut ExecutionContext,
        parent_event_id: EventId,
    ) {
        for (index, spec) in specs.iter().enumerate() {
            let run_event = Event::caused_by(
                EventSource::Core,
                "subaction.run",
                json!({
                    "step_index": index,
                    "kind": spec.kind_label(),
                }),
                parent_event_id,
            );
            self.bus.publish(run_event);

            let telemetry = dispatch(spec, arg_stack, index, &self.bus, self.dp.as_ref()).await;

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
        specs: &[forge_types::SubActionSpec],
        arg_stack: &forge_types::ArgStack,
        ctx: &mut ExecutionContext,
        parent_event_id: EventId,
    ) {
        use futures_util::future::join_all;

        let futures: Vec<_> = specs
            .iter()
            .enumerate()
            .map(|(index, spec)| {
                let run_event = Event::caused_by(
                    EventSource::Core,
                    "subaction.run",
                    json!({
                        "step_index": index,
                        "kind": spec.kind_label(),
                    }),
                    parent_event_id,
                );
                self.bus.publish(run_event);
                dispatch(spec, arg_stack, index, &self.bus, self.dp.as_ref())
            })
            .collect();

        let results = join_all(futures).await;

        let mut first_failure: Option<String> = None;
        for telemetry in results {
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

pub fn spawn_action_engine(bus: Arc<EventBus>, dp: Arc<dyn DataProvider>) -> ActionEngineHandle {
    ActionEngine::spawn(bus, dp)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use forge_storage::DataProvider;
    use forge_storage_sqlite::SqliteBackend;
    use forge_types::{Action, ActionId, EventId, LogLevel, Queue, QueueId, SubActionSpec};

    async fn make_dp() -> Arc<dyn DataProvider> {
        Arc::new(
            SqliteBackend::open_with_key(":memory:", [0xab; 32])
                .await
                .unwrap(),
        )
    }

    async fn seed_action(dp: &Arc<dyn DataProvider>, action: &Action) {
        let queue = Queue {
            id: action.queue_id,
            name: "default".to_string(),
            blocking: false,
        };
        dp.queue_repo().save(&queue).await.unwrap();
        dp.action_repo().save(action).await.unwrap();
    }

    fn simple_action(id: ActionId, queue_id: QueueId, concurrent: bool) -> Action {
        Action {
            id,
            name: "Test".to_string(),
            group: None,
            queue_id,
            enabled: true,
            concurrent,
            bypass_pause: false,
            description: None,
            sub_actions: vec![SubActionSpec::Log {
                level: LogLevel::Info,
                message: "running".to_string(),
            }],
        }
    }

    #[tokio::test]
    async fn disabled_action_skips_execution() {
        let dp = make_dp().await;
        let queue_id = QueueId::new();
        let mut action = simple_action(ActionId::new(), queue_id, false);
        action.enabled = false;
        seed_action(&dp, &action).await;

        let bus = EventBus::new();
        let handle = spawn_action_engine(Arc::clone(&bus), Arc::clone(&dp));

        handle
            .dispatch(ExecutionRequest {
                action_id: action.id,
                trigger_event_id: EventId::new(),
                initial_args: forge_types::ArgStack::new(),
            })
            .await
            .unwrap();

        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        let stats = bus.stats();
        assert_eq!(stats.total_published, 0, "no events for disabled action");
        handle.shutdown();
    }

    #[tokio::test]
    async fn sequential_log_action_publishes_start_and_done() {
        let dp = make_dp().await;
        let action_id = ActionId::new();
        let action = simple_action(action_id, QueueId::new(), false);
        seed_action(&dp, &action).await;

        let bus = EventBus::new();
        let mut sub = bus.subscribe();
        let handle = spawn_action_engine(Arc::clone(&bus), Arc::clone(&dp));

        let trigger_id = EventId::new();
        handle
            .dispatch(ExecutionRequest {
                action_id,
                trigger_event_id: trigger_id,
                initial_args: forge_types::ArgStack::new(),
            })
            .await
            .unwrap();

        let mut kinds = Vec::new();
        for _ in 0..3 {
            match tokio::time::timeout(tokio::time::Duration::from_millis(500), sub.recv()).await {
                Ok(Ok(ev)) => kinds.push(ev.kind.clone()),
                _ => break,
            }
        }

        assert!(
            kinds.contains(&"action.start".to_string()),
            "missing action.start"
        );
        assert!(
            kinds.contains(&"subaction.run".to_string()),
            "missing subaction.run"
        );
        assert!(
            kinds.contains(&"action.done".to_string()),
            "missing action.done"
        );
        handle.shutdown();
    }

    #[tokio::test]
    async fn action_start_caused_by_trigger_event() {
        let dp = make_dp().await;
        let action_id = ActionId::new();
        let action = simple_action(action_id, QueueId::new(), false);
        seed_action(&dp, &action).await;

        let bus = EventBus::new();
        let mut sub = bus.subscribe();
        let handle = spawn_action_engine(Arc::clone(&bus), Arc::clone(&dp));

        let trigger_id = EventId::new();
        handle
            .dispatch(ExecutionRequest {
                action_id,
                trigger_event_id: trigger_id,
                initial_args: forge_types::ArgStack::new(),
            })
            .await
            .unwrap();

        let start_event = tokio::time::timeout(tokio::time::Duration::from_millis(500), sub.recv())
            .await
            .unwrap()
            .unwrap();

        assert_eq!(start_event.kind, "action.start");
        assert_eq!(start_event.caused_by, Some(trigger_id));
        handle.shutdown();
    }

    #[tokio::test]
    async fn concurrent_action_runs_both_sub_actions() {
        let dp = make_dp().await;
        let action_id = ActionId::new();
        let mut action = simple_action(action_id, QueueId::new(), true);
        action.sub_actions = vec![
            SubActionSpec::Log {
                level: LogLevel::Info,
                message: "step-a".to_string(),
            },
            SubActionSpec::Log {
                level: LogLevel::Debug,
                message: "step-b".to_string(),
            },
        ];
        seed_action(&dp, &action).await;

        let bus = EventBus::new();
        let mut sub = bus.subscribe();
        let handle = spawn_action_engine(Arc::clone(&bus), Arc::clone(&dp));

        handle
            .dispatch(ExecutionRequest {
                action_id,
                trigger_event_id: EventId::new(),
                initial_args: forge_types::ArgStack::new(),
            })
            .await
            .unwrap();

        let mut subaction_run_count = 0usize;
        let mut received = 0usize;
        loop {
            match tokio::time::timeout(tokio::time::Duration::from_millis(500), sub.recv()).await {
                Ok(Ok(ev)) => {
                    if ev.kind == "subaction.run" {
                        subaction_run_count += 1;
                    }
                    received += 1;
                    if ev.kind == "action.done" {
                        break;
                    }
                }
                _ => break,
            }
            if received > 20 {
                break;
            }
        }

        assert_eq!(
            subaction_run_count, 2,
            "both sub-actions must fire subaction.run"
        );
        handle.shutdown();
    }

    #[tokio::test]
    async fn context_persisted_to_history_repo() {
        let dp = make_dp().await;
        let action_id = ActionId::new();
        let action = simple_action(action_id, QueueId::new(), false);
        seed_action(&dp, &action).await;

        let bus = EventBus::new();
        let mut sub = bus.subscribe();
        let handle = spawn_action_engine(Arc::clone(&bus), Arc::clone(&dp));

        handle
            .dispatch(ExecutionRequest {
                action_id,
                trigger_event_id: EventId::new(),
                initial_args: forge_types::ArgStack::new(),
            })
            .await
            .unwrap();

        loop {
            match tokio::time::timeout(tokio::time::Duration::from_millis(500), sub.recv()).await {
                Ok(Ok(ev)) if ev.kind == "action.done" => break,
                Ok(Ok(_)) => {}
                _ => break,
            }
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(30)).await;

        let history = dp
            .history_repo()
            .recent_for_action(action_id, 10)
            .await
            .unwrap();
        assert_eq!(history.len(), 1, "one execution must be saved");
        assert_eq!(history[0].action_id, action_id);
        handle.shutdown();
    }
}
