use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use forge_events::{Event, EventSource};
use forge_obs::ObsSink;
use forge_storage::{ActionRepo, GlobalsRepo, HistoryRepo};
use forge_types::{
    ActionId, ArgStack, EventId, ExecutionContext, ExecutionMetadata, ExecutionOutcome,
    SubActionOutcome, SubActionSpec,
};
use serde_json::json;
use time::OffsetDateTime;
use tokio::sync::mpsc;
use tracing::warn;

use crate::EventBus;
use crate::script_registry::ScriptRegistry;
use crate::sound_player::SoundPlayer;
use crate::speak_dispatcher::SpeakDispatcher;
use crate::sub_actions::dispatch;

struct QuickActionRequest {
    spec: SubActionSpec,
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
        spec: SubActionSpec,
        builtin_id: String,
        label: String,
    ) -> Result<(), DispatchError> {
        self.quick_sender
            .send(QuickActionRequest {
                spec,
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
    globals: Arc<dyn GlobalsRepo>,
    registry: Arc<ScriptRegistry>,
    obs_sink: Option<Arc<dyn ObsSink>>,
    sound_player: Option<Arc<dyn SoundPlayer>>,
    speak_dispatcher: Option<Arc<dyn SpeakDispatcher>>,
    input: mpsc::Receiver<ExecutionRequest>,
}

impl ActionEngine {
    #[allow(clippy::too_many_arguments)]
    pub fn spawn(
        bus: Arc<EventBus>,
        actions: Arc<dyn ActionRepo>,
        history: Arc<dyn HistoryRepo>,
        globals: Arc<dyn GlobalsRepo>,
        registry: Arc<ScriptRegistry>,
        obs_sink: Option<Arc<dyn ObsSink>>,
        sound_player: Option<Arc<dyn SoundPlayer>>,
        speak_dispatcher: Option<Arc<dyn SpeakDispatcher>>,
    ) -> ActionEngineHandle {
        let (tx, rx) = mpsc::channel(256);
        let (quick_tx, quick_rx) = mpsc::channel(64);
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_clone = Arc::clone(&cancel);
        let engine = Self {
            bus: Arc::clone(&bus),
            actions: Arc::clone(&actions),
            history: Arc::clone(&history),
            globals: Arc::clone(&globals),
            registry: Arc::clone(&registry),
            obs_sink: obs_sink.clone(),
            sound_player: sound_player.clone(),
            speak_dispatcher: speak_dispatcher.clone(),
            input: rx,
        };
        tokio::spawn(async move { engine.run(cancel_clone).await });
        tokio::spawn(run_quick_action_loop(
            quick_rx,
            bus,
            globals,
            registry,
            obs_sink,
            sound_player,
            speak_dispatcher,
        ));
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

        let pick: Vec<forge_types::SubActionSpec> = if matches!(
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
        specs: &[forge_types::SubActionSpec],
        arg_stack: &forge_types::ArgStack,
        ctx: &mut ExecutionContext,
        parent_event_id: EventId,
    ) {
        let mut current_stack = arg_stack.clone();
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
            let run_event_id = run_event.id;
            self.bus.publish(run_event);

            let (telemetry, updated_stack) = dispatch(
                spec,
                &current_stack,
                index,
                run_event_id,
                &self.bus,
                Arc::clone(&self.globals),
                Some(self.registry.as_ref()),
                self.obs_sink.clone(),
                self.sound_player.as_ref(),
                self.speak_dispatcher.as_ref(),
            )
            .await;

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
                let run_event_id = run_event.id;
                self.bus.publish(run_event);
                dispatch(
                    spec,
                    arg_stack,
                    index,
                    run_event_id,
                    &self.bus,
                    Arc::clone(&self.globals),
                    Some(self.registry.as_ref()),
                    self.obs_sink.clone(),
                    self.sound_player.as_ref(),
                    self.speak_dispatcher.as_ref(),
                )
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
    globals: Arc<dyn GlobalsRepo>,
    registry: Arc<ScriptRegistry>,
    obs_sink: Option<Arc<dyn ObsSink>>,
    sound_player: Option<Arc<dyn SoundPlayer>>,
    speak_dispatcher: Option<Arc<dyn SpeakDispatcher>>,
) {
    while let Some(req) = rx.recv().await {
        let run_event = Event::new(
            EventSource::Core,
            "subaction.run",
            json!({ "step_index": 0, "kind": req.spec.kind_label() }),
        );
        let run_event_id = run_event.id;
        bus.publish(run_event);

        let (telemetry, _) = dispatch(
            &req.spec,
            &ArgStack::new(),
            0,
            run_event_id,
            &bus,
            Arc::clone(&globals),
            Some(registry.as_ref()),
            obs_sink.clone(),
            sound_player.as_ref(),
            speak_dispatcher.as_ref(),
        )
        .await;

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

#[allow(clippy::too_many_arguments)]
pub fn spawn_action_engine(
    bus: Arc<EventBus>,
    actions: Arc<dyn ActionRepo>,
    history: Arc<dyn HistoryRepo>,
    globals: Arc<dyn GlobalsRepo>,
    registry: Arc<ScriptRegistry>,
    obs_sink: Option<Arc<dyn ObsSink>>,
    sound_player: Option<Arc<dyn SoundPlayer>>,
    speak_dispatcher: Option<Arc<dyn SpeakDispatcher>>,
) -> ActionEngineHandle {
    ActionEngine::spawn(
        bus,
        actions,
        history,
        globals,
        registry,
        obs_sink,
        sound_player,
        speak_dispatcher,
    )
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::NullEventLogRepo;
    use crate::script_registry::ScriptRegistry;
    use forge_storage::{DataProvider, GlobalsRepo};
    use forge_storage_sqlite::SqliteBackend;
    use forge_types::{
        Action, ActionId, EventId, LogLevel, Queue, QueueId, SubActionSpec, Variant,
    };

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
            execution_mode: forge_types::ExecutionMode::Sequential,
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

        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let handle = spawn_action_engine(
            Arc::clone(&bus),
            dp.action_repo(),
            dp.history_repo(),
            Arc::clone(&dp) as Arc<dyn GlobalsRepo>,
            Arc::new(ScriptRegistry::new()),
            None,
            None,
            None,
        );

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

        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let mut sub = bus.subscribe();
        let handle = spawn_action_engine(
            Arc::clone(&bus),
            dp.action_repo(),
            dp.history_repo(),
            Arc::clone(&dp) as Arc<dyn GlobalsRepo>,
            Arc::new(ScriptRegistry::new()),
            None,
            None,
            None,
        );

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

        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let mut sub = bus.subscribe();
        let handle = spawn_action_engine(
            Arc::clone(&bus),
            dp.action_repo(),
            dp.history_repo(),
            Arc::clone(&dp) as Arc<dyn GlobalsRepo>,
            Arc::new(ScriptRegistry::new()),
            None,
            None,
            None,
        );

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

        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let mut sub = bus.subscribe();
        let handle = spawn_action_engine(
            Arc::clone(&bus),
            dp.action_repo(),
            dp.history_repo(),
            Arc::clone(&dp) as Arc<dyn GlobalsRepo>,
            Arc::new(ScriptRegistry::new()),
            None,
            None,
            None,
        );

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

        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let mut sub = bus.subscribe();
        let handle = spawn_action_engine(
            Arc::clone(&bus),
            dp.action_repo(),
            dp.history_repo(),
            Arc::clone(&dp) as Arc<dyn GlobalsRepo>,
            Arc::new(ScriptRegistry::new()),
            None,
            None,
            None,
        );

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

    #[tokio::test]
    async fn set_global_sub_action_emits_global_set_event_via_action_engine() {
        use std::time::Duration;

        let dp = make_dp().await;
        let action_id = ActionId::new();
        let queue_id = QueueId::new();
        let action = Action {
            id: action_id,
            name: "Set Counter".to_string(),
            group: None,
            queue_id,
            enabled: true,
            concurrent: false,
            bypass_pause: false,
            execution_mode: forge_types::ExecutionMode::Sequential,
            description: None,
            sub_actions: vec![SubActionSpec::SetGlobal {
                name: "counter".to_string(),
                value: "99".to_string(),
            }],
        };
        seed_action(&dp, &action).await;

        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let mut sub = bus.subscribe();
        let handle = spawn_action_engine(
            Arc::clone(&bus),
            dp.action_repo(),
            dp.history_repo(),
            Arc::clone(&dp) as Arc<dyn GlobalsRepo>,
            Arc::new(ScriptRegistry::new()),
            None,
            None,
            None,
        );

        handle
            .dispatch(ExecutionRequest {
                action_id,
                trigger_event_id: EventId::new(),
                initial_args: forge_types::ArgStack::new(),
            })
            .await
            .unwrap();

        let mut global_set_event = None;
        loop {
            match tokio::time::timeout(Duration::from_millis(500), sub.recv()).await {
                Ok(Ok(ev)) if ev.kind == "global.set" => {
                    global_set_event = Some(ev);
                }
                Ok(Ok(ev)) if ev.kind == "action.done" => break,
                Ok(Ok(_)) => {}
                _ => break,
            }
        }
        handle.shutdown();

        assert!(
            global_set_event.is_some(),
            "global.set event must be emitted"
        );
        let event = global_set_event.unwrap();
        assert_eq!(event.payload["key"].as_str(), Some("counter"));
        assert_eq!(event.payload["new_value"].as_str(), Some("99"));
        assert!(event.caused_by.is_some());
    }

    #[tokio::test]
    async fn increment_global_sub_action_emits_global_incr_event_via_action_engine() {
        use std::time::Duration;

        let dp = make_dp().await;
        GlobalsRepo::set(dp.as_ref(), "hits", Variant::Int(3), false)
            .await
            .unwrap();

        let action_id = ActionId::new();
        let queue_id = QueueId::new();
        let action = Action {
            id: action_id,
            name: "Incr Hits".to_string(),
            group: None,
            queue_id,
            enabled: true,
            concurrent: false,
            bypass_pause: false,
            execution_mode: forge_types::ExecutionMode::Sequential,
            description: None,
            sub_actions: vec![SubActionSpec::IncrementGlobal {
                name: "hits".to_string(),
                amount: 5,
            }],
        };
        seed_action(&dp, &action).await;

        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let mut sub = bus.subscribe();
        let handle = spawn_action_engine(
            Arc::clone(&bus),
            dp.action_repo(),
            dp.history_repo(),
            Arc::clone(&dp) as Arc<dyn GlobalsRepo>,
            Arc::new(ScriptRegistry::new()),
            None,
            None,
            None,
        );

        handle
            .dispatch(ExecutionRequest {
                action_id,
                trigger_event_id: EventId::new(),
                initial_args: forge_types::ArgStack::new(),
            })
            .await
            .unwrap();

        let mut global_incr_event = None;
        loop {
            match tokio::time::timeout(Duration::from_millis(500), sub.recv()).await {
                Ok(Ok(ev)) if ev.kind == "global.incr" => {
                    global_incr_event = Some(ev);
                }
                Ok(Ok(ev)) if ev.kind == "action.done" => break,
                Ok(Ok(_)) => {}
                _ => break,
            }
        }
        handle.shutdown();

        assert!(
            global_incr_event.is_some(),
            "global.incr event must be emitted"
        );
        let event = global_incr_event.unwrap();
        assert_eq!(event.payload["key"].as_str(), Some("hits"));
        assert_eq!(event.payload["delta"].as_i64(), Some(5));
        assert_eq!(event.payload["new_value"].as_i64(), Some(8));
        assert!(event.caused_by.is_some());
    }

    #[tokio::test]
    async fn delete_global_sub_action_emits_global_del_event_via_action_engine() {
        use std::time::Duration;

        let dp = make_dp().await;
        GlobalsRepo::set(dp.as_ref(), "temp_key", Variant::Int(1), false)
            .await
            .unwrap();

        let action_id = ActionId::new();
        let queue_id = QueueId::new();
        let action = Action {
            id: action_id,
            name: "Del Temp".to_string(),
            group: None,
            queue_id,
            enabled: true,
            concurrent: false,
            bypass_pause: false,
            execution_mode: forge_types::ExecutionMode::Sequential,
            description: None,
            sub_actions: vec![SubActionSpec::DeleteGlobal {
                name: "temp_key".to_string(),
            }],
        };
        seed_action(&dp, &action).await;

        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let mut sub = bus.subscribe();
        let handle = spawn_action_engine(
            Arc::clone(&bus),
            dp.action_repo(),
            dp.history_repo(),
            Arc::clone(&dp) as Arc<dyn GlobalsRepo>,
            Arc::new(ScriptRegistry::new()),
            None,
            None,
            None,
        );

        handle
            .dispatch(ExecutionRequest {
                action_id,
                trigger_event_id: EventId::new(),
                initial_args: forge_types::ArgStack::new(),
            })
            .await
            .unwrap();

        let mut global_del_event = None;
        loop {
            match tokio::time::timeout(Duration::from_millis(500), sub.recv()).await {
                Ok(Ok(ev)) if ev.kind == "global.del" => {
                    global_del_event = Some(ev);
                }
                Ok(Ok(ev)) if ev.kind == "action.done" => break,
                Ok(Ok(_)) => {}
                _ => break,
            }
        }
        handle.shutdown();

        assert!(
            global_del_event.is_some(),
            "global.del event must be emitted"
        );
        let event = global_del_event.unwrap();
        assert_eq!(event.payload["key"].as_str(), Some("temp_key"));
        assert!(event.caused_by.is_some());
    }

    #[tokio::test]
    async fn run_script_sub_action_executes_and_writes_global_via_action_engine() {
        use forge_storage::{GlobalsRepo, ScriptRecord, ScriptRepo};
        use forge_types::{ScriptContract, ScriptId};
        use std::time::Duration;
        use time::OffsetDateTime;

        let dp = make_dp().await;
        let queue_id = QueueId::new();
        let action_id = ActionId::new();

        let ts = OffsetDateTime::now_utc();
        let script_record = ScriptRecord {
            id: ScriptId::new(),
            name: "write_marker".to_owned(),
            body: r#"forge::globals::set("qa_marker", 1, false);"#.to_owned(),
            contract: ScriptContract::default(),
            body_hash: "qa".to_owned(),
            enabled: true,
            created_at: ts,
            last_modified: ts,
        };
        ScriptRepo::save(dp.as_ref(), script_record).await.unwrap();

        let registry = Arc::new(ScriptRegistry::new());
        registry.load_all(dp.as_ref()).await.unwrap();

        let run_script = SubActionSpec::RunScript {
            script_name: "write_marker".to_string(),
        };
        let action = Action {
            id: action_id,
            name: "QA RunScript".to_string(),
            group: None,
            queue_id,
            enabled: true,
            concurrent: false,
            bypass_pause: false,
            execution_mode: forge_types::ExecutionMode::Sequential,
            description: None,
            sub_actions: vec![run_script],
        };
        seed_action(&dp, &action).await;

        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let mut sub = bus.subscribe();
        let handle = spawn_action_engine(
            Arc::clone(&bus),
            dp.action_repo(),
            dp.history_repo(),
            Arc::clone(&dp) as Arc<dyn GlobalsRepo>,
            Arc::clone(&registry),
            None,
            None,
            None,
        );

        handle
            .dispatch(ExecutionRequest {
                action_id,
                trigger_event_id: EventId::new(),
                initial_args: forge_types::ArgStack::new(),
            })
            .await
            .unwrap();

        loop {
            match tokio::time::timeout(Duration::from_millis(2_000), sub.recv()).await {
                Ok(Ok(ev)) if ev.kind == "action.done" => break,
                Ok(Ok(_)) => {}
                _ => break,
            }
        }

        tokio::time::sleep(Duration::from_millis(50)).await;
        handle.shutdown();

        let val = GlobalsRepo::get(dp.as_ref(), "qa_marker").await.unwrap();
        assert_eq!(
            val,
            Some(forge_types::Variant::Int(1)),
            "RunScript must execute and write the global via ActionEngine"
        );
    }
}
