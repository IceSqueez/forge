#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use forge_events::Event;
use forge_registry::{
    FormField, RegistryError, RunContext, StepTimer, SubActionCategory, SubActionRegistry,
    SubActionRunner,
};
use forge_runtime::{
    ActionCancelRegistry, EventBus, EventSubscription, MAX_PENDING_PER_QUEUE, NullEventLogRepo,
    QueueMode, QueueRuntimeState, QueueScheduler, QueueSchedulerHandle, SchedulerRequest,
    spawn_action_engine,
};
use forge_storage::{
    ActionRepo, ActionStats, ActionTelemetry, ExecutionStatus, HistoryRepo, StorageError,
};
use forge_types::{
    Action, ActionId, ArgStack, EventId, ExecutionContext, ExecutionMode, Queue, QueueId,
    SubActionConfig, SubActionStep, SubActionTelemetry,
};
use time::OffsetDateTime;
use tokio::sync::watch;

const GATE_KIND_ID: &str = "test.gate";
const SETTLE_ROUNDS: usize = 200;
const QUIET_STEP: Duration = Duration::from_secs(5);
const QUIET_ROUNDS: usize = 4;

/// Keeps the whole fixture inside the process: a sqlite-backed repo waits on a pool
/// timeout, which the paused clock fires the instant the runtime looks idle.
#[derive(Default)]
struct MemoryActionRepo {
    actions: Mutex<HashMap<ActionId, Action>>,
}

#[async_trait]
impl ActionRepo for MemoryActionRepo {
    async fn list(&self) -> Result<Vec<Action>, StorageError> {
        Ok(self.actions.lock().unwrap().values().cloned().collect())
    }
    async fn get(&self, id: ActionId) -> Result<Option<Action>, StorageError> {
        Ok(self.actions.lock().unwrap().get(&id).cloned())
    }
    async fn save(&self, action: &Action) -> Result<(), StorageError> {
        self.actions
            .lock()
            .unwrap()
            .insert(action.id, action.clone());
        Ok(())
    }
    async fn delete(&self, id: ActionId) -> Result<bool, StorageError> {
        Ok(self.actions.lock().unwrap().remove(&id).is_some())
    }
    async fn list_by_group<'a>(
        &'a self,
        _group: Option<&'a str>,
    ) -> Result<Vec<Action>, StorageError> {
        Ok(Vec::new())
    }
    async fn telemetry(&self, _id: ActionId) -> Result<ActionTelemetry, StorageError> {
        Ok(ActionTelemetry::default())
    }
    async fn record_execution(
        &self,
        _action_id: ActionId,
        _started_at: OffsetDateTime,
        _duration_ms: u64,
        _status: ExecutionStatus,
    ) -> Result<(), StorageError> {
        Ok(())
    }
    async fn prune_executions_before(&self, _cutoff: OffsetDateTime) -> Result<u64, StorageError> {
        Ok(0)
    }
}

struct NullHistoryRepo;

#[async_trait]
impl HistoryRepo for NullHistoryRepo {
    async fn save(&self, _ctx: &ExecutionContext) -> Result<(), StorageError> {
        Ok(())
    }
    async fn recent_for_action(
        &self,
        _action_id: ActionId,
        _limit: u32,
    ) -> Result<Vec<ExecutionContext>, StorageError> {
        Ok(Vec::new())
    }
    async fn recent_for_builtin(
        &self,
        _builtin_id: &str,
        _limit: u32,
    ) -> Result<Vec<ExecutionContext>, StorageError> {
        Ok(Vec::new())
    }
    async fn stats_summary(
        &self,
        _since: OffsetDateTime,
    ) -> Result<HashMap<ActionId, ActionStats>, StorageError> {
        Ok(HashMap::new())
    }
    async fn prune_before(&self, _cutoff: OffsetDateTime) -> Result<u64, StorageError> {
        Ok(0)
    }
}

/// Holds executions inside the engine until the test opens it, so a queue can be
/// inspected with work genuinely in flight and without a wall-clock sleep.
struct Gate {
    open: watch::Sender<bool>,
}

impl Gate {
    fn shut() -> (Self, watch::Receiver<bool>) {
        let (tx, rx) = watch::channel(false);
        (Self { open: tx }, rx)
    }

    fn open(&self) {
        self.open.send_replace(true);
    }
}

struct GateRunner {
    open: watch::Receiver<bool>,
}

