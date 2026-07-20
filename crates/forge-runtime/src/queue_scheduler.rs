//! The scheduler's queue registry is seeded at spawn and kept live thereafter
//! through register/deregister/reconfigure commands. Dropping a slot's `sender`
//! closes its task channel; the spawned runner then drains its buffered tasks
//! and exits when `recv()` returns `None` - this is the membership-change drain
//! guarantee, not a leak.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use arc_swap::ArcSwapOption;
use forge_events::{Event, EventSource};
use forge_registry::CancelSignal;
use forge_types::{ActionId, ArgStack, EventId, Queue, QueueId};
use serde_json::json;
use tokio::sync::{RwLock, Semaphore, mpsc, oneshot};
use tokio::task::JoinHandle;
use tracing::warn;

use crate::{ActionEngineHandle, EventBus, ExecutionRequest};

/// Lock-free, settable holder for the live `QueueSchedulerHandle`. Queue-control
/// sub-action runners are registered at boot, before the scheduler task exists;
/// this cell is handed to them empty and filled once `QueueScheduler::spawn`
/// returns, so runners reach the live scheduler without a registration-order
/// dependency.
#[derive(Clone, Default)]
pub struct SchedulerCell {
    inner: Arc<ArcSwapOption<QueueSchedulerHandle>>,
}

impl SchedulerCell {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&self, handle: QueueSchedulerHandle) {
        self.inner.store(Some(Arc::new(handle)));
    }

    pub fn get(&self) -> Option<QueueSchedulerHandle> {
        self.inner.load_full().map(|h| (*h).clone())
    }
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MembershipOutcome {
    Applied,
    AlreadyRegistered,
    NotFound,
}

#[derive(Clone)]
pub struct QueueSchedulerHandle {
    sender: mpsc::UnboundedSender<SchedulerCommand>,
}

enum SchedulerCommand {
    Enqueue(SchedulerRequest),
    Pause(QueueId, oneshot::Sender<Result<(), SchedulerError>>),
    Resume(QueueId, oneshot::Sender<Result<(), SchedulerError>>),
    Clear(QueueId, bool, oneshot::Sender<Result<(), SchedulerError>>),
    Register(Queue, oneshot::Sender<MembershipOutcome>),
    Deregister(QueueId, oneshot::Sender<MembershipOutcome>),
    Reconfigure(Queue, oneshot::Sender<MembershipOutcome>),
    QueryPaused(oneshot::Sender<std::collections::HashSet<QueueId>>),
    Shutdown,
}

struct QueueSlot {
    sender: mpsc::UnboundedSender<QueueTask>,
    state: Arc<RwLock<PauseState>>,
    name: String,
    concurrency: u32,
    runner: JoinHandle<()>,
    inflight: InflightTracker,
}

/// Holds the cancel signal of each execution a queue's runner has started but
/// not yet seen finish, so `Clear` with `keep_current = false` can cooperatively
/// cancel the running chain(s). A runner registers before dispatching and removes
/// on completion, so a finished execution is never cancelled retroactively.
#[derive(Clone, Default)]
struct InflightTracker {
    inner: Arc<Mutex<InflightInner>>,
}

#[derive(Default)]
struct InflightInner {
    next_id: u64,
    signals: HashMap<u64, CancelSignal>,
}

impl InflightTracker {
    fn register(&self, signal: CancelSignal) -> u64 {
        let mut inner = self.lock();
        let id = inner.next_id;
        inner.next_id = inner.next_id.wrapping_add(1);
        inner.signals.insert(id, signal);
        id
    }

    fn complete(&self, id: u64) {
        self.lock().signals.remove(&id);
    }

