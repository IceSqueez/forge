//! Queue registry is loaded once at scheduler spawn time. Adding or removing
//! queues at runtime requires re-spawning the scheduler — there is no hot-reload
//! channel from storage to the scheduler.

use std::collections::HashMap;
use std::sync::Arc;

use forge_events::{Event, EventSource};
use forge_types::{ActionId, ArgStack, EventId, Queue, QueueId};
use serde_json::json;
use tokio::sync::{RwLock, Semaphore, mpsc, oneshot};
use tracing::warn;

use crate::{ActionEngineHandle, EventBus, ExecutionRequest};

pub struct SchedulerRequest {
    pub queue_id: QueueId,
    pub action_id: ActionId,
    pub trigger_event_id: EventId,
    pub initial_args: ArgStack,
    pub bypass_pause: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum SchedulerError {
    #[error("scheduler channel closed")]
    ChannelClosed,
    #[error("queue not found: {0}")]
    QueueNotFound(QueueId),
}

#[derive(Clone)]
pub struct QueueSchedulerHandle {
    sender: mpsc::UnboundedSender<SchedulerCommand>,
}

enum SchedulerCommand {
    Enqueue(SchedulerRequest),
    Pause(QueueId, oneshot::Sender<Result<(), SchedulerError>>),
    Resume(QueueId, oneshot::Sender<Result<(), SchedulerError>>),
    QueryPaused(oneshot::Sender<std::collections::HashSet<QueueId>>),
    Shutdown,
}

struct QueueSlot {
    sender: mpsc::UnboundedSender<QueueTask>,
    state: Arc<RwLock<PauseState>>,
    name: String,
}

struct PauseState {
    paused: bool,
}

struct QueueTask {
    action_id: ActionId,
    trigger_event_id: EventId,
    initial_args: ArgStack,
}

impl QueueSchedulerHandle {
    pub async fn dispatch(&self, req: SchedulerRequest) -> Result<(), SchedulerError> {
        self.sender
            .send(SchedulerCommand::Enqueue(req))
            .map_err(|_| SchedulerError::ChannelClosed)
    }

    pub async fn pause(&self, queue_id: QueueId) -> Result<(), SchedulerError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(SchedulerCommand::Pause(queue_id, tx))
            .map_err(|_| SchedulerError::ChannelClosed)?;
        rx.await.map_err(|_| SchedulerError::ChannelClosed)?
    }

    pub async fn resume(&self, queue_id: QueueId) -> Result<(), SchedulerError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(SchedulerCommand::Resume(queue_id, tx))
            .map_err(|_| SchedulerError::ChannelClosed)?;
        rx.await.map_err(|_| SchedulerError::ChannelClosed)?
    }

    pub fn shutdown(self) {
        let _ = self.sender.send(SchedulerCommand::Shutdown);
    }

    pub async fn paused_queues(
        &self,
    ) -> Result<std::collections::HashSet<QueueId>, SchedulerError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(SchedulerCommand::QueryPaused(tx))
            .map_err(|_| SchedulerError::ChannelClosed)?;
        rx.await.map_err(|_| SchedulerError::ChannelClosed)
    }
}

pub struct QueueScheduler;

impl QueueScheduler {
    pub fn spawn(
        engine: ActionEngineHandle,
        bus: Arc<EventBus>,
        initial_queues: Vec<Queue>,
    ) -> QueueSchedulerHandle {
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        let engine = Arc::new(engine);

        let mut slots: HashMap<QueueId, QueueSlot> = HashMap::with_capacity(initial_queues.len());
        for queue in initial_queues {
            let id = queue.id;
            let slot = Self::make_queue_slot(queue, Arc::clone(&engine));
            slots.insert(id, slot);
        }

        tokio::spawn(Self::run_scheduler(cmd_rx, slots, bus));

        QueueSchedulerHandle { sender: cmd_tx }
    }

    fn make_queue_slot(queue: Queue, engine: Arc<ActionEngineHandle>) -> QueueSlot {
        let (task_tx, task_rx) = mpsc::unbounded_channel::<QueueTask>();
        let state = Arc::new(RwLock::new(PauseState { paused: false }));
        let name = queue.name.clone();

        if queue.blocking {
            let sem = Arc::new(Semaphore::new(1));
            tokio::spawn(Self::run_blocking(task_rx, engine, sem));
        } else {
            tokio::spawn(Self::run_nonblocking(task_rx, engine));
        }

        QueueSlot {
            sender: task_tx,
            state,
            name,
        }
    }