#[async_trait]
impl SubActionRunner for GateRunner {
    fn id(&self) -> &str {
        GATE_KIND_ID
    }
    fn category(&self) -> SubActionCategory {
        SubActionCategory::Delay
    }
    fn label(&self) -> &str {
        "Gate"
    }
    fn summary(&self) -> &str {
        "Blocks the execution until the test opens the gate"
    }
    fn search_text(&self) -> &str {
        "gate"
    }
    fn icon_name(&self) -> &str {
        "clock-pause"
    }
    fn default_config(&self) -> SubActionConfig {
        SubActionConfig::new()
    }
    fn config_fields(&self) -> Vec<FormField> {
        Vec::new()
    }
    fn validate_config(&self, _config: &SubActionConfig) -> Result<(), RegistryError> {
        Ok(())
    }
    async fn execute(
        &self,
        _config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, GATE_KIND_ID);
        let mut open = self.open.clone();
        while !*open.borrow_and_update() {
            if open.changed().await.is_err() {
                break;
            }
        }
        (timer.success(), None)
    }
}

struct GatedQueue {
    bus: Arc<EventBus>,
    sched: QueueSchedulerHandle,
    gate: Gate,
    q_id: QueueId,
}

fn queue_with(id: QueueId, concurrency: u32) -> Queue {
    Queue {
        id,
        name: "gated".to_string(),
        description: String::new(),
        concurrency,
    }
}

