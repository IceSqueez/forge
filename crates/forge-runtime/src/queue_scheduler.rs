//! Dropping a slot's `sender` closes its channel; the runner drains and exits - a drain guarantee, not a leak.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use arc_swap::ArcSwapOption;
use forge_events::{Event, EventSource};
use forge_registry::CancelSignal;
use forge_types::{ActionId, ArgStack, EventId, Queue, QueueId};
use serde_json::json;
use tokio::sync::{Semaphore, mpsc, oneshot, watch};
use tokio::task::JoinHandle;
use tracing::warn;

use crate::{ActionEngineHandle, EventBus, ExecutionRequest};

/// Defensive ceiling on tasks buffered per queue; a frozen queue accumulates until it is hit.
pub const MAX_PENDING_PER_QUEUE: usize = 500;

/// Filled once `QueueScheduler::spawn` returns, avoiding a boot registration-order dependency.
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
    pub trigger_kind: Option<String>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueueProcessing {
    Running,
    Frozen,
}

impl QueueProcessing {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Frozen => "frozen",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueueIntake {
    Accept,
    Skip,
}

impl QueueIntake {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Accept => "accept",
            Self::Skip => "skip",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueueMode {
    pub processing: QueueProcessing,
    pub intake: QueueIntake,
}

impl QueueMode {
    pub const RUNNING: Self = Self {
        processing: QueueProcessing::Running,
        intake: QueueIntake::Accept,
    };
    pub const DRAINING: Self = Self {
        processing: QueueProcessing::Running,
        intake: QueueIntake::Skip,
    };
    pub const HOLDING: Self = Self {
        processing: QueueProcessing::Frozen,
        intake: QueueIntake::Accept,
    };
    pub const PAUSED: Self = Self {
        processing: QueueProcessing::Frozen,
        intake: QueueIntake::Skip,
    };

    pub fn event_kind(self) -> &'static str {
        match (self.processing, self.intake) {
            (QueueProcessing::Running, QueueIntake::Accept) => "queue.resumed",
            (QueueProcessing::Running, QueueIntake::Skip) => "queue.draining",
            (QueueProcessing::Frozen, QueueIntake::Accept) => "queue.held",
            (QueueProcessing::Frozen, QueueIntake::Skip) => "queue.paused",
        }
    }

    fn skip_reason(self) -> &'static str {
        match self.processing {
            QueueProcessing::Running => "queue_draining",
            QueueProcessing::Frozen => "queue_paused",
        }
    }
}

impl Default for QueueMode {
    fn default() -> Self {
        Self::RUNNING
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueueRuntimeState {
    pub mode: QueueMode,
    pub pending: usize,
    pub in_flight: usize,
    /// Reset to zero whenever the queue enters a different mode.
    pub overflowed: u64,
}

#[derive(Clone)]
pub struct QueueSchedulerHandle {
    sender: mpsc::UnboundedSender<SchedulerCommand>,
}

enum SchedulerCommand {
    Enqueue(SchedulerRequest),
    SetMode(
        QueueId,
        QueueMode,
        oneshot::Sender<Result<(), SchedulerError>>,
    ),
    Clear(QueueId, bool, oneshot::Sender<Result<(), SchedulerError>>),
    Register(Queue, oneshot::Sender<MembershipOutcome>),
    Deregister(QueueId, oneshot::Sender<MembershipOutcome>),
    Reconfigure(Queue, oneshot::Sender<MembershipOutcome>),
    QueryStates(oneshot::Sender<HashMap<QueueId, QueueRuntimeState>>),
    Shutdown,
}

struct QueueSlot {
    sender: mpsc::UnboundedSender<QueueTask>,
    processing: watch::Sender<QueueProcessing>,
    mode: QueueMode,
    counters: QueueCounters,
    name: String,
    concurrency: u32,
    runner: JoinHandle<()>,
    inflight: InflightTracker,
}

#[derive(Clone, Default)]
struct QueueCounters {
    pending: Arc<AtomicUsize>,
    overflowed: Arc<AtomicU64>,
}

impl QueueCounters {
    fn try_reserve(&self) -> bool {
        self.pending
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                (current < MAX_PENDING_PER_QUEUE).then_some(current + 1)
            })
            .is_ok()
    }