    async fn run_blocking(
        mut rx: mpsc::UnboundedReceiver<QueueTask>,
        engine: Arc<ActionEngineHandle>,
        sem: Arc<Semaphore>,
    ) {
        while let Some(task) = rx.recv().await {
            let permit = match Arc::clone(&sem).acquire_owned().await {
                Ok(p) => p,
                Err(_) => break,
            };

            let req = ExecutionRequest {
                action_id: task.action_id,
                trigger_event_id: task.trigger_event_id,
                initial_args: task.initial_args,
            };

            let (done_tx, done_rx) = oneshot::channel::<()>();
            let engine_ref = Arc::clone(&engine);

            tokio::spawn(async move {
                let _ = engine_ref.dispatch(req).await;
                let _ = done_tx.send(());
            });

            let _ = done_rx.await;
            drop(permit);
        }
    }

    async fn run_nonblocking(
        mut rx: mpsc::UnboundedReceiver<QueueTask>,
        engine: Arc<ActionEngineHandle>,
    ) {
        while let Some(task) = rx.recv().await {
            let req = ExecutionRequest {
                action_id: task.action_id,
                trigger_event_id: task.trigger_event_id,
                initial_args: task.initial_args,
            };

            let engine_ref = Arc::clone(&engine);
            tokio::spawn(async move {
                let _ = engine_ref.dispatch(req).await;
            });
        }
    }

    async fn run_scheduler(
        mut cmd_rx: mpsc::UnboundedReceiver<SchedulerCommand>,
        slots: HashMap<QueueId, QueueSlot>,
        bus: Arc<EventBus>,
    ) {
        while let Some(cmd) = cmd_rx.recv().await {
            match cmd {
                SchedulerCommand::Enqueue(req) => {
                    Self::enqueue(req, &slots, &bus).await;
                }
                SchedulerCommand::Pause(queue_id, reply) => {
                    let r = Self::set_paused(&queue_id, true, &slots, &bus, "queue.paused").await;
                    let _ = reply.send(r);
                }
                SchedulerCommand::Resume(queue_id, reply) => {
                    let r = Self::set_paused(&queue_id, false, &slots, &bus, "queue.resumed").await;
                    let _ = reply.send(r);
                }
                SchedulerCommand::QueryPaused(reply) => {
                    let mut paused = std::collections::HashSet::new();
                    for (id, slot) in &slots {
                        if slot.state.read().await.paused {
                            paused.insert(*id);
                        }
                    }
                    let _ = reply.send(paused);
                }
                SchedulerCommand::Shutdown => break,
            }
        }
    }

    async fn enqueue(
        req: SchedulerRequest,
        slots: &HashMap<QueueId, QueueSlot>,
        bus: &Arc<EventBus>,
    ) {
        let slot = match slots.get(&req.queue_id) {
            Some(s) => s,
            None => {
                warn!("enqueue: queue {} not found, dropping", req.queue_id);
                bus.publish(Event::new(
                    EventSource::Core,
                    "action.skipped",
                    json!({
                        "action_id": req.action_id.to_string(),
                        "reason": "queue_not_found",
                        "queue_id": req.queue_id.to_string(),
                    }),
                ));
                return;
            }
        };

        if !req.bypass_pause {
            let paused = slot.state.read().await.paused;
            if paused {
                bus.publish(Event::new(
                    EventSource::Core,
                    "action.skipped",
                    json!({
                        "action_id": req.action_id.to_string(),
                        "reason": "queue_paused",
                        "queue_id": req.queue_id.to_string(),
                    }),
                ));
                return;
            }
        }

        let task = QueueTask {
            action_id: req.action_id,
            trigger_event_id: req.trigger_event_id,
            initial_args: req.initial_args,
        };

        if slot.sender.send(task).is_err() {
            warn!("queue task channel closed for queue {}", req.queue_id);
        }
    }