fn gated_action(id: ActionId, queue_id: QueueId) -> Action {
    Action {
        id,
        name: "gated".to_string(),
        group: None,
        queue_id,
        enabled: true,
        concurrent: false,
        bypass_pause: false,
        execution_mode: ExecutionMode::Sequential,
        description: None,
        sub_actions: vec![SubActionStep {
            kind_id: GATE_KIND_ID.to_owned(),
            config: BTreeMap::new(),
            enabled: true,
            continue_on_error: false,
            condition: None,
            label: None,
        }],
    }
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

async fn gated_queue(concurrency: u32, actions: &[ActionId]) -> GatedQueue {
    let q_id = QueueId::new();
    let repo = MemoryActionRepo::default();
    for id in actions {
        repo.save(&gated_action(*id, q_id)).await.unwrap();
    }

    let (gate, open) = Gate::shut();
    let mut registry = SubActionRegistry::new();
    registry.register(Box::new(GateRunner { open })).unwrap();

    let bus = EventBus::new(Arc::new(NullEventLogRepo));
    let engine = spawn_action_engine(
        Arc::clone(&bus),
        Arc::new(repo),
        Arc::new(NullHistoryRepo),
        Arc::new(registry),
        Arc::new(ActionCancelRegistry::new()),
    );
    let sched = QueueScheduler::spawn(
        engine,
        Arc::clone(&bus),
        vec![queue_with(q_id, concurrency)],
    );
    GatedQueue {
        bus,
        sched,
        gate,
        q_id,
    }
}

fn action_id_of(ev: &Event) -> Option<String> {
    ev.payload
        .get("action_id")
        .and_then(|v| v.as_str())
        .map(str::to_owned)
}

fn ids_of(events: &[Event], kind: &str) -> Vec<String> {
    events
        .iter()
        .filter(|ev| ev.kind == kind)
        .filter_map(action_id_of)
        .collect()
}

fn start_of(action_id: ActionId) -> impl FnMut(&Event) -> bool {
    let target = action_id.to_string();
    move |ev| ev.kind == "action.start" && action_id_of(ev).as_deref() == Some(target.as_str())
}

fn done_of(action_id: ActionId) -> impl FnMut(&Event) -> bool {
    let target = action_id.to_string();
    move |ev| ev.kind == "action.done" && action_id_of(ev).as_deref() == Some(target.as_str())
}

/// Re-reads the queue state until `want` holds; the query round-trips the single-threaded
/// scheduler loop, which is the barrier, so no wall-clock sleep is involved.
async fn settle(
    sched: &QueueSchedulerHandle,
    q_id: QueueId,
    what: &str,
    want: impl Fn(&QueueRuntimeState) -> bool,
) -> QueueRuntimeState {
    let mut last = None;
    for _ in 0..SETTLE_ROUNDS {
        let state = state_of(sched, q_id).await;
        if want(&state) {
            return state;
        }
        last = Some(state);
        tokio::task::yield_now().await;
    }
    panic!("the queue never settled to {what}, last saw {last:?}");
}

async fn state_of(sched: &QueueSchedulerHandle, q_id: QueueId) -> QueueRuntimeState {
    sched
        .queue_states()
        .await
        .unwrap()
        .remove(&q_id)
        .expect("the queue stays registered")
}

/// Reads bus events until `stop` matches, returning everything seen including it.
///
/// Why: every party here is a task on the paused runtime, so an elapsed step means the
/// system is quiescent and the awaited event is never coming - and it costs no wall time,
/// so a stalled queue fails the test immediately instead of hanging.
async fn events_until(
    sub: &mut EventSubscription,
    mut stop: impl FnMut(&Event) -> bool,
) -> Vec<Event> {
    let mut seen: Vec<Event> = Vec::new();
    let mut quiet = 0;
    loop {
        match tokio::time::timeout(QUIET_STEP, sub.recv()).await {
            Ok(Ok(ev)) => {
                quiet = 0;
                let matched = stop(&ev);
                seen.push(ev);
                if matched {
                    return seen;
                }
            }
            Ok(Err(e)) => panic!("the bus closed before the awaited event: {e}"),
            Err(_) => {
                quiet += 1;
                assert!(
                    quiet < QUIET_ROUNDS,
                    "the bus went quiet before the awaited event, saw {:?}",
                    seen.iter().map(|ev| ev.kind.as_str()).collect::<Vec<_>>()
                );
                tokio::task::yield_now().await;
            }
        }
    }
}

#[tokio::test(start_paused = true)]
async fn a_concurrency_change_carries_the_held_buffer_and_replays_it_in_order() {
    let (a, b, c) = (ActionId::new(), ActionId::new(), ActionId::new());
    let q = gated_queue(4, &[a, b, c]).await;
    q.gate.open();
    let mut sub = q.bus.subscribe();

    q.sched.set_mode(q.q_id, QueueMode::HOLDING).await.unwrap();
    for id in [a, b, c] {
        q.sched.dispatch(req(q.q_id, id)).await.unwrap();
    }
    settle(&q.sched, q.q_id, "three held tasks", |s| s.pending == 3).await;

    q.sched.reconfigure(queue_with(q.q_id, 1)).await.unwrap();

    let swapped = state_of(&q.sched, q.q_id).await;
    assert_eq!(
        swapped.pending, 3,
        "a concurrency change must carry every held task into the new runner"
    );
    assert_eq!(
        swapped.in_flight, 0,
        "a held queue must run nothing while its runner is swapped"
    );

    q.sched.set_mode(q.q_id, QueueMode::RUNNING).await.unwrap();
    let seen = events_until(&mut sub, done_of(c)).await;
    assert_eq!(
        ids_of(&seen, "action.start"),
        vec![a.to_string(), b.to_string(), c.to_string()],
        "the carried buffer must replay in enqueue order"
    );
    q.sched.shutdown();
}

#[tokio::test(start_paused = true)]
async fn a_concurrency_change_carries_tasks_queued_behind_a_full_semaphore() {
    let (g1, g2, b, c) = (
        ActionId::new(),
        ActionId::new(),
        ActionId::new(),
        ActionId::new(),
    );
    let q = gated_queue(2, &[g1, g2, b, c]).await;
    let mut sub = q.bus.subscribe();

    for id in [g1, g2] {
        q.sched.dispatch(req(q.q_id, id)).await.unwrap();
    }
    settle(&q.sched, q.q_id, "both permits taken", |s| s.in_flight == 2).await;
    for id in [b, c] {
        q.sched.dispatch(req(q.q_id, id)).await.unwrap();
    }
    settle(&q.sched, q.q_id, "two tasks behind the semaphore", |s| {
        s.pending == 2
    })
    .await;

    q.sched.reconfigure(queue_with(q.q_id, 1)).await.unwrap();
    q.gate.open();

    let seen = events_until(&mut sub, done_of(c)).await;
    let (b_id, c_id) = (b.to_string(), c.to_string());
    let carried: Vec<String> = ids_of(&seen, "action.start")
        .into_iter()
        .filter(|id| *id == b_id || *id == c_id)
        .collect();
    assert_eq!(
        carried,
        vec![b_id, c_id],
        "tasks waiting on the old semaphore must survive the swap in enqueue order"
    );
    q.sched.shutdown();
}

#[tokio::test(start_paused = true)]
async fn a_concurrency_value_that_clamps_to_the_current_one_leaves_the_slot_alone() {
    let (running, filler) = (ActionId::new(), ActionId::new());
    let q = gated_queue(1, &[running, filler]).await;
    let mut sub = q.bus.subscribe();

    q.sched.dispatch(req(q.q_id, running)).await.unwrap();
    events_until(&mut sub, start_of(running)).await;
    q.sched.set_mode(q.q_id, QueueMode::HOLDING).await.unwrap();
    for _ in 0..MAX_PENDING_PER_QUEUE {
        q.sched.dispatch(req(q.q_id, filler)).await.unwrap();
    }
    q.sched.dispatch(req(q.q_id, filler)).await.unwrap();

    q.sched.reconfigure(queue_with(q.q_id, 0)).await.unwrap();

    let state = state_of(&q.sched, q.q_id).await;
    assert_eq!(
        state.pending, MAX_PENDING_PER_QUEUE,
        "a concurrency that clamps to the current one must not touch the buffer"
    );
    assert_eq!(
        state.overflowed, 1,
        "a clamped no-op must not reset the overflow counter"
    );
    assert_eq!(
        state.mode,
        QueueMode::HOLDING,
        "a clamped no-op must not reset the mode"
    );
    assert_eq!(
        state.in_flight, 1,
        "a clamped no-op must not retire the runner holding the permit"
    );
    q.sched.shutdown();
}

#[tokio::test(start_paused = true)]
async fn a_task_dispatched_after_a_concurrency_change_runs_behind_the_carried_ones() {
    let (a, b, d) = (ActionId::new(), ActionId::new(), ActionId::new());
    let q = gated_queue(4, &[a, b, d]).await;
    q.gate.open();
    let mut sub = q.bus.subscribe();

    q.sched.set_mode(q.q_id, QueueMode::HOLDING).await.unwrap();
    for id in [a, b] {
        q.sched.dispatch(req(q.q_id, id)).await.unwrap();
    }
    settle(&q.sched, q.q_id, "two held tasks", |s| s.pending == 2).await;

    q.sched.reconfigure(queue_with(q.q_id, 1)).await.unwrap();
    q.sched.dispatch(req(q.q_id, d)).await.unwrap();

    q.sched.set_mode(q.q_id, QueueMode::RUNNING).await.unwrap();
    let seen = events_until(&mut sub, done_of(d)).await;
    assert_eq!(
        ids_of(&seen, "action.start"),
        vec![a.to_string(), b.to_string(), d.to_string()],
        "a dispatch after the swap must queue behind everything carried"
    );
    q.sched.shutdown();
}

#[tokio::test(start_paused = true)]
async fn clearing_with_keep_current_leaves_the_running_execution_cancellable() {
    let (a, d) = (ActionId::new(), ActionId::new());
    let q = gated_queue(1, &[a, d]).await;
    let mut sub = q.bus.subscribe();

    q.sched.dispatch(req(q.q_id, a)).await.unwrap();
    events_until(&mut sub, start_of(a)).await;

    q.sched.clear(q.q_id, true).await.unwrap();
    let kept = state_of(&q.sched, q.q_id).await;
    assert_eq!(
        kept.in_flight, 1,
        "keep_current must leave the running execution registered on the slot"
    );

    q.sched.clear(q.q_id, false).await.unwrap();
    q.gate.open();
    settle(&q.sched, q.q_id, "an idle queue", |s| s.in_flight == 0).await;

    q.sched.dispatch(req(q.q_id, d)).await.unwrap();
    let seen = events_until(&mut sub, done_of(d)).await;
    assert!(
        !ids_of(&seen, "action.done").contains(&a.to_string()),
        "the execution the first clear kept must be the one the second clear cancels"
    );
    q.sched.shutdown();
}

#[tokio::test(start_paused = true)]
async fn a_bypass_dispatch_jumps_a_held_buffer_without_disturbing_its_order() {
    let (a, b, z) = (ActionId::new(), ActionId::new(), ActionId::new());
    let q = gated_queue(1, &[a, b, z]).await;
    q.gate.open();
    let mut sub = q.bus.subscribe();

    q.sched.set_mode(q.q_id, QueueMode::HOLDING).await.unwrap();
    for id in [a, b] {
        q.sched.dispatch(req(q.q_id, id)).await.unwrap();
    }
    settle(&q.sched, q.q_id, "two held tasks", |s| s.pending == 2).await;

    q.sched
        .dispatch(SchedulerRequest {
            bypass_pause: true,
            ..req(q.q_id, z)
        })
        .await
        .unwrap();

    let jumped = events_until(&mut sub, done_of(z)).await;
    assert_eq!(
        ids_of(&jumped, "action.start"),
        vec![z.to_string()],
        "only the bypassing task may run while the queue is held"
    );

    q.sched.set_mode(q.q_id, QueueMode::RUNNING).await.unwrap();
    let resumed = events_until(&mut sub, done_of(b)).await;
    assert_eq!(
        ids_of(&resumed, "action.start"),
        vec![a.to_string(), b.to_string()],
        "the tasks the bypass jumped must keep their arrival order"
    );
    q.sched.shutdown();
}

#[tokio::test(start_paused = true)]
async fn the_dispatch_that_fills_the_last_free_slot_is_still_buffered() {
    let a = ActionId::new();
    let q = gated_queue(1, &[a]).await;
    q.sched.set_mode(q.q_id, QueueMode::HOLDING).await.unwrap();
    for _ in 0..MAX_PENDING_PER_QUEUE - 1 {
        q.sched.dispatch(req(q.q_id, a)).await.unwrap();
    }

    q.sched.dispatch(req(q.q_id, a)).await.unwrap();

    let at_cap = state_of(&q.sched, q.q_id).await;
    assert_eq!(
        at_cap.pending, MAX_PENDING_PER_QUEUE,
        "the dispatch filling the last free slot must be buffered, not skipped"
    );
    assert_eq!(
        at_cap.overflowed, 0,
        "filling the last free slot must not count as an overflow"
    );
    q.sched.shutdown();
}