    fn release(&self) {
        let _ = self
            .pending
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                Some(current.saturating_sub(1))
            });
    }

    fn record_overflow(&self) {
        self.overflowed.fetch_add(1, Ordering::Relaxed);
    }

    fn reset_overflow(&self) {
        self.overflowed.store(0, Ordering::Relaxed);
    }

    fn overflowed(&self) -> u64 {
        self.overflowed.load(Ordering::Relaxed)
    }

    fn adopt_overflow(&self, value: u64) {
        self.overflowed.store(value, Ordering::Relaxed);
    }

    fn pending(&self) -> usize {
        self.pending.load(Ordering::Relaxed)
    }
}

/// Tracks in-flight cancel signals so `Clear(keep_current = false)` never cancels a finished execution.
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

    fn len(&self) -> usize {
        self.lock().signals.len()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, InflightInner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }
}

struct QueueTask {
    action_id: ActionId,
    trigger_event_id: EventId,
    trigger_kind: Option<String>,
    initial_args: ArgStack,
    bypass_pause: bool,
}

impl QueueSchedulerHandle {
    pub async fn dispatch(&self, req: SchedulerRequest) -> Result<(), SchedulerError> {
        self.sender
            .send(SchedulerCommand::Enqueue(req))
            .map_err(|_| SchedulerError::ChannelClosed)
    }

