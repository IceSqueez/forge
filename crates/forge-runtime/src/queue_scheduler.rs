use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use forge_events::{ActionSkipped, Event, EventSource, TypedEvent};
use forge_registry::CancelSignal;
use forge_types::{Action, ActionId, ArgStack, EventId, Queue, QueueId};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::futures::Notified;
use tokio::sync::{Notify, mpsc, oneshot, watch};
use tracing::{info, warn};

use crate::queue_depth::{DepthBoard, DepthCell, QueueDepthWatch, RunningAction};
use crate::scheduled_runs::ScheduledOrigin;
use crate::{ActionEngineHandle, EventBus, ExecutionRequest};

pub const MAX_PENDING_PER_QUEUE: usize = 500;

pub const QUEUE_NOT_FOUND_REASON: &str = "queue_not_found";
pub const QUEUE_OVERFLOW_REASON: &str = "queue_pending_overflow";
const QUEUE_CLEARED_KIND: &str = "queue.cleared";
pub const QUEUE_PAUSED_REASON: &str = "queue_paused";
pub const QUEUE_DRAINING_REASON: &str = "queue_draining";
pub const QUEUE_DELETED_REASON: &str = "queue_deleted";
pub const ENGINE_STOPPED_REASON: &str = "engine_stopped";

pub struct SchedulerRequest {
    pub queue_id: QueueId,
    pub action_id: ActionId,
    pub trigger_event_id: EventId,
    pub trigger_kind: Option<String>,
    pub manual_test: bool,
    pub initial_args: ArgStack,
    pub admission: QueueAdmission,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct QueueAdmission {
    pub bypass_pause: bool,
    pub run_alongside: bool,
}

impl From<&Action> for QueueAdmission {
    fn from(action: &Action) -> Self {
        Self {
            bypass_pause: action.bypass_pause,
            run_alongside: action.concurrent,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Lane {
    Gated,
    Alongside,
}

enum NextTask {
    Ready(QueueTask),
    Overcommitted,
    Closed,
}

#[derive(Debug, thiserror::Error)]
pub enum SchedulerError {
    #[error("scheduler channel closed")]
    ChannelClosed,
    #[error("queue not found: {0}")]
    QueueNotFound(QueueId),
    #[error("queue refused the run: {0}")]
    Refused(&'static str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MembershipOutcome {
    Applied,
    AlreadyRegistered,
    NotFound,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum QueueProcessing {
    Running,
    Frozen,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum QueueIntake {
    Accept,
    Skip,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
    pub const ALL: [Self; 4] = [Self::RUNNING, Self::DRAINING, Self::HOLDING, Self::PAUSED];

    pub fn event_kind(self) -> &'static str {
        match (self.processing, self.intake) {
            (QueueProcessing::Running, QueueIntake::Accept) => "queue.resumed",
            (QueueProcessing::Running, QueueIntake::Skip) => "queue.draining",
            (QueueProcessing::Frozen, QueueIntake::Accept) => "queue.held",
            (QueueProcessing::Frozen, QueueIntake::Skip) => "queue.paused",
        }
    }

    fn intake_refusal(self, bypass_pause: bool) -> Option<&'static str> {
        if bypass_pause || self.intake == QueueIntake::Accept {
            return None;
        }
        Some(match self.processing {
            QueueProcessing::Running => QUEUE_DRAINING_REASON,
            QueueProcessing::Frozen => QUEUE_PAUSED_REASON,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueueModeChanged {
    pub queue_id: QueueId,
    #[serde(default)]
    pub queue_name: String,
    #[serde(flatten)]
    pub mode: QueueMode,
}

impl QueueModeChanged {
    pub fn into_event(self) -> Event {
        let kind = self.mode.event_kind();
        let payload = serde_json::to_value(&self).unwrap_or_else(|error| {
            tracing::error!(
                kind,
                category = ?error.classify(),
                "event payload could not be encoded"
            );
            serde_json::Value::Null
        });
        Event::new(EventSource::Core, kind, payload)
    }

    pub fn from_event(event: &Event) -> Option<Self> {
        if !QueueMode::ALL
            .iter()
            .any(|mode| mode.event_kind() == event.kind)
        {
            return None;
        }
        match serde_json::from_value(event.payload.clone()) {
            Ok(changed) => Some(changed),
            Err(error) => {
                warn!(
                    kind = %event.kind,
                    event = %event.id,
                    category = ?error.classify(),
                    "event payload does not match its kind"
                );
                None
            }
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
    pub overflowed: u64,
}

#[derive(Clone)]
pub struct QueueSchedulerHandle {
    sender: mpsc::UnboundedSender<SchedulerCommand>,
    depths: DepthBoard,
    intake_changes: watch::Receiver<()>,
}

enum SchedulerCommand {
    Enqueue(SchedulerRequest),
    Offer(
        SchedulerRequest,
        oneshot::Sender<Result<(), SchedulerError>>,
    ),
    Admit(
        SchedulerRequest,
        ScheduledOrigin,
        oneshot::Sender<Result<(), SchedulerError>>,
    ),
    IntakeRefusal(QueueId, bool, oneshot::Sender<Option<&'static str>>),
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

enum Arrival {
    Triggered,
    Scheduled(ScheduledOrigin),
}

struct QueueSlot {
    pending: PendingBuffer,
    processing: watch::Sender<QueueProcessing>,
    mode: QueueMode,
    overflowed: u64,
    name: String,
    gate: ConcurrencyGate,
    inflight: InflightTracker,
    depth: DepthCell,
}

#[derive(Clone)]
struct ConcurrencyGate {
    inner: Arc<GateInner>,
}

struct GateInner {
    state: Mutex<GateState>,
    freed: Notify,
}

struct GateState {
    limit: usize,
    outstanding: usize,
}

impl ConcurrencyGate {
    fn new(limit: usize) -> Self {
        Self {
            inner: Arc::new(GateInner {
                state: Mutex::new(GateState {
                    limit,
                    outstanding: 0,
                }),
                freed: Notify::new(),
            }),
        }
    }

    async fn acquire(&self) -> GatePermit {
        loop {
            let freed = self.inner.freed.notified();
            if self.take() {
                return GatePermit { gate: self.clone() };
            }
            freed.await;
        }
    }

    fn take(&self) -> bool {
        let mut state = self.lock();
        if state.outstanding >= state.limit {
            return false;
        }
        state.outstanding += 1;
        true
    }

    fn replace_limit(&self, limit: usize) -> usize {
        let previous = {
            let mut state = self.lock();
            std::mem::replace(&mut state.limit, limit)
        };
        if limit > previous {
            self.inner.freed.notify_one();
        }
        previous
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, GateState> {
        self.inner.state.lock().unwrap_or_else(|e| e.into_inner())
    }
}

struct GatePermit {
    gate: ConcurrencyGate,
}

impl GatePermit {
    fn within_limit(&self) -> bool {
        let state = self.gate.lock();
        state.outstanding <= state.limit
    }
}

impl Drop for GatePermit {
    fn drop(&mut self) {
        {
            let mut state = self.gate.lock();
            state.outstanding = state.outstanding.saturating_sub(1);
        }
        self.gate.inner.freed.notify_one();
    }
}

#[derive(Clone)]
struct PendingBuffer {
    inner: Arc<PendingInner>,
}

struct PendingInner {
    tasks: Mutex<VecDeque<QueueTask>>,
    gated_arrived: Notify,
    alongside_arrived: Notify,
    depth: DepthCell,
}

impl PendingBuffer {
    fn new(depth: DepthCell) -> Self {
        Self {
            inner: Arc::new(PendingInner {
                tasks: Mutex::new(VecDeque::new()),
                gated_arrived: Notify::new(),
                alongside_arrived: Notify::new(),
                depth,
            }),
        }
    }

    fn push(&self, task: QueueTask) -> bool {
        let lane = task.lane();
        {
            let mut tasks = self.lock();
            if tasks.len() >= MAX_PENDING_PER_QUEUE {
                return false;
            }
            tasks.push_back(task);
            self.publish(tasks.len());
        }
        self.arrival(lane).notify_one();
        true
    }

    fn take_next(&self, lane: Lane, frozen: bool) -> Option<QueueTask> {
        let mut tasks = self.lock();
        let at = tasks
            .iter()
            .position(|task| task.lane() == lane && (!frozen || task.admission.bypass_pause))?;
        let taken = tasks.remove(at);
        self.publish(tasks.len());
        taken
    }

    fn clear(&self) {
        self.take_all();
    }

    fn take_all(&self) -> VecDeque<QueueTask> {
        let mut tasks = self.lock();
        let taken = std::mem::take(&mut *tasks);
        self.publish(0);
        taken
    }

    fn publish(&self, pending: usize) {
        self.inner.depth.update(|depth| depth.pending = pending);
    }

    fn len(&self) -> usize {
        self.lock().len()
    }

    fn arrived(&self, lane: Lane) -> Notified<'_> {
        self.arrival(lane).notified()
    }

    fn arrival(&self, lane: Lane) -> &Notify {
        match lane {
            Lane::Gated => &self.inner.gated_arrived,
            Lane::Alongside => &self.inner.alongside_arrived,
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, VecDeque<QueueTask>> {
        self.inner.tasks.lock().unwrap_or_else(|e| e.into_inner())
    }
}

#[derive(Clone)]
struct InflightTracker {
    inner: Arc<Mutex<InflightInner>>,
    depth: DepthCell,
}

#[derive(Default)]
struct InflightInner {
    next_id: u64,
    runs: BTreeMap<u64, InflightRun>,
}

struct InflightRun {
    signal: CancelSignal,
    action: RunningAction,
}

impl InflightTracker {
    fn new(depth: DepthCell) -> Self {
        Self {
            inner: Arc::default(),
            depth,
        }
    }

    fn register(&self, signal: CancelSignal, action_id: ActionId) -> u64 {
        let mut inner = self.lock();
        let id = inner.next_id;
        inner.next_id = inner.next_id.wrapping_add(1);
        let action = RunningAction {
            action_id,
            started_at: Instant::now(),
        };
        inner.runs.insert(id, InflightRun { signal, action });
        self.publish(&inner);
        id
    }

    fn complete(&self, id: u64) {
        let mut inner = self.lock();
        inner.runs.remove(&id);
        self.publish(&inner);
    }

    fn publish(&self, inner: &InflightInner) {
        let running = inner.runs.values().map(|run| run.action).collect();
        self.depth.publish_running(running);
    }

    fn cancel_all(&self) {
        for run in std::mem::take(&mut self.lock().runs).into_values() {
            run.signal.cancel();
        }
    }

    fn len(&self) -> usize {
        self.lock().runs.len()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, InflightInner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }
}

struct QueueTask {
    action_id: ActionId,
    trigger_event_id: EventId,
    trigger_kind: Option<String>,
    manual_test: bool,
    origin: Option<ScheduledOrigin>,
    initial_args: ArgStack,
    admission: QueueAdmission,
}

impl QueueTask {
    fn lane(&self) -> Lane {
        if self.admission.run_alongside {
            Lane::Alongside
        } else {
            Lane::Gated
        }
    }
}

impl QueueSchedulerHandle {
    pub async fn dispatch(&self, req: SchedulerRequest) -> Result<(), SchedulerError> {
        self.sender
            .send(SchedulerCommand::Enqueue(req))
            .map_err(|_| SchedulerError::ChannelClosed)
    }

    pub(crate) async fn offer(&self, req: SchedulerRequest) -> Result<(), SchedulerError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(SchedulerCommand::Offer(req, tx))
            .map_err(|_| SchedulerError::ChannelClosed)?;
        rx.await.map_err(|_| SchedulerError::ChannelClosed)?
    }

    pub(crate) async fn admit(
        &self,
        req: SchedulerRequest,
        origin: ScheduledOrigin,
    ) -> Result<(), SchedulerError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(SchedulerCommand::Admit(req, origin, tx))
            .map_err(|_| SchedulerError::ChannelClosed)?;
        rx.await.map_err(|_| SchedulerError::ChannelClosed)?
    }

    pub(crate) async fn intake_refusal(
        &self,
        queue_id: QueueId,
        bypass_pause: bool,
    ) -> Result<Option<&'static str>, SchedulerError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(SchedulerCommand::IntakeRefusal(queue_id, bypass_pause, tx))
            .map_err(|_| SchedulerError::ChannelClosed)?;
        rx.await.map_err(|_| SchedulerError::ChannelClosed)
    }

    pub(crate) fn intake_changes(&self) -> watch::Receiver<()> {
        let mut changes = self.intake_changes.clone();
        changes.mark_unchanged();
        changes
    }

    pub async fn set_mode(&self, queue_id: QueueId, mode: QueueMode) -> Result<(), SchedulerError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(SchedulerCommand::SetMode(queue_id, mode, tx))
            .map_err(|_| SchedulerError::ChannelClosed)?;
        rx.await.map_err(|_| SchedulerError::ChannelClosed)?
    }

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

    pub fn watch_depths(&self) -> QueueDepthWatch {
        self.depths.watch()
    }

    pub fn shutdown(self) {
        #[expect(
            clippy::let_underscore_must_use,
            reason = "the scheduler may already have stopped"
        )]
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

pub struct QueueSchedulerStart {
    commands: mpsc::UnboundedReceiver<SchedulerCommand>,
    depths: DepthBoard,
    intake_tx: watch::Sender<()>,
}

impl QueueSchedulerStart {
    pub fn start(self, engine: ActionEngineHandle, bus: Arc<EventBus>, initial_queues: Vec<Queue>) {
        let Self {
            commands,
            depths,
            intake_tx,
        } = self;
        let engine = Arc::new(engine);

        let mut slots: HashMap<QueueId, QueueSlot> = HashMap::with_capacity(initial_queues.len());
        for queue in initial_queues {
            let id = queue.id;
            let slot = QueueScheduler::make_queue_slot(queue, Arc::clone(&engine), &depths, &bus);
            slots.insert(id, slot);
        }

        tokio::spawn(QueueScheduler::run_scheduler(
            commands, slots, bus, engine, depths, intake_tx,
        ));
    }
}

#[derive(Clone)]
struct Launcher {
    queue_id: QueueId,
    engine: Arc<ActionEngineHandle>,
    inflight: InflightTracker,
    bus: Arc<EventBus>,
}

impl Launcher {
    fn launch(&self, task: QueueTask, permit: Option<GatePermit>) {
        let action_id = task.action_id;
        let trigger_event_id = task.trigger_event_id;
        let req = ExecutionRequest {
            action_id,
            trigger_event_id,
            trigger_kind: task.trigger_kind,
            manual_test: task.manual_test,
            initial_args: task.initial_args,
        };
        let origin = task.origin;

        let cancel = CancelSignal::new();
        let id = self.inflight.register(cancel.clone(), action_id);
        let launcher = self.clone();
        tokio::spawn(async move {
            let (done_tx, done_rx) = oneshot::channel::<()>();
            let dispatched = match origin {
                Some(origin) => {
                    launcher
                        .engine
                        .dispatch_scheduled_tracked(req, origin, cancel, done_tx)
                        .await
                }
                None => launcher.engine.dispatch_tracked(req, cancel, done_tx).await,
            };
            match dispatched {
                Ok(()) => {
                    #[expect(
                        clippy::let_underscore_must_use,
                        reason = "a dropped completion signal also means the run ended"
                    )]
                    let _ = done_rx.await;
                }
                Err(error) => {
                    warn!(action = %action_id, %error, "queued run could not reach the engine");
                    QueueScheduler::publish_skip(
                        &launcher.bus,
                        launcher.queue_id,
                        action_id,
                        trigger_event_id,
                        ENGINE_STOPPED_REASON,
                    );
                }
            }
            launcher.inflight.complete(id);
            drop(permit);
        });
    }
}

impl QueueScheduler {
    pub fn reserve() -> (QueueSchedulerHandle, QueueSchedulerStart) {
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        let depths = DepthBoard::new();
        let (intake_tx, intake_changes) = watch::channel(());
        let handle = QueueSchedulerHandle {
            sender: cmd_tx,
            depths: depths.clone(),
            intake_changes,
        };
        let start = QueueSchedulerStart {
            commands: cmd_rx,
            depths,
            intake_tx,
        };
        (handle, start)
    }

    pub fn spawn(
        engine: ActionEngineHandle,
        bus: Arc<EventBus>,
        initial_queues: Vec<Queue>,
    ) -> QueueSchedulerHandle {
        let (handle, start) = Self::reserve();
        start.start(engine, bus, initial_queues);
        handle
    }

    fn make_queue_slot(
        queue: Queue,
        engine: Arc<ActionEngineHandle>,
        depths: &DepthBoard,
        bus: &Arc<EventBus>,
    ) -> QueueSlot {
        let mode = QueueMode::default();
        let (processing_tx, processing_rx) = watch::channel(mode.processing);
        let depth = depths.register(queue.id);
        let inflight = InflightTracker::new(depth.clone());
        let pending = PendingBuffer::new(depth.clone());
        let gate = ConcurrencyGate::new(queue.concurrency.get() as usize);

        let launcher = Launcher {
            queue_id: queue.id,
            engine,
            inflight: inflight.clone(),
            bus: Arc::clone(bus),
        };
        tokio::spawn(Self::run_alongside(
            pending.clone(),
            processing_tx.subscribe(),
            launcher.clone(),
        ));
        tokio::spawn(Self::run_bounded(
            pending.clone(),
            processing_rx,
            launcher,
            gate.clone(),
        ));

        QueueSlot {
            pending,
            processing: processing_tx,
            mode,
            overflowed: 0,
            name: queue.name,
            gate,
            inflight,
            depth,
        }
    }

    async fn run_bounded(
        pending: PendingBuffer,
        mut processing: watch::Receiver<QueueProcessing>,
        launcher: Launcher,
        gate: ConcurrencyGate,
    ) {
        loop {
            let permit = gate.acquire().await;
            match Self::next_task(&pending, &mut processing, Lane::Gated, Some(&permit)).await {
                NextTask::Ready(task) => launcher.launch(task, Some(permit)),
                NextTask::Overcommitted => continue,
                NextTask::Closed => return,
            }
        }
    }

    async fn run_alongside(
        pending: PendingBuffer,
        mut processing: watch::Receiver<QueueProcessing>,
        launcher: Launcher,
    ) {
        loop {
            match Self::next_task(&pending, &mut processing, Lane::Alongside, None).await {
                NextTask::Ready(task) => launcher.launch(task, None),
                NextTask::Overcommitted => continue,
                NextTask::Closed => return,
            }
        }
    }

    async fn next_task(
        pending: &PendingBuffer,
        processing: &mut watch::Receiver<QueueProcessing>,
        lane: Lane,
        permit: Option<&GatePermit>,
    ) -> NextTask {
        loop {
            let arrived = pending.arrived(lane);
            let frozen = *processing.borrow_and_update() == QueueProcessing::Frozen;
            if permit.is_some_and(|permit| !permit.within_limit()) {
                return NextTask::Overcommitted;
            }
            if let Some(task) = pending.take_next(lane, frozen) {
                return NextTask::Ready(task);
            }

            tokio::select! {
                biased;
                changed = processing.changed() => {
                    if changed.is_err() {
                        return NextTask::Closed;
                    }
                }
                _ = arrived => {}
            }
        }
    }

    async fn run_scheduler(
        mut cmd_rx: mpsc::UnboundedReceiver<SchedulerCommand>,
        mut slots: HashMap<QueueId, QueueSlot>,
        bus: Arc<EventBus>,
        engine: Arc<ActionEngineHandle>,
        depths: DepthBoard,
        intake_changed: watch::Sender<()>,
    ) {
        while let Some(cmd) = cmd_rx.recv().await {
            match cmd {
                SchedulerCommand::Enqueue(req) => {
                    #[expect(
                        clippy::let_underscore_must_use,
                        reason = "refusals are published as skip events inside enqueue"
                    )]
                    let _ = Self::enqueue(req, Arrival::Triggered, &mut slots, &bus);
                }
                SchedulerCommand::Offer(req, reply) => {
                    let admitted = Self::enqueue(req, Arrival::Triggered, &mut slots, &bus)
                        .map_err(SchedulerError::Refused);
                    #[expect(
                        clippy::let_underscore_must_use,
                        reason = "the requester may stop waiting for the reply"
                    )]
                    let _ = reply.send(admitted);
                }
                SchedulerCommand::Admit(req, origin, reply) => {
                    let admitted = Self::enqueue(req, Arrival::Scheduled(origin), &mut slots, &bus)
                        .map_err(SchedulerError::Refused);
                    #[expect(
                        clippy::let_underscore_must_use,
                        reason = "the requester may stop waiting for the reply"
                    )]
                    let _ = reply.send(admitted);
                }
                SchedulerCommand::IntakeRefusal(queue_id, bypass_pause, reply) => {
                    let refusal = slots
                        .get(&queue_id)
                        .and_then(|slot| slot.mode.intake_refusal(bypass_pause));
                    #[expect(
                        clippy::let_underscore_must_use,
                        reason = "the requester may stop waiting for the reply"
                    )]
                    let _ = reply.send(refusal);
                }
                SchedulerCommand::SetMode(queue_id, mode, reply) => {
                    let r = Self::set_mode(&queue_id, mode, &mut slots, &bus);
                    if r.is_ok() {
                        intake_changed.send_replace(());
                    }
                    #[expect(
                        clippy::let_underscore_must_use,
                        reason = "the requester may stop waiting for the reply"
                    )]
                    let _ = reply.send(r);
                }
                SchedulerCommand::Clear(queue_id, keep_current, reply) => {
                    let r = Self::clear_queue(&slots, &queue_id, keep_current, &bus);
                    #[expect(
                        clippy::let_underscore_must_use,
                        reason = "the requester may stop waiting for the reply"
                    )]
                    let _ = reply.send(r);
                }
                SchedulerCommand::Register(queue, reply) => {
                    let outcome = if slots.contains_key(&queue.id) {
                        MembershipOutcome::AlreadyRegistered
                    } else {
                        let id = queue.id;
                        let slot = Self::make_queue_slot(queue, Arc::clone(&engine), &depths, &bus);
                        slots.insert(id, slot);
                        intake_changed.send_replace(());
                        MembershipOutcome::Applied
                    };
                    #[expect(
                        clippy::let_underscore_must_use,
                        reason = "the requester may stop waiting for the reply"
                    )]
                    let _ = reply.send(outcome);
                }
                SchedulerCommand::Deregister(queue_id, reply) => {
                    let outcome = match slots.remove(&queue_id) {
                        Some(slot) => {
                            for task in slot.pending.take_all() {
                                Self::publish_skip(
                                    &bus,
                                    queue_id,
                                    task.action_id,
                                    task.trigger_event_id,
                                    QUEUE_DELETED_REASON,
                                );
                            }
                            depths.deregister(&queue_id);
                            intake_changed.send_replace(());
                            MembershipOutcome::Applied
                        }
                        None => MembershipOutcome::NotFound,
                    };
                    #[expect(
                        clippy::let_underscore_must_use,
                        reason = "the requester may stop waiting for the reply"
                    )]
                    let _ = reply.send(outcome);
                }
                SchedulerCommand::Reconfigure(queue, reply) => {
                    let outcome = Self::reconfigure(queue, &mut slots);
                    #[expect(
                        clippy::let_underscore_must_use,
                        reason = "the requester may stop waiting for the reply"
                    )]
                    let _ = reply.send(outcome);
                }
                SchedulerCommand::QueryStates(reply) => {
                    #[expect(
                        clippy::let_underscore_must_use,
                        reason = "the requester may stop waiting for the reply"
                    )]
                    let _ = reply.send(Self::queue_states(&slots));
                }
                SchedulerCommand::Shutdown => break,
            }
        }
    }

    fn enqueue(
        req: SchedulerRequest,
        arrival: Arrival,
        slots: &mut HashMap<QueueId, QueueSlot>,
        bus: &Arc<EventBus>,
    ) -> Result<(), &'static str> {
        let SchedulerRequest {
            queue_id,
            action_id,
            trigger_event_id,
            trigger_kind,
            manual_test,
            initial_args,
            admission,
        } = req;

        let Some(slot) = slots.get_mut(&queue_id) else {
            warn!("enqueue: queue {queue_id} not found, dropping");
            Self::publish_skip(
                bus,
                queue_id,
                action_id,
                trigger_event_id,
                QUEUE_NOT_FOUND_REASON,
            );
            return Err(QUEUE_NOT_FOUND_REASON);
        };

        let origin = match arrival {
            Arrival::Triggered => {
                if let Some(reason) = slot.mode.intake_refusal(admission.bypass_pause) {
                    Self::publish_skip(bus, queue_id, action_id, trigger_event_id, reason);
                    return Err(reason);
                }
                None
            }
            Arrival::Scheduled(origin) => Some(origin),
        };

        let accepted = slot.pending.push(QueueTask {
            action_id,
            trigger_event_id,
            trigger_kind,
            manual_test,
            origin,
            initial_args,
            admission,
        });

        if accepted {
            return Ok(());
        }
        slot.overflowed = slot.overflowed.saturating_add(1);
        let overflowed = slot.overflowed;
        slot.depth.update(|depth| depth.overflowed = overflowed);
        Self::publish_skip(
            bus,
            queue_id,
            action_id,
            trigger_event_id,
            QUEUE_OVERFLOW_REASON,
        );
        Err(QUEUE_OVERFLOW_REASON)
    }

    fn publish_skip(
        bus: &EventBus,
        queue_id: QueueId,
        action_id: ActionId,
        trigger_event_id: EventId,
        reason: &str,
    ) {
        bus.publish(
            ActionSkipped {
                action_id,
                reason: reason.to_owned(),
                queue_id,
            }
            .into_event(EventSource::Core, Some(trigger_event_id)),
        );
    }

    fn queue_states(slots: &HashMap<QueueId, QueueSlot>) -> HashMap<QueueId, QueueRuntimeState> {
        slots
            .iter()
            .map(|(id, slot)| {
                (
                    *id,
                    QueueRuntimeState {
                        mode: slot.mode,
                        pending: slot.pending.len(),
                        in_flight: slot.inflight.len(),
                        overflowed: slot.overflowed,
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
        slot.overflowed = 0;
        slot.depth.update(|depth| depth.overflowed = 0);
        slot.processing.send_replace(mode.processing);

        bus.publish(
            QueueModeChanged {
                queue_id: *queue_id,
                queue_name: slot.name.clone(),
                mode,
            }
            .into_event(),
        );

        Ok(())
    }

    fn reconfigure(queue: Queue, slots: &mut HashMap<QueueId, QueueSlot>) -> MembershipOutcome {
        let Some(slot) = slots.get_mut(&queue.id) else {
            return MembershipOutcome::NotFound;
        };

        let limit = queue.concurrency.get() as usize;
        let previous = slot.gate.replace_limit(limit);
        if previous != limit {
            info!(
                "queue {}: concurrency {previous} -> {limit} with {} executions still running",
                queue.name,
                slot.inflight.len()
            );
        }
        slot.name = queue.name;

        MembershipOutcome::Applied
    }

    fn clear_queue(
        slots: &HashMap<QueueId, QueueSlot>,
        queue_id: &QueueId,
        keep_current: bool,
        bus: &Arc<EventBus>,
    ) -> Result<(), SchedulerError> {
        let slot = slots
            .get(queue_id)
            .ok_or(SchedulerError::QueueNotFound(*queue_id))?;

        if !keep_current {
            slot.inflight.cancel_all();
        }
        slot.pending.clear();

        bus.publish(Event::new(
            EventSource::Core,
            QUEUE_CLEARED_KIND,
            json!({
                "queue_id": queue_id.to_string(),
                "queue_name": slot.name,
                "keep_current": keep_current,
            }),
        ));

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use crate::test_support::{Sandboxed, sandboxed_backend};
    use forge_registry::SubActionRegistry;
    use forge_storage::DataProvider;
    use forge_types::{Action, ActionId, EventId, Queue, QueueId, SubActionStep};

    use super::*;
    use crate::{EventBus, NullEventLogRepo, spawn_action_engine};
    use forge_events::EventSubscription;

    async fn make_dp() -> Sandboxed<Arc<dyn DataProvider>> {
        sandboxed_backend([0xab; 32])
            .await
            .map(|backend| Arc::new(backend) as Arc<dyn DataProvider>)
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
            concurrency: std::num::NonZeroU32::new(8).unwrap(),
        }
    }

    fn blocking_q(id: QueueId) -> Queue {
        Queue {
            id,
            name: "serial".to_string(),
            description: String::new(),
            concurrency: std::num::NonZeroU32::MIN,
        }
    }

    const HANG_GUARD: Duration = Duration::from_secs(30);

    const BYPASS_PAUSE: QueueAdmission = QueueAdmission {
        bypass_pause: true,
        run_alongside: false,
    };

    const ALONGSIDE: QueueAdmission = QueueAdmission {
        bypass_pause: false,
        run_alongside: true,
    };

    async fn events_until(
        sub: &mut EventSubscription,
        done: impl Fn(&[Event]) -> bool,
    ) -> Option<Vec<Event>> {
        let mut seen = Vec::new();
        tokio::time::timeout(HANG_GUARD, async {
            while !done(&seen) {
                match sub.recv().await {
                    Ok(ev) => seen.push(ev),
                    Err(forge_events::EventsError::LaggingReceiver) => {}
                    Err(e) => panic!("bus failed: {e:?}"),
                }
            }
        })
        .await
        .ok()
        .map(|()| seen)
    }

    fn has(events: &[Event], kind: &str, action_id: ActionId) -> bool {
        ids_of(events, kind).contains(&action_id.to_string())
    }

    fn buffered(sub: &mut EventSubscription) -> Vec<Event> {
        let mut seen = Vec::new();
        while let Ok(Some(ev)) = sub.try_recv() {
            seen.push(ev);
        }
        seen
    }

    async fn collect_events(sub: &mut EventSubscription, target_kind: &str) -> bool {
        collect_event(sub, target_kind).await.is_some()
    }

    struct HeldRunner {
        release: Arc<tokio::sync::Semaphore>,
        cancelled: tokio::sync::mpsc::UnboundedSender<()>,
    }

    #[async_trait::async_trait]
    impl forge_registry::SubActionRunner for HeldRunner {
        fn id(&self) -> &str {
            "test.held"
        }
        fn category(&self) -> forge_registry::SubActionCategory {
            forge_registry::SubActionCategory::Util
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
        fn default_config(&self) -> forge_types::SubActionConfig {
            forge_types::SubActionConfig::new()
        }
        fn config_fields(&self) -> Vec<forge_registry::FormField> {
            Vec::new()
        }
        fn validate_config(
            &self,
            _: &forge_types::SubActionConfig,
        ) -> Result<(), forge_registry::RegistryError> {
            Ok(())
        }
        async fn execute(
            &self,
            _: &forge_types::SubActionConfig,
            ctx: &forge_registry::RunContext<'_>,
        ) -> (forge_types::SubActionTelemetry, Option<ArgStack>) {
            loop {
                if ctx.cancel.is_cancelled() {
                    #[expect(
                        clippy::let_underscore_must_use,
                        reason = "the receiver may already be gone"
                    )]
                    let _ = self.cancelled.send(());
                    break;
                }
                if let Ok(permit) = self.release.try_acquire() {
                    permit.forget();
                    break;
                }
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
            (
                forge_types::SubActionTelemetry {
                    args_in: std::collections::BTreeMap::new(),
                    produced: std::collections::BTreeMap::new(),
                    index: ctx.index,
                    kind: "test.held".to_owned(),
                    started_at: time::OffsetDateTime::now_utc(),
                    duration_ms: 0,
                    outcome: forge_types::SubActionOutcome::Success,
                },
                None,
            )
        }
    }

    struct Held {
        release: Arc<tokio::sync::Semaphore>,
        cancelled: tokio::sync::mpsc::UnboundedReceiver<()>,
    }

    impl Held {
        fn release_one(&self) {
            self.release.add_permits(1);
        }

        async fn saw_a_cancelled_run(&mut self) -> bool {
            tokio::time::timeout(HANG_GUARD, self.cancelled.recv())
                .await
                .is_ok_and(|ended| ended.is_some())
        }
    }

    fn held_registry() -> (Arc<SubActionRegistry>, Held) {
        let release = Arc::new(tokio::sync::Semaphore::new(0));
        let (cancelled_tx, cancelled) = tokio::sync::mpsc::unbounded_channel();
        let mut reg = SubActionRegistry::new();
        reg.register(Box::new(HeldRunner {
            release: Arc::clone(&release),
            cancelled: cancelled_tx,
        }))
        .unwrap();
        (Arc::new(reg), Held { release, cancelled })
    }

    fn held_action(id: ActionId, queue_id: QueueId, held: bool) -> Action {
        let mut action = log_action(id, queue_id);
        if held {
            action.sub_actions[0].kind_id = "test.held".to_owned();
        }
        action
    }

    fn spawn_sched(
        dp: &Arc<dyn DataProvider>,
        bus: &Arc<EventBus>,
        registry: Arc<SubActionRegistry>,
        queues: Vec<Queue>,
    ) -> QueueSchedulerHandle {
        let engine = spawn_action_engine(
            Arc::clone(bus),
            crate::Catalog::new(
                dp.action_repo(),
                dp.trigger_instance_repo(),
                dp.catalog_revision(),
            ),
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

    async fn collect_event(sub: &mut EventSubscription, target_kind: &str) -> Option<Event> {
        events_until(sub, |seen| {
            seen.last().is_some_and(|ev| ev.kind == target_kind)
        })
        .await
        .and_then(|mut seen| seen.pop())
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
        let sched = spawn_sched(&dp, &bus, Arc::new(SubActionRegistry::new()), vec![queue]);
        let mut sub = bus.subscribe();

        sched.dispatch(req(q_id, a_id)).await.unwrap();

        assert!(
            collect_events(&mut sub, "action.done").await,
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
        let sched = spawn_sched(&dp, &bus, Arc::new(SubActionRegistry::new()), vec![queue]);
        let mut sub = bus.subscribe();

        for a_id in &action_ids {
            sched.dispatch(req(q_id, *a_id)).await.unwrap();
        }

        let dones = |seen: &[Event]| seen.iter().filter(|ev| ev.kind == "action.done").count();
        let seen = events_until(&mut sub, |seen| dones(seen) == 3).await;

        assert!(seen.is_some(), "all three serialized actions must complete");
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
        let sched = spawn_sched(&dp, &bus, Arc::new(SubActionRegistry::new()), vec![queue]);
        let mut sub = bus.subscribe();

        sched.set_mode(q_id, QueueMode::PAUSED).await.unwrap();

        let trigger = EventId::new();
        sched
            .dispatch(SchedulerRequest {
                trigger_event_id: trigger,
                ..req(q_id, a_id)
            })
            .await
            .unwrap();

        let skipped = collect_event(&mut sub, "action.skipped")
            .await
            .expect("paused queue must emit action.skipped");
        assert_eq!(skipped.payload["reason"].as_str(), Some("queue_paused"));
        assert_eq!(
            skipped.caused_by,
            Some(trigger),
            "action.skipped must link caused_by to the triggering event"
        );

        assert!(
            record(&mut sub, Duration::from_millis(180))
                .await
                .iter()
                .all(|ev| ev.kind != "action.done"),
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
        let sched = spawn_sched(&dp, &bus, Arc::new(SubActionRegistry::new()), vec![queue]);
        let mut sub = bus.subscribe();

        sched.set_mode(q_id, QueueMode::PAUSED).await.unwrap();

        sched
            .dispatch(SchedulerRequest {
                admission: BYPASS_PAUSE,
                ..req(q_id, a_id)
            })
            .await
            .unwrap();

        assert!(
            collect_events(&mut sub, "action.done").await,
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
        let sched = spawn_sched(&dp, &bus, Arc::new(SubActionRegistry::new()), vec![queue]);
        let mut sub = bus.subscribe();

        sched.set_mode(q_id, QueueMode::PAUSED).await.unwrap();
        sched.set_mode(q_id, QueueMode::RUNNING).await.unwrap();

        sched.dispatch(req(q_id, a_id)).await.unwrap();

        assert!(
            collect_events(&mut sub, "action.done").await,
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
            let published = collect_event(&mut sub, kind).await;
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
        let sched = spawn_sched(&dp, &bus, Arc::new(SubActionRegistry::new()), vec![queue]);
        let mut sub = bus.subscribe();

        let trigger = EventId::new();
        sched
            .dispatch(SchedulerRequest {
                trigger_event_id: trigger,
                ..req(unknown_q, a_id)
            })
            .await
            .unwrap();

        let skipped = collect_event(&mut sub, "action.skipped")
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
        let sched = spawn_sched(&dp, &bus, Arc::new(SubActionRegistry::new()), vec![]);
        let mut sub = bus.subscribe();

        let outcome = sched.register(queue).await.unwrap();
        assert_eq!(outcome, MembershipOutcome::Applied);

        sched.dispatch(req(q_id, a_id)).await.unwrap();

        assert!(
            collect_events(&mut sub, "action.done").await,
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
        let sched = spawn_sched(
            &dp,
            &bus,
            Arc::new(SubActionRegistry::new()),
            vec![nonblocking(q_id)],
        );
        let mut sub = bus.subscribe();

        sched.set_mode(q_id, QueueMode::PAUSED).await.unwrap();
        let outcome = sched.register(nonblocking(q_id)).await.unwrap();
        assert_eq!(outcome, MembershipOutcome::AlreadyRegistered);

        sched.dispatch(req(q_id, a_id)).await.unwrap();

        let state = sched.queue_states().await.unwrap().remove(&q_id).unwrap();
        assert_eq!(
            state.mode,
            QueueMode::PAUSED,
            "re-registering must not un-pause the existing slot"
        );
        let skipped = collect_event(&mut sub, "action.skipped")
            .await
            .expect("the surviving paused slot must keep refusing dispatches");
        assert_eq!(skipped.payload["reason"].as_str(), Some("queue_paused"));
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
        let sched = spawn_sched(&dp, &bus, Arc::new(SubActionRegistry::new()), vec![queue]);
        let mut sub = bus.subscribe();

        let outcome = sched.deregister(q_id).await.unwrap();
        assert_eq!(outcome, MembershipOutcome::Applied);

        sched.dispatch(req(q_id, a_id)).await.unwrap();

        assert!(
            collect_events(&mut sub, "action.skipped").await,
            "dispatch after deregister must emit action.skipped"
        );
        sched.shutdown();
    }

    #[tokio::test]
    async fn deregistering_a_queue_skips_its_buffered_work_in_any_mode_and_lets_the_running_task_end()
     {
        for mode in [QueueMode::default(), QueueMode::HOLDING] {
            let q_id = QueueId::new();
            let (running, waiting) = (ActionId::new(), ActionId::new());
            let (_dp, bus, sched, held) =
                serial_queue_with_held(&[(running, true), (waiting, false)], q_id).await;
            let mut sub = bus.subscribe();
            sched.dispatch(req(q_id, running)).await.unwrap();
            assert!(await_action_start(&mut sub, running).await, "{mode:?}");
            sched.dispatch(req(q_id, waiting)).await.unwrap();
            sched.set_mode(q_id, mode).await.unwrap();

            assert_eq!(
                sched.deregister(q_id).await.unwrap(),
                MembershipOutcome::Applied,
                "{mode:?}"
            );
            held.release_one();

            let seen = events_until_done(&mut sub, running)
                .await
                .expect("the task already in flight still finishes");
            assert_eq!(
                (
                    skip_reasons(&seen),
                    ids_of(&seen, "action.skipped"),
                    has(&seen, "action.start", waiting),
                ),
                (vec![QUEUE_DELETED_REASON], vec![waiting.to_string()], false),
                "{mode:?}"
            );
            sched.shutdown();
        }
    }

    #[tokio::test]
    async fn deregister_unknown_queue_returns_not_found_without_panic() {
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let dp = make_dp().await;
        let sched = spawn_sched(&dp, &bus, Arc::new(SubActionRegistry::new()), vec![]);

        let outcome = sched.deregister(QueueId::new()).await.unwrap();
        assert_eq!(outcome, MembershipOutcome::NotFound);
        sched.shutdown();
    }

    #[tokio::test]
    async fn reconfigure_blocking_flip_keeps_accepting_work() {
        let dp = make_dp().await;
        let q_id = QueueId::new();
        let a_id = ActionId::new();
        let queue = nonblocking(q_id);
        let action = log_action(a_id, q_id);
        seed(&dp, &queue, &action).await;

        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let sched = spawn_sched(
            &dp,
            &bus,
            Arc::new(SubActionRegistry::new()),
            vec![nonblocking(q_id)],
        );
        let mut sub = bus.subscribe();

        let flipped = blocking_q(q_id);
        let outcome = sched.reconfigure(flipped).await.unwrap();
        assert_eq!(outcome, MembershipOutcome::Applied);

        sched.dispatch(req(q_id, a_id)).await.unwrap();

        assert!(
            collect_events(&mut sub, "action.done").await,
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
        let sched = spawn_sched(
            &dp,
            &bus,
            Arc::new(SubActionRegistry::new()),
            vec![nonblocking(q_id)],
        );
        let mut sub = bus.subscribe();

        sched.set_mode(q_id, QueueMode::PAUSED).await.unwrap();
        let outcome = sched.reconfigure(blocking_q(q_id)).await.unwrap();
        assert_eq!(outcome, MembershipOutcome::Applied);

        sched.dispatch(req(q_id, a_id)).await.unwrap();

        let state = sched.queue_states().await.unwrap().remove(&q_id).unwrap();
        assert_eq!(
            state.mode,
            QueueMode::PAUSED,
            "a blocking-flip reconfigure must leave the pause in place"
        );
        let skipped = collect_event(&mut sub, "action.skipped")
            .await
            .expect("a paused queue must keep refusing dispatches after a reconfigure");
        assert_eq!(skipped.payload["reason"].as_str(), Some("queue_paused"));
        sched.shutdown();
    }

    #[tokio::test]
    async fn reconfigure_unknown_queue_returns_not_found() {
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let dp = make_dp().await;
        let sched = spawn_sched(&dp, &bus, Arc::new(SubActionRegistry::new()), vec![]);

        let outcome = sched
            .reconfigure(Queue {
                id: QueueId::new(),
                name: "ghost".to_string(),
                description: String::new(),
                concurrency: std::num::NonZeroU32::new(8).unwrap(),
            })
            .await
            .unwrap();
        assert_eq!(outcome, MembershipOutcome::NotFound);
        sched.shutdown();
    }

    fn req(queue_id: QueueId, action_id: ActionId) -> SchedulerRequest {
        SchedulerRequest {
            queue_id,
            action_id,
            trigger_event_id: EventId::new(),
            trigger_kind: None,
            manual_test: false,
            initial_args: ArgStack::new(),
            admission: QueueAdmission::default(),
        }
    }

    fn action_id_of(ev: &Event) -> Option<String> {
        ev.payload
            .get("action_id")
            .and_then(|v| v.as_str())
            .map(str::to_owned)
    }

    async fn await_action_start(sub: &mut EventSubscription, action_id: ActionId) -> bool {
        events_until(sub, |seen| has(seen, "action.start", action_id))
            .await
            .is_some()
    }

    async fn events_until_done(
        sub: &mut EventSubscription,
        action_id: ActionId,
    ) -> Option<Vec<Event>> {
        events_until(sub, |seen| has(seen, "action.done", action_id)).await
    }

    async fn await_queue_event(sub: &mut EventSubscription, kind: &str, queue_id: QueueId) -> bool {
        let target = queue_id.to_string();
        events_until(sub, |seen| {
            seen.iter().any(|ev| {
                ev.kind == kind
                    && ev.payload.get("queue_id").and_then(|v| v.as_str()) == Some(target.as_str())
            })
        })
        .await
        .is_some()
    }

    #[tokio::test]
    async fn clear_discards_pending_and_the_queue_keeps_running_new_work() {
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
        for (id, held) in [(a, true), (b, false), (c, false), (d, false)] {
            dp.action_repo()
                .save(&held_action(id, q_id, held))
                .await
                .unwrap();
        }
        let (registry, held) = held_registry();

        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let sched = spawn_sched(&dp, &bus, registry, vec![queue]);
        let mut sub = bus.subscribe();

        sched.dispatch(req(q_id, a)).await.unwrap();
        assert!(
            await_action_start(&mut sub, a).await,
            "A must occupy the blocking slot before clearing"
        );
        sched.dispatch(req(q_id, b)).await.unwrap();
        sched.dispatch(req(q_id, c)).await.unwrap();

        sched.clear(q_id, true).await.unwrap();
        sched.dispatch(req(q_id, d)).await.unwrap();
        held.release_one();

        let seen = events_until_done(&mut sub, d)
            .await
            .expect("the queue must run work dispatched after clear");
        assert!(
            has(&seen, "action.done", a),
            "keep_current=true must let the in-flight A finish"
        );
        assert!(
            !has(&seen, "action.start", b),
            "pending B must be discarded by clear"
        );
        assert!(
            !has(&seen, "action.start", c),
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
        for (id, held) in [(a, true), (d, false)] {
            dp.action_repo()
                .save(&held_action(id, q_id, held))
                .await
                .unwrap();
        }
        let (registry, mut held) = held_registry();

        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let sched = spawn_sched(&dp, &bus, registry, vec![queue]);
        let mut sub = bus.subscribe();

        sched.dispatch(req(q_id, a)).await.unwrap();
        assert!(
            await_action_start(&mut sub, a).await,
            "A must be in-flight before clearing"
        );

        sched.clear(q_id, false).await.unwrap();
        sched.dispatch(req(q_id, d)).await.unwrap();

        let seen = events_until_done(&mut sub, d).await;
        assert!(
            seen.is_some(),
            "queue must keep working after a keep_current=false clear"
        );
        assert!(
            held.saw_a_cancelled_run().await,
            "keep_current=false must abort the in-flight action"
        );
        sched.shutdown();
    }

    #[tokio::test]
    async fn clear_preserves_the_queue_mode() {
        let dp = make_dp().await;
        let q_id = QueueId::new();
        let queue = nonblocking(q_id);
        dp.queue_repo().save(&queue).await.unwrap();

        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let sched = spawn_sched(&dp, &bus, Arc::new(SubActionRegistry::new()), vec![queue]);

        sched.set_mode(q_id, QueueMode::PAUSED).await.unwrap();
        sched.clear(q_id, true).await.unwrap();

        let states = sched.queue_states().await.unwrap();
        assert_eq!(
            states.get(&q_id).map(|s| s.mode),
            Some(QueueMode::PAUSED),
            "clear must leave the queue mode alone"
        );
        sched.shutdown();
    }

    #[tokio::test]
    async fn clear_unknown_queue_returns_queue_not_found() {
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let dp = make_dp().await;
        let sched = spawn_sched(&dp, &bus, Arc::new(SubActionRegistry::new()), vec![]);

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
        let sched = spawn_sched(&dp, &bus, Arc::new(SubActionRegistry::new()), vec![queue]);
        let mut sub = bus.subscribe();

        sched.clear(q_id, true).await.unwrap();

        assert!(
            await_queue_event(&mut sub, "queue.cleared", q_id).await,
            "clear must emit queue.cleared carrying the cleared queue_id"
        );
        sched.shutdown();
    }

    async fn serial_queue_with_held(
        actions: &[(ActionId, bool)],
        q_id: QueueId,
    ) -> (
        Sandboxed<Arc<dyn DataProvider>>,
        Arc<EventBus>,
        QueueSchedulerHandle,
        Held,
    ) {
        let dp = make_dp().await;
        dp.queue_repo().save(&blocking_q(q_id)).await.unwrap();
        for (id, held) in actions {
            dp.action_repo()
                .save(&held_action(*id, q_id, *held))
                .await
                .unwrap();
        }
        let (registry, held) = held_registry();
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let sched = spawn_sched(&dp, &bus, registry, vec![blocking_q(q_id)]);
        (dp, bus, sched, held)
    }

    async fn settles_at(
        sched: &QueueSchedulerHandle,
        q_id: QueueId,
        expected: crate::QueueDepth,
    ) -> Option<crate::QueueDepth> {
        settled_depth(&mut sched.watch_depths(), q_id, expected).await
    }

    #[tokio::test]
    async fn freezing_lets_the_running_task_finish_but_withholds_the_next_one() {
        let q_id = QueueId::new();
        let (a, b) = (ActionId::new(), ActionId::new());
        let (_dp, bus, sched, held) = serial_queue_with_held(&[(a, true), (b, false)], q_id).await;
        let mut sub = bus.subscribe();

        sched.dispatch(req(q_id, a)).await.unwrap();
        assert!(
            await_action_start(&mut sub, a).await,
            "A must occupy the single slot before the freeze"
        );
        sched.dispatch(req(q_id, b)).await.unwrap();
        sched.set_mode(q_id, QueueMode::HOLDING).await.unwrap();
        held.release_one();

        let mut seen = events_until_done(&mut sub, a)
            .await
            .expect("a task already in flight when the freeze lands must still finish");
        let settled = settles_at(&sched, q_id, depth(1, 0, 0)).await;
        seen.extend(buffered(&mut sub));
        assert_eq!(
            (settled, has(&seen, "action.start", b)),
            (Some(depth(1, 0, 0)), false),
            "a frozen queue must stop between tasks, not start the buffered B"
        );
        sched.shutdown();
    }

    #[tokio::test]
    async fn resuming_runs_tasks_buffered_while_frozen_in_enqueue_order() {
        let q_id = QueueId::new();
        let (a, b, c) = (ActionId::new(), ActionId::new(), ActionId::new());
        let (_dp, bus, sched, _held) =
            serial_queue_with_held(&[(a, false), (b, false), (c, false)], q_id).await;
        let mut sub = bus.subscribe();

        sched.set_mode(q_id, QueueMode::HOLDING).await.unwrap();
        for id in [a, b, c] {
            sched.dispatch(req(q_id, id)).await.unwrap();
        }

        let settled = settles_at(&sched, q_id, depth(3, 0, 0)).await;
        let while_frozen = buffered(&mut sub);
        assert_eq!(
            (settled, ids_of(&while_frozen, "action.start")),
            (Some(depth(3, 0, 0)), Vec::<String>::new()),
            "a holding queue must collect the dispatches without starting them"
        );

        sched.set_mode(q_id, QueueMode::RUNNING).await.unwrap();
        let after_resume = events_until_done(&mut sub, c).await.unwrap_or_default();
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
        let (_dp, bus, sched, held) =
            serial_queue_with_held(&[(a, true), (b, false), (c, false)], q_id).await;
        let mut sub = bus.subscribe();

        sched.dispatch(req(q_id, a)).await.unwrap();
        assert!(
            await_action_start(&mut sub, a).await,
            "A must occupy the single slot before draining starts"
        );
        sched.dispatch(req(q_id, b)).await.unwrap();
        sched.set_mode(q_id, QueueMode::DRAINING).await.unwrap();
        sched.dispatch(req(q_id, c)).await.unwrap();
        held.release_one();

        let seen = events_until(&mut sub, |seen| {
            has(seen, "action.done", b) && !skip_reasons(seen).is_empty()
        })
        .await;
        assert!(
            seen.is_some(),
            "draining must keep executing work buffered before the switch"
        );
        let seen = seen.unwrap_or_default();
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
        let (_dp, bus, sched, held) = serial_queue_with_held(&[(a, true), (b, false)], q_id).await;
        let mut sub = bus.subscribe();

        sched.dispatch(req(q_id, a)).await.unwrap();
        assert!(
            await_action_start(&mut sub, a).await,
            "A must occupy the single slot before the pause"
        );
        sched.dispatch(req(q_id, b)).await.unwrap();
        sched.set_mode(q_id, QueueMode::PAUSED).await.unwrap();
        held.release_one();

        let mut while_paused = events_until_done(&mut sub, a).await.unwrap_or_default();
        let settled = settles_at(&sched, q_id, depth(1, 0, 0)).await;
        while_paused.extend(buffered(&mut sub));
        assert_eq!(
            (settled, has(&while_paused, "action.start", b)),
            (Some(depth(1, 0, 0)), false),
            "pausing must freeze processing too, not only close intake"
        );

        sched.set_mode(q_id, QueueMode::RUNNING).await.unwrap();
        assert!(
            events_until_done(&mut sub, b).await.is_some(),
            "resuming must release the withheld task"
        );
        sched.shutdown();
    }

    #[tokio::test]
    async fn queue_states_reports_live_pending_and_in_flight_counts() {
        let q_id = QueueId::new();
        let (a, b, c) = (ActionId::new(), ActionId::new(), ActionId::new());
        let (_dp, bus, sched, _held) =
            serial_queue_with_held(&[(a, true), (b, false), (c, false)], q_id).await;
        let mut sub = bus.subscribe();

        sched.dispatch(req(q_id, a)).await.unwrap();
        assert!(
            await_action_start(&mut sub, a).await,
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

        let skipped = collect_event(&mut sub, "action.skipped")
            .await
            .expect("the dispatch past the cap must report a skip");
        assert_eq!(
            skipped.payload["reason"].as_str(),
            Some("queue_pending_overflow"),
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

    fn outstanding(gate: &ConcurrencyGate) -> usize {
        gate.lock().outstanding
    }

    #[test]
    fn gate_take_admits_up_to_the_limit_and_refuses_at_it() {
        let gate = ConcurrencyGate::new(2);
        assert!(gate.take(), "0 of 2 slots taken must admit");
        assert!(gate.take(), "1 of 2 slots taken must admit");
        assert!(!gate.take(), "2 of 2 slots taken must refuse");
        assert_eq!(outstanding(&gate), 2, "a refused take must not count");
    }

    #[tokio::test]
    async fn dropping_a_gate_permit_frees_exactly_one_slot() {
        let gate = ConcurrencyGate::new(2);
        let first = gate.acquire().await;
        let _second = gate.acquire().await;

        drop(first);

        assert!(gate.take(), "the dropped permit's slot must be free again");
        assert!(
            !gate.take(),
            "one dropped permit must free exactly one slot"
        );
    }

    #[tokio::test]
    async fn lowering_the_gate_limit_keeps_held_permits_and_admits_nothing_until_below_it() {
        let gate = ConcurrencyGate::new(3);
        let mut held = vec![
            gate.acquire().await,
            gate.acquire().await,
            gate.acquire().await,
        ];

        assert_eq!(gate.replace_limit(1), 3);
        assert_eq!(outstanding(&gate), 3, "a lowered limit must not preempt");

        for still_held in [2, 1] {
            held.pop();
            assert!(
                !gate.take(),
                "{still_held} held permits must still block a limit of 1"
            );
        }
        held.pop();
        assert!(
            gate.take(),
            "the limit must admit once every old permit is gone"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn widening_the_gate_limit_wakes_a_blocked_acquire() {
        let gate = ConcurrencyGate::new(1);
        let _held = gate.acquire().await;
        let waiter = tokio::spawn({
            let gate = gate.clone();
            async move { gate.acquire().await }
        });
        tokio::task::yield_now().await;
        assert!(!waiter.is_finished(), "a full gate must block the acquire");

        gate.replace_limit(2);

        let woken = tokio::time::timeout(Duration::from_secs(30), waiter).await;
        assert!(
            woken.is_ok(),
            "a widened limit must wake the acquire without waiting for a permit drop"
        );
    }

    async fn await_depths(
        watch: &mut QueueDepthWatch,
        settled: impl Fn(&crate::QueueDepths) -> bool,
    ) -> crate::QueueDepths {
        let mut depths = watch.current();
        while !settled(&depths) {
            match tokio::time::timeout(HANG_GUARD, watch.changed()).await {
                Ok(Some(next)) => depths = next,
                _ => break,
            }
        }
        depths
    }

    async fn settled_depth(
        watch: &mut QueueDepthWatch,
        q_id: QueueId,
        expected: crate::QueueDepth,
    ) -> Option<crate::QueueDepth> {
        await_depths(watch, |depths| depths.get(&q_id) == Some(&expected))
            .await
            .get(&q_id)
            .copied()
    }

    fn depth(pending: usize, in_flight: usize, overflowed: u64) -> crate::QueueDepth {
        crate::QueueDepth {
            pending,
            in_flight,
            overflowed,
        }
    }

    async fn held_log_queues(
        q_ids: &[QueueId],
        a_id: ActionId,
    ) -> (Sandboxed<Arc<dyn DataProvider>>, QueueSchedulerHandle) {
        let dp = make_dp().await;
        for q_id in q_ids {
            seed(&dp, &nonblocking(*q_id), &log_action(a_id, *q_id)).await;
        }
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let queues = q_ids.iter().map(|q_id| nonblocking(*q_id)).collect();
        let sched = spawn_sched(&dp, &bus, Arc::new(SubActionRegistry::new()), queues);
        for q_id in q_ids {
            sched.set_mode(*q_id, QueueMode::HOLDING).await.unwrap();
        }
        (dp, sched)
    }

    #[tokio::test]
    async fn depth_watch_counts_work_held_by_a_frozen_queue_and_empties_once_it_runs() {
        let q_id = QueueId::new();
        let a_id = ActionId::new();
        let (_dp, sched) = held_log_queues(&[q_id], a_id).await;
        let mut watch = sched.watch_depths();

        for _ in 0..3 {
            sched.dispatch(req(q_id, a_id)).await.unwrap();
        }
        assert_eq!(
            settled_depth(&mut watch, q_id, depth(3, 0, 0)).await,
            Some(depth(3, 0, 0))
        );

        sched.set_mode(q_id, QueueMode::RUNNING).await.unwrap();
        assert_eq!(
            settled_depth(&mut watch, q_id, depth(0, 0, 0)).await,
            Some(depth(0, 0, 0))
        );
        sched.shutdown();
    }

    #[tokio::test]
    async fn depth_watch_shows_the_running_task_in_flight_and_clears_it_when_it_finishes() {
        let q_id = QueueId::new();
        let (a, b) = (ActionId::new(), ActionId::new());
        let (_dp, _bus, sched, held) = serial_queue_with_held(&[(a, true), (b, false)], q_id).await;
        let mut watch = sched.watch_depths();

        sched.dispatch(req(q_id, a)).await.unwrap();
        sched.dispatch(req(q_id, b)).await.unwrap();
        assert_eq!(
            settled_depth(&mut watch, q_id, depth(1, 1, 0)).await,
            Some(depth(1, 1, 0))
        );
        held.release_one();

        assert_eq!(
            settled_depth(&mut watch, q_id, depth(0, 0, 0)).await,
            Some(depth(0, 0, 0))
        );
        sched.shutdown();
    }

    #[tokio::test]
    async fn depth_watch_carries_the_overflow_count_and_zeroes_it_on_a_real_mode_change() {
        let (q_id, _bus, sched) = frozen_queue_filled_to_the_cap().await;
        let mut watch = sched.watch_depths();

        sched.dispatch(req(q_id, ActionId::new())).await.unwrap();
        assert_eq!(
            settled_depth(&mut watch, q_id, depth(MAX_PENDING_PER_QUEUE, 0, 1)).await,
            Some(depth(MAX_PENDING_PER_QUEUE, 0, 1))
        );

        sched.set_mode(q_id, QueueMode::PAUSED).await.unwrap();
        assert_eq!(
            settled_depth(&mut watch, q_id, depth(MAX_PENDING_PER_QUEUE, 0, 0)).await,
            Some(depth(MAX_PENDING_PER_QUEUE, 0, 0))
        );
        sched.shutdown();
    }

    #[tokio::test]
    async fn clearing_a_held_queue_publishes_an_empty_pending_depth() {
        let q_id = QueueId::new();
        let a_id = ActionId::new();
        let (_dp, sched) = held_log_queues(&[q_id], a_id).await;
        let mut watch = sched.watch_depths();
        for _ in 0..2 {
            sched.dispatch(req(q_id, a_id)).await.unwrap();
        }
        assert_eq!(
            settled_depth(&mut watch, q_id, depth(2, 0, 0)).await,
            Some(depth(2, 0, 0))
        );

        sched.clear(q_id, true).await.unwrap();

        assert_eq!(watch.current().get(&q_id), Some(&depth(0, 0, 0)));
        sched.shutdown();
    }

    #[tokio::test]
    async fn depth_watch_keeps_each_queue_on_its_own_counts() {
        let (first, second) = (QueueId::new(), QueueId::new());
        let a_id = ActionId::new();
        let (_dp, sched) = held_log_queues(&[first, second], a_id).await;
        let mut watch = sched.watch_depths();

        for _ in 0..2 {
            sched.dispatch(req(first, a_id)).await.unwrap();
        }
        sched.dispatch(req(second, a_id)).await.unwrap();

        let depths = await_depths(&mut watch, |depths| {
            depths.values().map(|depth| depth.pending).sum::<usize>() == 3
        })
        .await;
        assert_eq!(
            (
                depths.get(&first).map(|d| d.pending),
                depths.get(&second).map(|d| d.pending)
            ),
            (Some(2), Some(1))
        );
        sched.shutdown();
    }

    #[tokio::test]
    async fn a_registered_queue_joins_the_depths_and_a_deregistered_one_leaves_them() {
        let dp = make_dp().await;
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let sched = spawn_sched(&dp, &bus, Arc::new(SubActionRegistry::new()), vec![]);
        let q_id = QueueId::new();
        let mut watch = sched.watch_depths();

        sched.register(nonblocking(q_id)).await.unwrap();
        let joined = watch.current().get(&q_id).copied();
        sched.deregister(q_id).await.unwrap();
        let left = await_depths(&mut watch, |depths| !depths.contains_key(&q_id)).await;

        assert_eq!((joined, left.len()), (Some(depth(0, 0, 0)), 0));
        sched.shutdown();
    }

    fn task(admission: QueueAdmission) -> (ActionId, QueueTask) {
        let action_id = ActionId::new();
        let task = QueueTask {
            action_id,
            trigger_event_id: EventId::new(),
            trigger_kind: None,
            manual_test: false,
            origin: None,
            initial_args: ArgStack::new(),
            admission,
        };
        (action_id, task)
    }

    fn buffer_of(admissions: &[QueueAdmission]) -> (PendingBuffer, Vec<ActionId>) {
        let buffer = PendingBuffer::new(DepthBoard::new().register(QueueId::new()));
        let ids = admissions
            .iter()
            .map(|admission| {
                let (id, task) = task(*admission);
                assert!(buffer.push(task));
                id
            })
            .collect();
        (buffer, ids)
    }

    fn drain_lane(buffer: &PendingBuffer, lane: Lane, frozen: bool) -> Vec<ActionId> {
        std::iter::from_fn(|| buffer.take_next(lane, frozen))
            .map(|task| task.action_id)
            .collect()
    }

    const BYPASS_ALONGSIDE: QueueAdmission = QueueAdmission {
        bypass_pause: true,
        run_alongside: true,
    };

    #[test]
    fn each_lane_takes_its_own_tasks_in_arrival_order_past_the_other_lane() {
        let gated = QueueAdmission::default();
        let (buffer, ids) = buffer_of(&[ALONGSIDE, gated, ALONGSIDE, gated]);

        let taken = (
            drain_lane(&buffer, Lane::Gated, false),
            drain_lane(&buffer, Lane::Alongside, false),
        );

        assert_eq!(taken, (vec![ids[1], ids[3]], vec![ids[0], ids[2]]));
    }

    #[test]
    fn a_frozen_lane_takes_only_its_bypass_pause_tasks_and_leaves_the_rest() {
        let (buffer, ids) = buffer_of(&[ALONGSIDE, BYPASS_PAUSE, BYPASS_ALONGSIDE]);

        let taken = drain_lane(&buffer, Lane::Alongside, true);

        assert_eq!((taken, buffer.len()), (vec![ids[2]], 2));
    }

    #[test]
    fn a_mode_change_event_keeps_the_legacy_wire_shape_and_decodes_back() {
        for (mode, kind, processing, intake) in [
            (QueueMode::RUNNING, "queue.resumed", "running", "accept"),
            (QueueMode::DRAINING, "queue.draining", "running", "skip"),
            (QueueMode::HOLDING, "queue.held", "frozen", "accept"),
            (QueueMode::PAUSED, "queue.paused", "frozen", "skip"),
        ] {
            let changed = QueueModeChanged {
                queue_id: QueueId::new(),
                queue_name: "serial".to_owned(),
                mode,
            };

            let event = changed.clone().into_event();

            assert_eq!(
                (event.source, event.kind.as_str(), &event.payload),
                (
                    EventSource::Core,
                    kind,
                    &json!({
                        "queue_id": changed.queue_id.to_string(),
                        "queue_name": "serial",
                        "processing": processing,
                        "intake": intake,
                    })
                ),
                "{kind} wire"
            );
            assert_eq!(
                QueueModeChanged::from_event(&event),
                Some(changed),
                "{kind} round-trip"
            );
        }
    }

    #[test]
    fn a_mode_payload_under_a_non_mode_kind_does_not_decode() {
        let event = QueueModeChanged {
            queue_id: QueueId::new(),
            queue_name: "serial".to_owned(),
            mode: QueueMode::PAUSED,
        }
        .into_event();
        let cleared = Event::new(EventSource::Core, "queue.cleared", event.payload);

        assert_eq!(QueueModeChanged::from_event(&cleared), None);
    }

    #[test]
    fn a_mode_kind_with_an_unreadable_payload_does_not_decode() {
        let queue_id = QueueId::new().to_string();
        for payload in [
            json!({ "processing": "frozen", "intake": "skip" }),
            json!({ "queue_id": "not-a-queue-id", "processing": "frozen", "intake": "skip" }),
            json!({ "queue_id": queue_id, "processing": "Frozen", "intake": "skip" }),
            json!({ "queue_id": queue_id, "processing": "frozen", "intake": "hoard" }),
            json!({ "queue_id": queue_id, "intake": "skip" }),
            serde_json::Value::Null,
        ] {
            let event = Event::new(EventSource::Core, "queue.paused", payload.clone());
            assert_eq!(QueueModeChanged::from_event(&event), None, "{payload}");
        }
    }

    #[test]
    fn a_mode_payload_without_a_queue_name_decodes_with_an_empty_name() {
        let queue_id = QueueId::new();
        let event = Event::new(
            EventSource::Core,
            "queue.held",
            json!({ "queue_id": queue_id.to_string(), "processing": "frozen", "intake": "accept" }),
        );

        assert_eq!(
            QueueModeChanged::from_event(&event),
            Some(QueueModeChanged {
                queue_id,
                queue_name: String::new(),
                mode: QueueMode::HOLDING,
            })
        );
    }

    #[test]
    fn queue_mode_axes_serialize_lowercase_and_reject_other_casings() {
        for (mode, wire) in [
            (
                QueueMode::RUNNING,
                json!({ "processing": "running", "intake": "accept" }),
            ),
            (
                QueueMode::PAUSED,
                json!({ "processing": "frozen", "intake": "skip" }),
            ),
        ] {
            assert_eq!(
                serde_json::to_value(mode).ok(),
                Some(wire.clone()),
                "{mode:?}"
            );
            assert_eq!(
                serde_json::from_value::<QueueMode>(wire).ok(),
                Some(mode),
                "{mode:?}"
            );
        }
        for rejected in [json!("Running"), json!("FROZEN")] {
            assert!(
                serde_json::from_value::<QueueProcessing>(rejected.clone()).is_err(),
                "{rejected}"
            );
        }
        for rejected in [json!("Accept"), json!("SKIP")] {
            assert!(
                serde_json::from_value::<QueueIntake>(rejected.clone()).is_err(),
                "{rejected}"
            );
        }
    }

    #[tokio::test]
    async fn an_offer_reports_the_reason_a_queue_refuses_the_run() {
        for mode in [QueueMode::PAUSED, QueueMode::HOLDING] {
            let (q_id, _bus, sched) = frozen_queue_filled_to_the_cap().await;
            let expected = if mode == QueueMode::PAUSED {
                QUEUE_PAUSED_REASON
            } else {
                QUEUE_OVERFLOW_REASON
            };
            sched.set_mode(q_id, mode).await.unwrap();

            let offered = sched.offer(req(q_id, ActionId::new())).await;

            assert!(
                matches!(offered, Err(SchedulerError::Refused(reason)) if reason == expected),
                "mode {mode:?}: got {offered:?}"
            );
            sched.shutdown();
        }
    }

    #[tokio::test]
    async fn an_offer_to_a_queue_the_scheduler_does_not_hold_is_refused_as_not_found() {
        let (_q_id, _bus, sched) = frozen_queue_filled_to_the_cap().await;

        let offered = sched.offer(req(QueueId::new(), ActionId::new())).await;

        assert!(
            matches!(
                offered,
                Err(SchedulerError::Refused(QUEUE_NOT_FOUND_REASON))
            ),
            "got {offered:?}"
        );
        sched.shutdown();
    }

    #[tokio::test]
    async fn an_offer_a_holding_queue_buffers_is_accepted_up_to_the_cap() {
        let dp = make_dp().await;
        let q_id = QueueId::new();
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let sched = spawn_sched(
            &dp,
            &bus,
            Arc::new(SubActionRegistry::new()),
            vec![nonblocking(q_id)],
        );
        sched.set_mode(q_id, QueueMode::HOLDING).await.unwrap();

        let offered = sched.offer(req(q_id, ActionId::new())).await;

        assert!(offered.is_ok(), "got {offered:?}");
        let state = sched.queue_states().await.unwrap().remove(&q_id).unwrap();
        assert_eq!(state.pending, 1);
        sched.shutdown();
    }

    fn tracked(id: QueueId) -> (crate::queue_depth::DepthBoard, InflightTracker) {
        let board = crate::queue_depth::DepthBoard::new();
        let tracker = InflightTracker::new(board.register(id));
        (board, tracker)
    }

    fn running_ids(board: &crate::queue_depth::DepthBoard, id: QueueId) -> Vec<ActionId> {
        board
            .watch()
            .running()
            .get(&id)
            .map(|runs| runs.iter().map(|run| run.action_id).collect())
            .unwrap_or_default()
    }

    #[test]
    fn the_running_list_follows_launch_order_and_drops_each_finished_run() {
        let id = QueueId::new();
        let (board, tracker) = tracked(id);
        let (first, second, third) = (ActionId::new(), ActionId::new(), ActionId::new());

        tracker.register(CancelSignal::new(), first);
        let middle = tracker.register(CancelSignal::new(), second);
        tracker.register(CancelSignal::new(), third);
        assert_eq!(running_ids(&board, id), vec![first, second, third]);

        tracker.complete(middle);

        assert_eq!(running_ids(&board, id), vec![first, third]);
    }

    #[test]
    fn cancelling_every_run_signals_each_of_them_and_the_last_completion_clears_the_list() {
        let id = QueueId::new();
        let (board, tracker) = tracked(id);
        let signals = [CancelSignal::new(), CancelSignal::new()];
        let run = tracker.register(signals[0].clone(), ActionId::new());
        tracker.register(signals[1].clone(), ActionId::new());

        tracker.cancel_all();
        tracker.complete(run);

        assert!(signals.iter().all(CancelSignal::is_cancelled));
        assert_eq!(running_ids(&board, id), Vec::new());
    }
}