    async fn set_paused(
        queue_id: &QueueId,
        paused: bool,
        slots: &HashMap<QueueId, QueueSlot>,
        bus: &Arc<EventBus>,
        event_kind: &'static str,
    ) -> Result<(), SchedulerError> {
        let slot = slots
            .get(queue_id)
            .ok_or(SchedulerError::QueueNotFound(*queue_id))?;

        {
            let mut state = slot.state.write().await;
            state.paused = paused;
        }

        bus.publish(Event::new(
            EventSource::Core,
            event_kind,
            json!({
                "queue_id": queue_id.to_string(),
                "queue_name": slot.name,
            }),
        ));

        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use forge_storage::{DataProvider, GlobalsRepo};
    use forge_storage_sqlite::SqliteBackend;
    use forge_types::{Action, ActionId, EventId, LogLevel, Queue, QueueId, SubActionSpec};

    use super::*;
    use crate::{
        EventBus, EventSubscription, NullEventLogRepo, ScriptRegistry, spawn_action_engine,
    };

    async fn make_dp() -> Arc<dyn DataProvider> {
        Arc::new(
            SqliteBackend::open_with_key(":memory:", [0xab; 32])
                .await
                .unwrap(),
        )
    }

    async fn seed(dp: &Arc<dyn DataProvider>, queue: &Queue, action: &Action) {
        dp.queue_repo().save(queue).await.unwrap();
        dp.action_repo().save(action).await.unwrap();
    }

    fn log_action(id: ActionId, queue_id: QueueId) -> Action {
        Action {
            id,
            name: "test".to_string(),
            group: None,
            queue_id,
            enabled: true,
            concurrent: false,
            bypass_pause: false,
            execution_mode: forge_types::ExecutionMode::Sequential,
            description: None,
            sub_actions: vec![SubActionSpec::Log {
                level: LogLevel::Info,
                message: "ping".to_string(),
            }],
        }
    }

    fn nonblocking(id: QueueId) -> Queue {
        Queue {
            id,
            name: "default".to_string(),
            blocking: false,
        }
    }

    fn blocking_q(id: QueueId) -> Queue {
        Queue {
            id,
            name: "serial".to_string(),
            blocking: true,
        }
    }

    async fn collect_events(
        sub: &mut EventSubscription,
        target_kind: &str,
        max_attempts: usize,
        timeout_ms: u64,
    ) -> bool {
        for _ in 0..max_attempts {
            match tokio::time::timeout(Duration::from_millis(timeout_ms), sub.recv()).await {
                Ok(Ok(ev)) if ev.kind == target_kind => return true,
                Ok(Ok(_)) => {}
                _ => break,
            }
        }
        false
    }

    #[tokio::test]
    async fn nonblocking_queue_executes_request() {
        let dp = make_dp().await;
        let q_id = QueueId::new();
        let a_id = ActionId::new();
        let queue = nonblocking(q_id);
        let action = log_action(a_id, q_id);
        seed(&dp, &queue, &action).await;

        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let engine = spawn_action_engine(
            Arc::clone(&bus),
            dp.action_repo(),
            dp.history_repo(),
            Arc::clone(&dp) as Arc<dyn GlobalsRepo>,
            Arc::new(ScriptRegistry::new()),
            None,
            None,
            None,
        );
        let sched = QueueScheduler::spawn(engine, Arc::clone(&bus), vec![queue]);
        let mut sub = bus.subscribe();

        sched
            .dispatch(SchedulerRequest {
                queue_id: q_id,
                action_id: a_id,
                trigger_event_id: EventId::new(),
                initial_args: ArgStack::new(),
                bypass_pause: false,
            })
            .await
            .unwrap();

        assert!(
            collect_events(&mut sub, "action.done", 20, 300).await,
            "action.done must arrive for non-blocking queue"
        );
        sched.shutdown();
    }

    #[tokio::test]
    async fn blocking_queue_serializes_three_requests() {
        let dp = make_dp().await;
        let q_id = QueueId::new();
        let queue = blocking_q(q_id);
        dp.queue_repo().save(&queue).await.unwrap();

        let mut action_ids = Vec::new();
        for _ in 0..3 {
            let a_id = ActionId::new();
            let action = log_action(a_id, q_id);
            dp.action_repo().save(&action).await.unwrap();
            action_ids.push(a_id);
        }

        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let engine = spawn_action_engine(
            Arc::clone(&bus),
            dp.action_repo(),
            dp.history_repo(),
            Arc::clone(&dp) as Arc<dyn GlobalsRepo>,
            Arc::new(ScriptRegistry::new()),
            None,
            None,
            None,
        );
        let sched = QueueScheduler::spawn(engine, Arc::clone(&bus), vec![queue]);
        let mut sub = bus.subscribe();

        for a_id in &action_ids {
            sched
                .dispatch(SchedulerRequest {
                    queue_id: q_id,
                    action_id: *a_id,
                    trigger_event_id: EventId::new(),
                    initial_args: ArgStack::new(),
                    bypass_pause: false,
                })
                .await
                .unwrap();
        }

        let mut done_count = 0;
        for _ in 0..60 {
            match tokio::time::timeout(Duration::from_millis(300), sub.recv()).await {
                Ok(Ok(ev)) if ev.kind == "action.done" => {
                    done_count += 1;
                    if done_count == 3 {
                        break;
                    }
                }
                Ok(Ok(_)) => {}
                _ => break,
            }
        }

        assert_eq!(done_count, 3, "all three serialized actions must complete");
        sched.shutdown();
    }

    #[tokio::test]
    async fn paused_queue_emits_skipped_and_does_not_execute() {
        let dp = make_dp().await;
        let q_id = QueueId::new();
        let a_id = ActionId::new();
        let queue = nonblocking(q_id);
        let action = log_action(a_id, q_id);
        seed(&dp, &queue, &action).await;

        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let engine = spawn_action_engine(
            Arc::clone(&bus),
            dp.action_repo(),
            dp.history_repo(),
            Arc::clone(&dp) as Arc<dyn GlobalsRepo>,
            Arc::new(ScriptRegistry::new()),
            None,
            None,
            None,
        );
        let sched = QueueScheduler::spawn(engine, Arc::clone(&bus), vec![queue]);
        let mut sub = bus.subscribe();

        sched.pause(q_id).await.unwrap();

        sched
            .dispatch(SchedulerRequest {
                queue_id: q_id,
                action_id: a_id,
                trigger_event_id: EventId::new(),
                initial_args: ArgStack::new(),
                bypass_pause: false,
            })
            .await
            .unwrap();

        tokio::time::sleep(Duration::from_millis(120)).await;

        let mut saw_skipped = false;
        let mut saw_done = false;

        for _ in 0..20 {
            match tokio::time::timeout(Duration::from_millis(30), sub.recv()).await {
                Ok(Ok(ev)) if ev.kind == "action.skipped" => saw_skipped = true,
                Ok(Ok(ev)) if ev.kind == "action.done" => saw_done = true,
                Ok(Ok(_)) => {}
                _ => break,
            }
        }

        assert!(saw_skipped, "paused queue must emit action.skipped");
        assert!(!saw_done, "paused queue must not execute action");
        sched.shutdown();
    }

    #[tokio::test]
    async fn bypass_pause_executes_through_paused_queue() {
        let dp = make_dp().await;
        let q_id = QueueId::new();
        let a_id = ActionId::new();
        let queue = nonblocking(q_id);
        let action = log_action(a_id, q_id);
        seed(&dp, &queue, &action).await;

        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let engine = spawn_action_engine(
            Arc::clone(&bus),
            dp.action_repo(),
            dp.history_repo(),
            Arc::clone(&dp) as Arc<dyn GlobalsRepo>,
            Arc::new(ScriptRegistry::new()),
            None,
            None,
            None,
        );
        let sched = QueueScheduler::spawn(engine, Arc::clone(&bus), vec![queue]);
        let mut sub = bus.subscribe();

        sched.pause(q_id).await.unwrap();

        sched
            .dispatch(SchedulerRequest {
                queue_id: q_id,
                action_id: a_id,
                trigger_event_id: EventId::new(),
                initial_args: ArgStack::new(),
                bypass_pause: true,
            })
            .await
            .unwrap();

        assert!(
            collect_events(&mut sub, "action.done", 30, 300).await,
            "bypass_pause must execute despite paused queue"
        );
        sched.shutdown();
    }

    #[tokio::test]
    async fn resume_unblocks_subsequent_dispatches() {
        let dp = make_dp().await;
        let q_id = QueueId::new();
        let a_id = ActionId::new();
        let queue = nonblocking(q_id);
        let action = log_action(a_id, q_id);
        seed(&dp, &queue, &action).await;

        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let engine = spawn_action_engine(
            Arc::clone(&bus),
            dp.action_repo(),
            dp.history_repo(),
            Arc::clone(&dp) as Arc<dyn GlobalsRepo>,
            Arc::new(ScriptRegistry::new()),
            None,
            None,
            None,
        );
        let sched = QueueScheduler::spawn(engine, Arc::clone(&bus), vec![queue]);
        let mut sub = bus.subscribe();

        sched.pause(q_id).await.unwrap();
        sched.resume(q_id).await.unwrap();

        sched
            .dispatch(SchedulerRequest {
                queue_id: q_id,
                action_id: a_id,
                trigger_event_id: EventId::new(),
                initial_args: ArgStack::new(),
                bypass_pause: false,
            })
            .await
            .unwrap();

        assert!(
            collect_events(&mut sub, "action.done", 30, 300).await,
            "resumed queue must execute actions"
        );
        sched.shutdown();
    }

    #[tokio::test]
    async fn pause_emits_queue_paused_event() {
        let dp = make_dp().await;
        let q_id = QueueId::new();
        let queue = nonblocking(q_id);
        dp.queue_repo().save(&queue).await.unwrap();

        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let engine = spawn_action_engine(
            Arc::clone(&bus),
            dp.action_repo(),
            dp.history_repo(),
            Arc::clone(&dp) as Arc<dyn GlobalsRepo>,
            Arc::new(ScriptRegistry::new()),
            None,
            None,
            None,
        );
        let sched = QueueScheduler::spawn(engine, Arc::clone(&bus), vec![queue]);
        let mut sub = bus.subscribe();

        sched.pause(q_id).await.unwrap();

        assert!(
            collect_events(&mut sub, "queue.paused", 10, 200).await,
            "pause must emit queue.paused"
        );
        sched.shutdown();
    }

    #[tokio::test]
    async fn resume_emits_queue_resumed_event() {
        let dp = make_dp().await;
        let q_id = QueueId::new();
        let queue = nonblocking(q_id);
        dp.queue_repo().save(&queue).await.unwrap();

        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let engine = spawn_action_engine(
            Arc::clone(&bus),
            dp.action_repo(),
            dp.history_repo(),
            Arc::clone(&dp) as Arc<dyn GlobalsRepo>,
            Arc::new(ScriptRegistry::new()),
            None,
            None,
            None,
        );
        let sched = QueueScheduler::spawn(engine, Arc::clone(&bus), vec![queue]);
        let mut sub = bus.subscribe();

        sched.pause(q_id).await.unwrap();
        sched.resume(q_id).await.unwrap();

        assert!(
            collect_events(&mut sub, "queue.resumed", 10, 200).await,
            "resume must emit queue.resumed"
        );
        sched.shutdown();
    }

    #[tokio::test]
    async fn unknown_queue_emits_action_skipped() {
        let dp = make_dp().await;
        let q_id = QueueId::new();
        let unknown_q = QueueId::new();
        let a_id = ActionId::new();
        let queue = nonblocking(q_id);
        let action = log_action(a_id, q_id);
        seed(&dp, &queue, &action).await;

        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let engine = spawn_action_engine(
            Arc::clone(&bus),
            dp.action_repo(),
            dp.history_repo(),
            Arc::clone(&dp) as Arc<dyn GlobalsRepo>,
            Arc::new(ScriptRegistry::new()),
            None,
            None,
            None,
        );
        let sched = QueueScheduler::spawn(engine, Arc::clone(&bus), vec![queue]);
        let mut sub = bus.subscribe();

        sched
            .dispatch(SchedulerRequest {
                queue_id: unknown_q,
                action_id: a_id,
                trigger_event_id: EventId::new(),
                initial_args: ArgStack::new(),
                bypass_pause: false,
            })
            .await
            .unwrap();

        assert!(
            collect_events(&mut sub, "action.skipped", 10, 200).await,
            "unknown queue must emit action.skipped"
        );
        sched.shutdown();
    }
}