    pub async fn set_mode(&self, queue_id: QueueId, mode: QueueMode) -> Result<(), SchedulerError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(SchedulerCommand::SetMode(queue_id, mode, tx))
            .map_err(|_| SchedulerError::ChannelClosed)?;
        rx.await.map_err(|_| SchedulerError::ChannelClosed)?
    }

    /// Discards pending executions; with `keep_current = false` also cancels the in-flight one.
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

    pub async fn queue_states(
        &self,
    ) -> Result<HashMap<QueueId, QueueRuntimeState>, SchedulerError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(SchedulerCommand::QueryStates(tx))
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
            let slot = Self::make_queue_slot(queue, Arc::clone(&engine), QueueMode::default());
            slots.insert(id, slot);
        }

        tokio::spawn(Self::run_scheduler(cmd_rx, slots, bus, engine));

        QueueSchedulerHandle { sender: cmd_tx }
    }

    fn make_queue_slot(
        queue: Queue,
        engine: Arc<ActionEngineHandle>,
        mode: QueueMode,
    ) -> QueueSlot {
        let (task_tx, task_rx) = mpsc::unbounded_channel::<QueueTask>();
        let (processing_tx, processing_rx) = watch::channel(mode.processing);
        let inflight = InflightTracker::default();
        let counters = QueueCounters::default();
        let name = queue.name.clone();
        let concurrency = queue.concurrency.max(1);

        let sem = Arc::new(Semaphore::new(concurrency as usize));
        let runner = tokio::spawn(Self::run_bounded(
            task_rx,
            processing_rx,
            engine,
            sem,
            inflight.clone(),
            counters.clone(),
        ));

        QueueSlot {
            sender: task_tx,
            processing: processing_tx,
            mode,
            counters,
            name,
            concurrency,
            runner,
            inflight,
        }
    }

    async fn run_bounded(
        mut rx: mpsc::UnboundedReceiver<QueueTask>,
        mut processing: watch::Receiver<QueueProcessing>,
        engine: Arc<ActionEngineHandle>,
        sem: Arc<Semaphore>,
        inflight: InflightTracker,
        counters: QueueCounters,
    ) {
        let mut frozen_out: VecDeque<QueueTask> = VecDeque::new();

        loop {
            let permit = match Arc::clone(&sem).acquire_owned().await {
                Ok(p) => p,
                Err(_) => return,
            };

            let task = loop {
                let frozen = *processing.borrow_and_update() == QueueProcessing::Frozen;
                if !frozen && let Some(task) = frozen_out.pop_front() {
                    break task;
                }

                tokio::select! {
                    biased;
                    changed = processing.changed() => {
                        if changed.is_err() {
                            return;
                        }
                    }
                    received = rx.recv() => match received {
                        Some(task) if frozen && !task.bypass_pause => frozen_out.push_back(task),
                        Some(task) => break task,
                        None => return,
                    },
                }
            };

            counters.release();

            let req = ExecutionRequest {
                action_id: task.action_id,
                trigger_event_id: task.trigger_event_id,
                trigger_kind: task.trigger_kind,
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
                    Self::enqueue(req, &slots, &bus);
                }
                SchedulerCommand::SetMode(queue_id, mode, reply) => {
                    let r = Self::set_mode(&queue_id, mode, &mut slots, &bus);
                    let _ = reply.send(r);
                }
                SchedulerCommand::Clear(queue_id, keep_current, reply) => {
                    let r = Self::clear_queue(&mut slots, &queue_id, keep_current, &bus, &engine);
                    let _ = reply.send(r);
                }
                SchedulerCommand::Register(queue, reply) => {
                    let outcome = if slots.contains_key(&queue.id) {
                        MembershipOutcome::AlreadyRegistered
                    } else {
                        let id = queue.id;
                        let slot =
                            Self::make_queue_slot(queue, Arc::clone(&engine), QueueMode::default());
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
                            // Rebuild must carry the mode forward, else the queue silently resumes.
                            let mode = old.mode;
                            let overflowed = old.counters.overflowed();
                            let id = queue.id;
                            let slot = Self::make_queue_slot(queue, Arc::clone(&engine), mode);
                            slot.counters.adopt_overflow(overflowed);
                            slots.insert(id, slot);
                            MembershipOutcome::Applied
                        }
                        None => MembershipOutcome::NotFound,
                    };
                    let _ = reply.send(outcome);
                }
                SchedulerCommand::QueryStates(reply) => {
                    let _ = reply.send(Self::queue_states(&slots));
                }
                SchedulerCommand::Shutdown => break,
            }
        }
    }

    fn enqueue(req: SchedulerRequest, slots: &HashMap<QueueId, QueueSlot>, bus: &Arc<EventBus>) {
        let slot = match slots.get(&req.queue_id) {
            Some(s) => s,
            None => {
                warn!("enqueue: queue {} not found, dropping", req.queue_id);
                Self::publish_skip(bus, &req, "queue_not_found");
                return;
            }
        };

        if !req.bypass_pause && slot.mode.intake == QueueIntake::Skip {
            Self::publish_skip(bus, &req, slot.mode.skip_reason());
            return;
        }

        if !slot.counters.try_reserve() {
            slot.counters.record_overflow();
            Self::publish_skip(bus, &req, "queue_pending_overflow");
            return;
        }

        let task = QueueTask {
            action_id: req.action_id,
            trigger_event_id: req.trigger_event_id,
            trigger_kind: req.trigger_kind,
            initial_args: req.initial_args,
            bypass_pause: req.bypass_pause,
        };

        if slot.sender.send(task).is_err() {
            slot.counters.release();
            warn!("queue task channel closed for queue {}", req.queue_id);
        }
    }

    fn publish_skip(bus: &EventBus, req: &SchedulerRequest, reason: &str) {
        bus.publish(Event::caused_by(
            EventSource::Core,
            "action.skipped",
            json!({
                "action_id": req.action_id.to_string(),
                "reason": reason,
                "queue_id": req.queue_id.to_string(),
            }),
            req.trigger_event_id,
        ));
    }

    fn queue_states(slots: &HashMap<QueueId, QueueSlot>) -> HashMap<QueueId, QueueRuntimeState> {
        slots
            .iter()
            .map(|(id, slot)| {
                (
                    *id,
                    QueueRuntimeState {
                        mode: slot.mode,
                        pending: slot.counters.pending(),
                        in_flight: slot.inflight.len(),
                        overflowed: slot.counters.overflowed(),
                    },
                )
            })
            .collect()
    }

    fn set_mode(
        queue_id: &QueueId,
        mode: QueueMode,
        slots: &mut HashMap<QueueId, QueueSlot>,
        bus: &Arc<EventBus>,
    ) -> Result<(), SchedulerError> {
        let slot = slots
            .get_mut(queue_id)
            .ok_or(SchedulerError::QueueNotFound(*queue_id))?;

        if slot.mode == mode {
            return Ok(());
        }

        slot.mode = mode;
        slot.counters.reset_overflow();
        slot.processing.send_replace(mode.processing);

        bus.publish(Event::new(
            EventSource::Core,
            mode.event_kind(),
            json!({
                "queue_id": queue_id.to_string(),
                "queue_name": slot.name,
                "processing": mode.processing.as_str(),
                "intake": mode.intake.as_str(),
            }),
        ));

        Ok(())
    }

    fn clear_queue(
        slots: &mut HashMap<QueueId, QueueSlot>,
        queue_id: &QueueId,
        keep_current: bool,
        bus: &Arc<EventBus>,
        engine: &Arc<ActionEngineHandle>,
    ) -> Result<(), SchedulerError> {
        let (mode, overflowed, name, concurrency) = {
            let slot = slots
                .get(queue_id)
                .ok_or(SchedulerError::QueueNotFound(*queue_id))?;

            if !keep_current {
                slot.inflight.cancel_all();
            }
            // Abort discards buffered executions; the signal above unwinds the in-flight one.
            slot.runner.abort();

            (
                slot.mode,
                slot.counters.overflowed(),
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
            },
            Arc::clone(engine),
            mode,
        );
        rebuilt.counters.adopt_overflow(overflowed);
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
#[allow(clippy::unwrap_used, clippy::expect_used)]
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
        }
    }

    fn blocking_q(id: QueueId) -> Queue {
        Queue {
            id,
            name: "serial".to_string(),
            description: String::new(),
            concurrency: 1,
        }
    }

    async fn collect_events(
        sub: &mut EventSubscription,
        target_kind: &str,
        max_attempts: usize,
        timeout_ms: u64,
    ) -> bool {
        collect_event(sub, target_kind, max_attempts, timeout_ms)
            .await
            .is_some()
    }

    fn spawn_sched(
        dp: &Arc<dyn DataProvider>,
        bus: &Arc<EventBus>,
        registry: Arc<SubActionRegistry>,
        queues: Vec<Queue>,
    ) -> QueueSchedulerHandle {
        let engine = spawn_action_engine(
            Arc::clone(bus),
            dp.action_repo(),
            dp.history_repo(),
            registry,
            Arc::new(crate::action_cancel::ActionCancelRegistry::new()),
        );
        QueueScheduler::spawn(engine, Arc::clone(bus), queues)
    }

    async fn record(sub: &mut EventSubscription, window: Duration) -> Vec<Event> {
        let deadline = tokio::time::Instant::now() + window;
        let mut seen = Vec::new();
        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                break;
            }
            match tokio::time::timeout(remaining, sub.recv()).await {
                Ok(Ok(ev)) => seen.push(ev),
                _ => break,
            }
        }
        seen
    }

    fn ids_of(events: &[Event], kind: &str) -> Vec<String> {
        events
            .iter()
            .filter(|ev| ev.kind == kind)
            .filter_map(action_id_of)
            .collect()
    }

    fn skip_reasons(events: &[Event]) -> Vec<&str> {
        events
            .iter()
            .filter(|ev| ev.kind == "action.skipped")
            .filter_map(|ev| ev.payload.get("reason").and_then(|v| v.as_str()))
            .collect()
    }

    async fn collect_event(
        sub: &mut EventSubscription,
        target_kind: &str,
        max_attempts: usize,
        timeout_ms: u64,
    ) -> Option<Event> {
        for _ in 0..max_attempts {
            match tokio::time::timeout(Duration::from_millis(timeout_ms), sub.recv()).await {
                Ok(Ok(ev)) if ev.kind == target_kind => return Some(ev),
                Ok(Ok(_)) => {}
                _ => break,
            }
        }
        None
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
                trigger_kind: None,
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
                    trigger_kind: None,
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

        sched.set_mode(q_id, QueueMode::PAUSED).await.unwrap();

        let trigger = EventId::new();
        sched
            .dispatch(SchedulerRequest {
                queue_id: q_id,
                action_id: a_id,
                trigger_event_id: trigger,
                trigger_kind: None,
                initial_args: ArgStack::new(),
                bypass_pause: false,
            })
            .await
            .unwrap();

        let skipped = collect_event(&mut sub, "action.skipped", 20, 60)
            .await
            .expect("paused queue must emit action.skipped");
        assert_eq!(skipped.payload["reason"].as_str(), Some("queue_paused"));
        assert_eq!(
            skipped.caused_by,
            Some(trigger),
            "action.skipped must link caused_by to the triggering event"
        );

        assert!(
            !collect_events(&mut sub, "action.done", 6, 30).await,
            "paused queue must not execute action"
        );
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

        sched.set_mode(q_id, QueueMode::PAUSED).await.unwrap();

        sched
            .dispatch(SchedulerRequest {
                queue_id: q_id,
                action_id: a_id,
                trigger_event_id: EventId::new(),
                trigger_kind: None,
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

        sched.set_mode(q_id, QueueMode::PAUSED).await.unwrap();
        sched.set_mode(q_id, QueueMode::RUNNING).await.unwrap();

        sched
            .dispatch(SchedulerRequest {
                queue_id: q_id,
                action_id: a_id,
                trigger_event_id: EventId::new(),
                trigger_kind: None,
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
    async fn each_mode_change_publishes_its_kind_carrying_both_axes() {
        let dp = make_dp().await;
        let q_id = QueueId::new();
        let queue = nonblocking(q_id);
        dp.queue_repo().save(&queue).await.unwrap();

        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let sched = spawn_sched(&dp, &bus, Arc::new(SubActionRegistry::new()), vec![queue]);
        let mut sub = bus.subscribe();

        for (mode, kind, processing, intake) in [
            (QueueMode::DRAINING, "queue.draining", "running", "skip"),
            (QueueMode::HOLDING, "queue.held", "frozen", "accept"),
            (QueueMode::PAUSED, "queue.paused", "frozen", "skip"),
            (QueueMode::RUNNING, "queue.resumed", "running", "accept"),
        ] {
            sched.set_mode(q_id, mode).await.unwrap();
            let published = collect_event(&mut sub, kind, 10, 200).await;
            assert!(published.is_some(), "mode change must publish {kind}");
            let ev = published.unwrap();
            assert_eq!(
                ev.payload["processing"].as_str(),
                Some(processing),
                "{kind} must carry its processing axis"
            );
            assert_eq!(
                ev.payload["intake"].as_str(),
                Some(intake),
                "{kind} must carry its intake axis"
            );
        }
        sched.shutdown();
    }

    #[tokio::test]
    async fn re_applying_the_current_mode_publishes_nothing() {
        let dp = make_dp().await;
        let q_id = QueueId::new();
        let queue = nonblocking(q_id);
        dp.queue_repo().save(&queue).await.unwrap();

        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let sched = spawn_sched(&dp, &bus, Arc::new(SubActionRegistry::new()), vec![queue]);

        sched.set_mode(q_id, QueueMode::PAUSED).await.unwrap();
        let mut sub = bus.subscribe();
        sched.set_mode(q_id, QueueMode::PAUSED).await.unwrap();

        let seen = record(&mut sub, Duration::from_millis(200)).await;
        let kinds: Vec<&str> = seen.iter().map(|ev| ev.kind.as_str()).collect();
        assert!(
            kinds.iter().all(|kind| !kind.starts_with("queue.")),
            "an unchanged mode must publish no queue event, saw {kinds:?}"
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

        let trigger = EventId::new();
        sched
            .dispatch(SchedulerRequest {
                queue_id: unknown_q,
                action_id: a_id,
                trigger_event_id: trigger,
                trigger_kind: None,
                initial_args: ArgStack::new(),
                bypass_pause: false,
            })
            .await
            .unwrap();

        let skipped = collect_event(&mut sub, "action.skipped", 10, 200)
            .await
            .expect("unknown queue must emit action.skipped");
        assert_eq!(skipped.payload["reason"].as_str(), Some("queue_not_found"));
        assert_eq!(
            skipped.caused_by,
            Some(trigger),
            "action.skipped must link caused_by to the triggering event"
        );
        sched.shutdown();
    }

    #[tokio::test]
    async fn register_new_queue_returns_applied_and_enables_dispatch() {
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
        let sched = QueueScheduler::spawn(engine, Arc::clone(&bus), vec![]);
        let mut sub = bus.subscribe();

        let outcome = sched.register(queue).await.unwrap();
        assert_eq!(outcome, MembershipOutcome::Applied);

        sched
            .dispatch(SchedulerRequest {
                queue_id: q_id,
                action_id: a_id,
                trigger_event_id: EventId::new(),
                trigger_kind: None,
                initial_args: ArgStack::new(),
                bypass_pause: false,
            })
            .await
            .unwrap();

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

        sched.set_mode(q_id, QueueMode::PAUSED).await.unwrap();
        let outcome = sched.register(nonblocking(q_id)).await.unwrap();
        assert_eq!(outcome, MembershipOutcome::AlreadyRegistered);

        sched
            .dispatch(SchedulerRequest {
                queue_id: q_id,
                action_id: a_id,
                trigger_event_id: EventId::new(),
                trigger_kind: None,
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
        assert!(saw_skipped, "re-register must not un-pause existing slot");
        assert!(
            !saw_done,
            "re-register must not replace existing paused slot"
        );
        sched.shutdown();
    }

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
                trigger_kind: None,
                initial_args: ArgStack::new(),
                bypass_pause: false,
            })
            .await
            .unwrap();

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

    #[tokio::test]
    async fn reconfigure_rename_preserves_pause_state_and_runner() {
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

        sched.set_mode(q_id, QueueMode::PAUSED).await.unwrap();

        let renamed = Queue {
            id: q_id,
            name: "renamed".to_string(),
            description: String::new(),
            concurrency: 8,
        };
        let outcome = sched.reconfigure(renamed).await.unwrap();
        assert_eq!(outcome, MembershipOutcome::Applied);

        sched
            .dispatch(SchedulerRequest {
                queue_id: q_id,
                action_id: a_id,
                trigger_event_id: EventId::new(),
                trigger_kind: None,
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

        let flipped = blocking_q(q_id);
        let outcome = sched.reconfigure(flipped).await.unwrap();
        assert_eq!(outcome, MembershipOutcome::Applied);

        sched
            .dispatch(SchedulerRequest {
                queue_id: q_id,
                action_id: a_id,
                trigger_event_id: EventId::new(),
                trigger_kind: None,
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

        sched.set_mode(q_id, QueueMode::PAUSED).await.unwrap();
        let outcome = sched.reconfigure(blocking_q(q_id)).await.unwrap();
        assert_eq!(outcome, MembershipOutcome::Applied);

        sched
            .dispatch(SchedulerRequest {
                queue_id: q_id,
                action_id: a_id,
                trigger_event_id: EventId::new(),
                trigger_kind: None,
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
            })
            .await
            .unwrap();
        assert_eq!(outcome, MembershipOutcome::NotFound);
        sched.shutdown();
    }

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
            trigger_kind: None,
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

        sched.set_mode(q_id, QueueMode::PAUSED).await.unwrap();
        sched.clear(q_id, true).await.unwrap();

        let states = sched.queue_states().await.unwrap();
        assert_eq!(
            states.get(&q_id).map(|s| s.mode),
            Some(QueueMode::PAUSED),
            "clear must preserve the queue mode across the slot rebuild"
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

    async fn serial_queue_with_waits(
        ms: &[(ActionId, i64)],
        q_id: QueueId,
    ) -> (Arc<dyn DataProvider>, Arc<EventBus>, QueueSchedulerHandle) {
        let dp = make_dp().await;
        dp.queue_repo().save(&blocking_q(q_id)).await.unwrap();
        for (id, wait_ms) in ms {
            dp.action_repo()
                .save(&wait_action(*id, q_id, *wait_ms))
                .await
                .unwrap();
        }
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let sched = spawn_sched(&dp, &bus, waiting_registry(), vec![blocking_q(q_id)]);
        (dp, bus, sched)
    }

    #[tokio::test]
    async fn freezing_lets_the_running_task_finish_but_withholds_the_next_one() {
        let q_id = QueueId::new();
        let (a, b) = (ActionId::new(), ActionId::new());
        let (_dp, bus, sched) = serial_queue_with_waits(&[(a, 150), (b, 0)], q_id).await;
        let mut sub = bus.subscribe();

        sched.dispatch(req(q_id, a)).await.unwrap();
        assert!(
            await_action_start(&mut sub, a, 30).await,
            "A must occupy the single slot before the freeze"
        );
        sched.dispatch(req(q_id, b)).await.unwrap();
        sched.set_mode(q_id, QueueMode::HOLDING).await.unwrap();

        let seen = record(&mut sub, Duration::from_millis(500)).await;
        assert!(
            ids_of(&seen, "action.done").contains(&a.to_string()),
            "a task already in flight when the freeze lands must still finish"
        );
        assert!(
            !ids_of(&seen, "action.start").contains(&b.to_string()),
            "a frozen queue must stop between tasks, not start the buffered B"
        );
        sched.shutdown();
    }

    #[tokio::test]
    async fn resuming_runs_tasks_buffered_while_frozen_in_enqueue_order() {
        let q_id = QueueId::new();
        let (a, b, c) = (ActionId::new(), ActionId::new(), ActionId::new());
        let (_dp, bus, sched) = serial_queue_with_waits(&[(a, 0), (b, 0), (c, 0)], q_id).await;
        let mut sub = bus.subscribe();

        sched.set_mode(q_id, QueueMode::HOLDING).await.unwrap();
        for id in [a, b, c] {
            sched.dispatch(req(q_id, id)).await.unwrap();
        }

        let while_frozen = record(&mut sub, Duration::from_millis(150)).await;
        assert!(
            ids_of(&while_frozen, "action.start").is_empty(),
            "a holding queue must collect the dispatches without starting them"
        );

        sched.set_mode(q_id, QueueMode::RUNNING).await.unwrap();
        let after_resume = record(&mut sub, Duration::from_millis(600)).await;
        assert_eq!(
            ids_of(&after_resume, "action.start"),
            vec![a.to_string(), b.to_string(), c.to_string()],
            "the frozen buffer must replay in enqueue order"
        );
        sched.shutdown();
    }

    #[tokio::test]
    async fn draining_skips_new_work_while_buffered_tasks_keep_executing() {
        let q_id = QueueId::new();
        let (a, b, c) = (ActionId::new(), ActionId::new(), ActionId::new());
        let (_dp, bus, sched) = serial_queue_with_waits(&[(a, 150), (b, 0), (c, 0)], q_id).await;
        let mut sub = bus.subscribe();

        sched.dispatch(req(q_id, a)).await.unwrap();
        assert!(
            await_action_start(&mut sub, a, 30).await,
            "A must occupy the single slot before draining starts"
        );
        sched.dispatch(req(q_id, b)).await.unwrap();
        sched.set_mode(q_id, QueueMode::DRAINING).await.unwrap();
        sched.dispatch(req(q_id, c)).await.unwrap();

        let seen = record(&mut sub, Duration::from_millis(600)).await;
        assert!(
            ids_of(&seen, "action.done").contains(&b.to_string()),
            "draining must keep executing work buffered before the switch"
        );
        assert!(
            !ids_of(&seen, "action.start").contains(&c.to_string()),
            "draining must refuse work dispatched after the switch"
        );
        assert_eq!(
            skip_reasons(&seen),
            vec!["queue_draining"],
            "the refused dispatch must report the draining reason"
        );
        sched.shutdown();
    }

    #[tokio::test]
    async fn pausing_withholds_buffered_work_until_the_queue_resumes() {
        let q_id = QueueId::new();
        let (a, b) = (ActionId::new(), ActionId::new());
        let (_dp, bus, sched) = serial_queue_with_waits(&[(a, 150), (b, 0)], q_id).await;
        let mut sub = bus.subscribe();

        sched.dispatch(req(q_id, a)).await.unwrap();
        assert!(
            await_action_start(&mut sub, a, 30).await,
            "A must occupy the single slot before the pause"
        );
        sched.dispatch(req(q_id, b)).await.unwrap();
        sched.set_mode(q_id, QueueMode::PAUSED).await.unwrap();

        let while_paused = record(&mut sub, Duration::from_millis(500)).await;
        assert!(
            !ids_of(&while_paused, "action.start").contains(&b.to_string()),
            "pausing must freeze processing too, not only close intake"
        );

        sched.set_mode(q_id, QueueMode::RUNNING).await.unwrap();
        let after_resume = record(&mut sub, Duration::from_millis(400)).await;
        assert!(
            ids_of(&after_resume, "action.done").contains(&b.to_string()),
            "resuming must release the withheld task"
        );
        sched.shutdown();
    }

    #[tokio::test]
    async fn queue_states_reports_live_pending_and_in_flight_counts() {
        let q_id = QueueId::new();
        let (a, b, c) = (ActionId::new(), ActionId::new(), ActionId::new());
        let (_dp, bus, sched) = serial_queue_with_waits(&[(a, 300), (b, 0), (c, 0)], q_id).await;
        let mut sub = bus.subscribe();

        sched.dispatch(req(q_id, a)).await.unwrap();
        assert!(
            await_action_start(&mut sub, a, 30).await,
            "A must be executing before the counts are read"
        );
        sched.dispatch(req(q_id, b)).await.unwrap();
        sched.dispatch(req(q_id, c)).await.unwrap();

        let state = sched.queue_states().await.unwrap().remove(&q_id).unwrap();
        assert_eq!(state.in_flight, 1, "only A occupies the serial slot");
        assert_eq!(state.pending, 2, "B and C must be counted as pending");
        sched.shutdown();
    }

    async fn frozen_queue_filled_to_the_cap() -> (QueueId, Arc<EventBus>, QueueSchedulerHandle) {
        let dp = make_dp().await;
        let q_id = QueueId::new();
        let a_id = ActionId::new();
        dp.queue_repo().save(&nonblocking(q_id)).await.unwrap();
        dp.action_repo()
            .save(&log_action(a_id, q_id))
            .await
            .unwrap();

        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let sched = spawn_sched(
            &dp,
            &bus,
            Arc::new(SubActionRegistry::new()),
            vec![nonblocking(q_id)],
        );
        sched.set_mode(q_id, QueueMode::HOLDING).await.unwrap();
        for _ in 0..MAX_PENDING_PER_QUEUE {
            sched.dispatch(req(q_id, a_id)).await.unwrap();
        }
        (q_id, bus, sched)
    }

    #[tokio::test]
    async fn dispatch_past_the_pending_cap_is_skipped_and_counted_as_overflow() {
        let (q_id, bus, sched) = frozen_queue_filled_to_the_cap().await;
        let mut sub = bus.subscribe();

        sched.dispatch(req(q_id, ActionId::new())).await.unwrap();

        let state = sched.queue_states().await.unwrap().remove(&q_id).unwrap();
        assert_eq!(
            state.pending, MAX_PENDING_PER_QUEUE,
            "the queue must buffer exactly the cap"
        );
        assert_eq!(
            state.overflowed, 1,
            "the dispatch past the cap must bump the overflow counter"
        );

        let seen = record(&mut sub, Duration::from_millis(150)).await;
        assert_eq!(
            skip_reasons(&seen),
            vec!["queue_pending_overflow"],
            "the dispatch past the cap must report the overflow reason"
        );
        sched.shutdown();
    }

    #[tokio::test]
    async fn overflow_counter_survives_a_no_op_mode_set_and_resets_on_a_real_change() {
        let (q_id, _bus, sched) = frozen_queue_filled_to_the_cap().await;
        sched.dispatch(req(q_id, ActionId::new())).await.unwrap();

        sched.set_mode(q_id, QueueMode::HOLDING).await.unwrap();
        let unchanged = sched.queue_states().await.unwrap().remove(&q_id).unwrap();
        assert_eq!(
            unchanged.overflowed, 1,
            "re-applying the same mode must not clear the overflow counter"
        );

        sched.set_mode(q_id, QueueMode::PAUSED).await.unwrap();
        let changed = sched.queue_states().await.unwrap().remove(&q_id).unwrap();
        assert_eq!(
            changed.overflowed, 0,
            "a real mode change must reset the overflow counter"
        );
        sched.shutdown();
    }
}