    fn cancel_all(&self) {
        for (_, signal) in self.lock().signals.drain() {
            signal.cancel();
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, InflightInner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }
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

    /// Discards the queue's pending (not-yet-started) executions. With
    /// `keep_current = false` the in-flight execution is cancelled cooperatively
    /// too; with `true` it runs to completion.
    pub async fn clear(&self, queue_id: QueueId, keep_current: bool) -> Result<(), SchedulerError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(SchedulerCommand::Clear(queue_id, keep_current, tx))
            .map_err(|_| SchedulerError::ChannelClosed)?;
        rx.await.map_err(|_| SchedulerError::ChannelClosed)?
    }

    pub async fn register(&self, queue: Queue) -> Result<MembershipOutcome, SchedulerError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(SchedulerCommand::Register(queue, tx))
            .map_err(|_| SchedulerError::ChannelClosed)?;
        rx.await.map_err(|_| SchedulerError::ChannelClosed)
    }

    pub async fn deregister(&self, queue_id: QueueId) -> Result<MembershipOutcome, SchedulerError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(SchedulerCommand::Deregister(queue_id, tx))
            .map_err(|_| SchedulerError::ChannelClosed)?;
        rx.await.map_err(|_| SchedulerError::ChannelClosed)
    }

    pub async fn reconfigure(&self, queue: Queue) -> Result<MembershipOutcome, SchedulerError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(SchedulerCommand::Reconfigure(queue, tx))
            .map_err(|_| SchedulerError::ChannelClosed)?;
        rx.await.map_err(|_| SchedulerError::ChannelClosed)
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

        tokio::spawn(Self::run_scheduler(cmd_rx, slots, bus, engine));

        QueueSchedulerHandle { sender: cmd_tx }
    }

    fn make_queue_slot(queue: Queue, engine: Arc<ActionEngineHandle>) -> QueueSlot {
        let (task_tx, task_rx) = mpsc::unbounded_channel::<QueueTask>();
        let state = Arc::new(RwLock::new(PauseState {
            paused: queue.paused,
        }));
        let inflight = InflightTracker::default();
        let name = queue.name.clone();
        let concurrency = queue.concurrency.max(1);

        let sem = Arc::new(Semaphore::new(concurrency as usize));
        let runner = tokio::spawn(Self::run_bounded(task_rx, engine, sem, inflight.clone()));

        QueueSlot {
            sender: task_tx,
            state,
            name,
            concurrency,
            runner,
            inflight,
        }
    }

    async fn run_bounded(
        mut rx: mpsc::UnboundedReceiver<QueueTask>,
        engine: Arc<ActionEngineHandle>,
        sem: Arc<Semaphore>,
        inflight: InflightTracker,
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

            let cancel = CancelSignal::new();
            let id = inflight.register(cancel.clone());
            let engine_ref = Arc::clone(&engine);
            let inflight_ref = inflight.clone();
            tokio::spawn(async move {
                let (done_tx, done_rx) = oneshot::channel::<()>();
                if engine_ref
                    .dispatch_tracked(req, cancel, done_tx)
                    .await
                    .is_ok()
                {
                    let _ = done_rx.await;
                }
                inflight_ref.complete(id);
                drop(permit);
            });
        }
    }

    async fn run_scheduler(
        mut cmd_rx: mpsc::UnboundedReceiver<SchedulerCommand>,
        mut slots: HashMap<QueueId, QueueSlot>,
        bus: Arc<EventBus>,
        engine: Arc<ActionEngineHandle>,
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
                SchedulerCommand::Clear(queue_id, keep_current, reply) => {
                    let r =
                        Self::clear_queue(&mut slots, &queue_id, keep_current, &bus, &engine).await;
                    let _ = reply.send(r);
                }
                SchedulerCommand::Register(queue, reply) => {
                    let outcome = if slots.contains_key(&queue.id) {
                        MembershipOutcome::AlreadyRegistered
                    } else {
                        let id = queue.id;
                        let slot = Self::make_queue_slot(queue, Arc::clone(&engine));
                        slots.insert(id, slot);
                        MembershipOutcome::Applied
                    };
                    let _ = reply.send(outcome);
                }
                SchedulerCommand::Deregister(queue_id, reply) => {
                    let outcome = match slots.remove(&queue_id) {
                        Some(_) => MembershipOutcome::Applied,
                        None => MembershipOutcome::NotFound,
                    };
                    let _ = reply.send(outcome);
                }
                SchedulerCommand::Reconfigure(queue, reply) => {
                    let outcome = match slots.get_mut(&queue.id) {
                        Some(slot) if slot.concurrency == queue.concurrency => {
                            slot.name = queue.name;
                            MembershipOutcome::Applied
                        }
                        Some(old) => {
                            // A concurrency change rebuilds the runner, but must carry pause
                            // state forward, else a config edit silently resumes a paused
                            // queue with no event.
                            let was_paused = old.state.read().await.paused;
                            let id = queue.id;
                            let slot = Self::make_queue_slot(queue, Arc::clone(&engine));
                            slot.state.write().await.paused = was_paused;
                            slots.insert(id, slot);
                            MembershipOutcome::Applied
                        }
                        None => MembershipOutcome::NotFound,
                    };
                    let _ = reply.send(outcome);
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

    async fn clear_queue(
        slots: &mut HashMap<QueueId, QueueSlot>,
        queue_id: &QueueId,
        keep_current: bool,
        bus: &Arc<EventBus>,
        engine: &Arc<ActionEngineHandle>,
    ) -> Result<(), SchedulerError> {
        let (was_paused, name, concurrency) = {
            let slot = slots
                .get(queue_id)
                .ok_or(SchedulerError::QueueNotFound(*queue_id))?;

            if !keep_current {
                slot.inflight.cancel_all();
            }
            // Aborting the runner drops its task receiver, discarding every
            // buffered (not-yet-started) execution. With keep_current = false the
            // running chain unwinds cooperatively via the cancel signal set above;
            // with true it keeps running to completion in the engine.
            slot.runner.abort();

            (
                slot.state.read().await.paused,
                slot.name.clone(),
                slot.concurrency,
            )
        };

        let rebuilt = Self::make_queue_slot(
            Queue {
                id: *queue_id,
                name: name.clone(),
                description: String::new(),
                concurrency,
                paused: was_paused,
            },
            Arc::clone(engine),
        );
        slots.insert(*queue_id, rebuilt);

        bus.publish(Event::new(
            EventSource::Core,
            "queue.cleared",
            json!({
                "queue_id": queue_id.to_string(),
                "queue_name": name,
                "keep_current": keep_current,
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

    use forge_registry::SubActionRegistry;
    use forge_storage::DataProvider;
    use forge_storage_sqlite::SqliteBackend;
    use forge_types::{Action, ActionId, EventId, Queue, QueueId, SubActionStep, Variant};

    use super::*;
    use crate::sub_action_runners::CoreLogicWaitRunner;
    use crate::{EventBus, EventSubscription, NullEventLogRepo, spawn_action_engine};

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
            sub_actions: vec![SubActionStep {
                kind_id: "core.log.write".to_owned(),
                config: std::collections::BTreeMap::new(),
                enabled: true,
                continue_on_error: false,
                condition: None,
                label: None,
            }],
        }
    }

    fn nonblocking(id: QueueId) -> Queue {
        Queue {
            id,
            name: "default".to_string(),
            description: String::new(),
            concurrency: 8,
            paused: false,
        }
    }

    fn blocking_q(id: QueueId) -> Queue {
        Queue {
            id,
            name: "serial".to_string(),
            description: String::new(),
            concurrency: 1,
            paused: false,
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
            Arc::new(SubActionRegistry::new()),
            Arc::new(crate::action_cancel::ActionCancelRegistry::new()),
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
            Arc::new(SubActionRegistry::new()),
            Arc::new(crate::action_cancel::ActionCancelRegistry::new()),
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
            Arc::new(SubActionRegistry::new()),
            Arc::new(crate::action_cancel::ActionCancelRegistry::new()),
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
            Arc::new(SubActionRegistry::new()),
            Arc::new(crate::action_cancel::ActionCancelRegistry::new()),
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
            Arc::new(SubActionRegistry::new()),
            Arc::new(crate::action_cancel::ActionCancelRegistry::new()),
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
            Arc::new(SubActionRegistry::new()),
            Arc::new(crate::action_cancel::ActionCancelRegistry::new()),
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
            Arc::new(SubActionRegistry::new()),
            Arc::new(crate::action_cancel::ActionCancelRegistry::new()),
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
            Arc::new(SubActionRegistry::new()),
            Arc::new(crate::action_cancel::ActionCancelRegistry::new()),
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

    // ── live-membership: register ────────────────────────────────────────────

    #[tokio::test]
    async fn register_new_queue_returns_applied_and_enables_dispatch() {
        // Core fix verification: a queue registered post-spawn must actually
        // receive and execute work (not emit queue_not_found skipped).
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
            Arc::new(SubActionRegistry::new()),
            Arc::new(crate::action_cancel::ActionCancelRegistry::new()),
        );
        // Spawn with NO initial queues - q_id is unregistered.
        let sched = QueueScheduler::spawn(engine, Arc::clone(&bus), vec![]);
        let mut sub = bus.subscribe();

        let outcome = sched.register(queue).await.unwrap();
        assert_eq!(outcome, MembershipOutcome::Applied);

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

        // Must reach action.done, NOT action.skipped with reason=queue_not_found.
        assert!(
            collect_events(&mut sub, "action.done", 30, 300).await,
            "newly registered queue must execute dispatched actions"
        );
        sched.shutdown();
    }

    #[tokio::test]
    async fn register_existing_queue_returns_already_registered_and_preserves_pause() {
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
            Arc::new(SubActionRegistry::new()),
            Arc::new(crate::action_cancel::ActionCancelRegistry::new()),
        );
        let sched = QueueScheduler::spawn(engine, Arc::clone(&bus), vec![nonblocking(q_id)]);
        let mut sub = bus.subscribe();

        // Pause the slot, then attempt to re-register the same id.
        sched.pause(q_id).await.unwrap();
        let outcome = sched.register(nonblocking(q_id)).await.unwrap();
        assert_eq!(outcome, MembershipOutcome::AlreadyRegistered);

        // Existing slot must remain paused (not replaced by a fresh unpaused slot).
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

        // Drain briefly: must see skipped (still paused), not done.
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
        assert!(saw_skipped, "re-register must not un-pause existing slot");
        assert!(
            !saw_done,
            "re-register must not replace existing paused slot"
        );
        sched.shutdown();
    }

    // ── live-membership: deregister ─────────────────────────────────────────

    #[tokio::test]
    async fn deregister_existing_queue_returns_applied_then_skips_dispatch() {
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
            Arc::new(SubActionRegistry::new()),
            Arc::new(crate::action_cancel::ActionCancelRegistry::new()),
        );
        let sched = QueueScheduler::spawn(engine, Arc::clone(&bus), vec![queue]);
        let mut sub = bus.subscribe();

        let outcome = sched.deregister(q_id).await.unwrap();
        assert_eq!(outcome, MembershipOutcome::Applied);

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

        // Dispatch after deregister must hit the queue_not_found safety net.
        assert!(
            collect_events(&mut sub, "action.skipped", 10, 200).await,
            "dispatch after deregister must emit action.skipped"
        );
        sched.shutdown();
    }

    #[tokio::test]
    async fn deregister_unknown_queue_returns_not_found_without_panic() {
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let dp = make_dp().await;
        let engine = spawn_action_engine(
            Arc::clone(&bus),
            dp.action_repo(),
            dp.history_repo(),
            Arc::new(SubActionRegistry::new()),
            Arc::new(crate::action_cancel::ActionCancelRegistry::new()),
        );
        let sched = QueueScheduler::spawn(engine, Arc::clone(&bus), vec![]);

        let outcome = sched.deregister(QueueId::new()).await.unwrap();
        assert_eq!(outcome, MembershipOutcome::NotFound);
        sched.shutdown();
    }

    // ── live-membership: reconfigure ────────────────────────────────────────

    #[tokio::test]
    async fn reconfigure_rename_preserves_pause_state_and_runner() {
        // Same `blocking` value → only `name` is updated; runner is NOT torn
        // down and rebuilt. A paused queue must remain paused after the rename.
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
            Arc::new(SubActionRegistry::new()),
            Arc::new(crate::action_cancel::ActionCancelRegistry::new()),
        );
        let sched = QueueScheduler::spawn(engine, Arc::clone(&bus), vec![nonblocking(q_id)]);
        let mut sub = bus.subscribe();

        sched.pause(q_id).await.unwrap();

        // Rename only - blocking stays false.
        let renamed = Queue {
            id: q_id,
            name: "renamed".to_string(),
            description: String::new(),
            concurrency: 8,
            paused: false,
        };
        let outcome = sched.reconfigure(renamed).await.unwrap();
        assert_eq!(outcome, MembershipOutcome::Applied);

        // Dispatch: must still be skipped (pause survived rename).
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
        assert!(
            saw_skipped,
            "rename-only reconfigure must not clear pause state"
        );
        assert!(
            !saw_done,
            "rename-only reconfigure must not replace the paused slot"
        );
        sched.shutdown();
    }

    #[tokio::test]
    async fn reconfigure_blocking_flip_accepts_work_after_rebuild() {
        // blocking-flip builds a fresh slot; the queue must still accept and
        // execute dispatched work.
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
            Arc::new(SubActionRegistry::new()),
            Arc::new(crate::action_cancel::ActionCancelRegistry::new()),
        );
        // Start non-blocking, flip to blocking.
        let sched = QueueScheduler::spawn(engine, Arc::clone(&bus), vec![nonblocking(q_id)]);
        let mut sub = bus.subscribe();

        let flipped = blocking_q(q_id);
        let outcome = sched.reconfigure(flipped).await.unwrap();
        assert_eq!(outcome, MembershipOutcome::Applied);

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
            "queue must execute work after blocking-flip reconfigure"
        );
        sched.shutdown();
    }

    #[tokio::test]
    async fn reconfigure_blocking_flip_preserves_pause() {
        // Regression: before 8bc1b04, a blocking-flip reconfigure rebuilt the
        // slot with paused=false, silently resuming a paused queue.  After the
        // fix, `was_paused` is carried forward into the new slot.
        //
        // Arrange: non-blocking queue, paused before the flip.
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
            Arc::new(SubActionRegistry::new()),
            Arc::new(crate::action_cancel::ActionCancelRegistry::new()),
        );
        let sched = QueueScheduler::spawn(engine, Arc::clone(&bus), vec![nonblocking(q_id)]);
        let mut sub = bus.subscribe();

        // Pause first, then flip to blocking.
        sched.pause(q_id).await.unwrap();
        let outcome = sched.reconfigure(blocking_q(q_id)).await.unwrap();
        assert_eq!(outcome, MembershipOutcome::Applied);

        // Dispatch into the rebuilt (now blocking) queue.
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

        // Allow time for the action to execute if it were to slip through.
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

        assert!(
            saw_skipped,
            "blocking-flip reconfigure must preserve pause state: action.skipped expected"
        );
        assert!(
            !saw_done,
            "blocking-flip reconfigure must not silently resume a paused queue"
        );
        sched.shutdown();
    }

    #[tokio::test]
    async fn reconfigure_unknown_queue_returns_not_found() {
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let dp = make_dp().await;
        let engine = spawn_action_engine(
            Arc::clone(&bus),
            dp.action_repo(),
            dp.history_repo(),
            Arc::new(SubActionRegistry::new()),
            Arc::new(crate::action_cancel::ActionCancelRegistry::new()),
        );
        let sched = QueueScheduler::spawn(engine, Arc::clone(&bus), vec![]);

        let outcome = sched
            .reconfigure(Queue {
                id: QueueId::new(),
                name: "ghost".to_string(),
                description: String::new(),
                concurrency: 8,
                paused: false,
            })
            .await
            .unwrap();
        assert_eq!(outcome, MembershipOutcome::NotFound);
        sched.shutdown();
    }

    // ── clear ────────────────────────────────────────────────────────────────

    /// An action whose only step is a `core.logic.wait` of `ms` milliseconds.
    /// Used to hold a blocking queue's single execution slot so that later
    /// dispatches observably pile up as pending (not-yet-started) work.
    fn wait_action(id: ActionId, queue_id: QueueId, ms: i64) -> Action {
        let mut config = std::collections::BTreeMap::new();
        config.insert("ms".to_owned(), Variant::Int(ms));
        Action {
            id,
            name: "wait".to_string(),
            group: None,
            queue_id,
            enabled: true,
            concurrent: false,
            bypass_pause: false,
            execution_mode: forge_types::ExecutionMode::Sequential,
            description: None,
            sub_actions: vec![SubActionStep {
                kind_id: "core.logic.wait".to_owned(),
                config,
                enabled: true,
                continue_on_error: false,
                condition: None,
                label: None,
            }],
        }
    }

    fn waiting_registry() -> Arc<SubActionRegistry> {
        let mut reg = SubActionRegistry::new();
        reg.register(Box::new(CoreLogicWaitRunner)).unwrap();
        Arc::new(reg)
    }

    fn req(queue_id: QueueId, action_id: ActionId) -> SchedulerRequest {
        SchedulerRequest {
            queue_id,
            action_id,
            trigger_event_id: EventId::new(),
            initial_args: ArgStack::new(),
            bypass_pause: false,
        }
    }

    fn action_id_of(ev: &Event) -> Option<String> {
        ev.payload
            .get("action_id")
            .and_then(|v| v.as_str())
            .map(str::to_owned)
    }

    /// Bounded poll for the `action.start` of a specific action - i.e. proof
    /// that the action has acquired its slot and is in-flight.
    async fn await_action_start(
        sub: &mut EventSubscription,
        action_id: ActionId,
        attempts: usize,
    ) -> bool {
        let target = action_id.to_string();
        for _ in 0..attempts {
            match tokio::time::timeout(Duration::from_millis(200), sub.recv()).await {
                Ok(Ok(ev))
                    if ev.kind == "action.start"
                        && action_id_of(&ev).as_deref() == Some(target.as_str()) =>
                {
                    return true;
                }
                Ok(Ok(_)) => {}
                _ => {}
            }
        }
        false
    }

    /// Drain every `action.done` published within `window`, returning the set of
    /// completed action ids. A deadline-bounded drain (not a fixed sleep): it
    /// returns as soon as the bus goes quiet for the remaining budget, and the
    /// window is sized to exceed the in-flight action's own wait so a surviving
    /// execution WOULD be observed if the clear failed to discard/abort it.
    async fn drain_dones(
        sub: &mut EventSubscription,
        window: Duration,
    ) -> std::collections::HashSet<String> {
        let deadline = tokio::time::Instant::now() + window;
        let mut seen = std::collections::HashSet::new();
        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                break;
            }
            match tokio::time::timeout(remaining, sub.recv()).await {
                Ok(Ok(ev)) if ev.kind == "action.done" => {
                    if let Some(a) = action_id_of(&ev) {
                        seen.insert(a);
                    }
                }
                Ok(Ok(_)) => {}
                _ => break,
            }
        }
        seen
    }

    async fn await_queue_event(
        sub: &mut EventSubscription,
        kind: &str,
        queue_id: QueueId,
        attempts: usize,
    ) -> bool {
        let target = queue_id.to_string();
        for _ in 0..attempts {
            match tokio::time::timeout(Duration::from_millis(200), sub.recv()).await {
                Ok(Ok(ev))
                    if ev.kind == kind
                        && ev.payload.get("queue_id").and_then(|v| v.as_str())
                            == Some(target.as_str()) =>
                {
                    return true;
                }
                Ok(Ok(_)) => {}
                _ => {}
            }
        }
        false
    }

    #[tokio::test]
    async fn clear_discards_pending_and_rebuilt_queue_runs_new_work() {
        // A holds the blocking queue's single slot (150 ms wait); B and C queue
        // up behind it as pending work. A `clear(keep_current = true)` must drop
        // B and C while letting A finish, and the rebuilt slot must still run a
        // fresh dispatch D. Window (800 ms) exceeds A's wait, so had B/C survived
        // they would have run after A and shown up in the done set.
        let dp = make_dp().await;
        let q_id = QueueId::new();
        let (a, b, c, d) = (
            ActionId::new(),
            ActionId::new(),
            ActionId::new(),
            ActionId::new(),
        );
        let queue = blocking_q(q_id);
        dp.queue_repo().save(&queue).await.unwrap();
        dp.action_repo()
            .save(&wait_action(a, q_id, 150))
            .await
            .unwrap();
        for id in [b, c, d] {
            dp.action_repo()
                .save(&wait_action(id, q_id, 0))
                .await
                .unwrap();
        }

        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let engine = spawn_action_engine(
            Arc::clone(&bus),
            dp.action_repo(),
            dp.history_repo(),
            waiting_registry(),
            Arc::new(crate::action_cancel::ActionCancelRegistry::new()),
        );
        let sched = QueueScheduler::spawn(engine, Arc::clone(&bus), vec![queue]);
        let mut sub = bus.subscribe();

        sched.dispatch(req(q_id, a)).await.unwrap();
        assert!(
            await_action_start(&mut sub, a, 30).await,
            "A must occupy the blocking slot before clearing"
        );
        sched.dispatch(req(q_id, b)).await.unwrap();
        sched.dispatch(req(q_id, c)).await.unwrap();

        sched.clear(q_id, true).await.unwrap();
        sched.dispatch(req(q_id, d)).await.unwrap();

        let dones = drain_dones(&mut sub, Duration::from_millis(800)).await;
        assert!(
            dones.contains(&a.to_string()),
            "keep_current=true must let the in-flight A finish"
        );
        assert!(
            dones.contains(&d.to_string()),
            "rebuilt queue must run work dispatched after clear"
        );
        assert!(
            !dones.contains(&b.to_string()),
            "pending B must be discarded by clear"
        );
        assert!(
            !dones.contains(&c.to_string()),
            "pending C must be discarded by clear"
        );
        sched.shutdown();
    }

    #[tokio::test]
    async fn clear_with_keep_current_false_aborts_in_flight_action() {
        // REGRESSION (currently FAILING - reproduces a bug, do not weaken):
        // `clear(keep_current = false)` calls `InflightTracker::abort_all`, but the
        // tracker only owns the dispatch-SEND future (`ActionEngineHandle::dispatch`
        // returns once the request is enqueued on the engine's mpsc). The real
        // execution runs detached in `ActionEngine::run`, so aborting the tracked
        // task is a no-op: an action that has already started runs to completion.
        //
        // A is mid-flight (150 ms wait) when clear(keep_current = false) fires;
        // its execution must be aborted (no action.done), while the rebuilt slot
        // still runs a fresh dispatch D. The 800 ms window exceeds A's wait, so a
        // non-aborted A would have completed and been observed.
        let dp = make_dp().await;
        let q_id = QueueId::new();
        let (a, d) = (ActionId::new(), ActionId::new());
        let queue = blocking_q(q_id);
        dp.queue_repo().save(&queue).await.unwrap();
        dp.action_repo()
            .save(&wait_action(a, q_id, 150))
            .await
            .unwrap();
        dp.action_repo()
            .save(&wait_action(d, q_id, 0))
            .await
            .unwrap();

        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let engine = spawn_action_engine(
            Arc::clone(&bus),
            dp.action_repo(),
            dp.history_repo(),
            waiting_registry(),
            Arc::new(crate::action_cancel::ActionCancelRegistry::new()),
        );
        let sched = QueueScheduler::spawn(engine, Arc::clone(&bus), vec![queue]);
        let mut sub = bus.subscribe();

        sched.dispatch(req(q_id, a)).await.unwrap();
        assert!(
            await_action_start(&mut sub, a, 30).await,
            "A must be in-flight before clearing"
        );

        sched.clear(q_id, false).await.unwrap();
        sched.dispatch(req(q_id, d)).await.unwrap();

        let dones = drain_dones(&mut sub, Duration::from_millis(800)).await;
        assert!(
            dones.contains(&d.to_string()),
            "queue must keep working after a keep_current=false clear"
        );
        assert!(
            !dones.contains(&a.to_string()),
            "keep_current=false must abort the in-flight action"
        );
        sched.shutdown();
    }

    #[tokio::test]
    async fn clear_preserves_paused_state_across_slot_rebuild() {
        // Load-bearing carry-forward: clearing rebuilds the queue's slot, and the
        // paused flag must survive that rebuild (mirrors the blocking-flip
        // reconfigure pause-preservation). Asserted directly via paused_queues so
        // the test pins the state, not a downstream side effect.
        let dp = make_dp().await;
        let q_id = QueueId::new();
        let queue = nonblocking(q_id);
        dp.queue_repo().save(&queue).await.unwrap();

        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let engine = spawn_action_engine(
            Arc::clone(&bus),
            dp.action_repo(),
            dp.history_repo(),
            Arc::new(SubActionRegistry::new()),
            Arc::new(crate::action_cancel::ActionCancelRegistry::new()),
        );
        let sched = QueueScheduler::spawn(engine, Arc::clone(&bus), vec![queue]);

        sched.pause(q_id).await.unwrap();
        sched.clear(q_id, true).await.unwrap();

        let paused = sched.paused_queues().await.unwrap();
        assert!(
            paused.contains(&q_id),
            "clear must preserve the paused state across the slot rebuild"
        );
        sched.shutdown();
    }

    #[tokio::test]
    async fn clear_unknown_queue_returns_queue_not_found() {
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let dp = make_dp().await;
        let engine = spawn_action_engine(
            Arc::clone(&bus),
            dp.action_repo(),
            dp.history_repo(),
            Arc::new(SubActionRegistry::new()),
            Arc::new(crate::action_cancel::ActionCancelRegistry::new()),
        );
        let sched = QueueScheduler::spawn(engine, Arc::clone(&bus), vec![]);

        let err = sched.clear(QueueId::new(), true).await.unwrap_err();
        assert!(matches!(err, SchedulerError::QueueNotFound(_)));
        sched.shutdown();
    }

    #[tokio::test]
    async fn clear_emits_queue_cleared_event_for_the_queue() {
        let dp = make_dp().await;
        let q_id = QueueId::new();
        let queue = nonblocking(q_id);
        dp.queue_repo().save(&queue).await.unwrap();

        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let engine = spawn_action_engine(
            Arc::clone(&bus),
            dp.action_repo(),
            dp.history_repo(),
            Arc::new(SubActionRegistry::new()),
            Arc::new(crate::action_cancel::ActionCancelRegistry::new()),
        );
        let sched = QueueScheduler::spawn(engine, Arc::clone(&bus), vec![queue]);
        let mut sub = bus.subscribe();

        sched.clear(q_id, true).await.unwrap();

        assert!(
            await_queue_event(&mut sub, "queue.cleared", q_id, 10).await,
            "clear must emit queue.cleared carrying the cleared queue_id"
        );
        sched.shutdown();
    }
}
