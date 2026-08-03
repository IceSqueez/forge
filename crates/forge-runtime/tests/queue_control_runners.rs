#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;
use std::time::Duration;

use forge_events::{Event, EventPublisher};
use forge_registry::{RunContext, SubActionRegistry, SubActionRunner};
use forge_runtime::sub_action_runners::{
    CoreQueueClearRunner, CoreQueuePauseRunner, CoreQueueResumeRunner,
};
use forge_runtime::{
    EventBus, EventSubscription, NullEventLogRepo, QueueMode, QueueScheduler, QueueSchedulerHandle,
    SchedulerCell, spawn_action_engine,
};
use forge_storage::DataProvider;
use forge_storage_sqlite::SqliteBackend;
use forge_types::{ArgStack, EventId, Queue, QueueId, SubActionConfig, SubActionOutcome, Variant};

struct NullPublisher;
impl EventPublisher for NullPublisher {
    fn publish(&self, _event: Event) {}
}

async fn make_dp() -> Arc<dyn DataProvider> {
    Arc::new(
        SqliteBackend::open_with_key(":memory:", [0xcd; 32])
            .await
            .unwrap(),
    )
}

fn nonblocking(id: QueueId) -> Queue {
    Queue {
        id,
        name: "default".to_string(),
        description: String::new(),
        concurrency: 8,
    }
}

async fn live_scheduler(queue: Queue) -> (SchedulerCell, QueueSchedulerHandle, Arc<EventBus>) {
    let dp = make_dp().await;
    dp.queue_repo().save(&queue).await.unwrap();

    let bus = EventBus::new(Arc::new(NullEventLogRepo));
    let engine = spawn_action_engine(
        Arc::clone(&bus),
        dp.action_repo(),
        dp.history_repo(),
        Arc::new(SubActionRegistry::new()),
        Arc::new(forge_runtime::ActionCancelRegistry::new()),
    );
    let handle = QueueScheduler::spawn(engine, Arc::clone(&bus), vec![queue]);

    let cell = SchedulerCell::new();
    cell.set(handle.clone());
    (cell, handle, bus)
}

fn cfg_queue(queue_id: &str) -> SubActionConfig {
    let mut c = SubActionConfig::new();
    c.insert("queue_id".to_owned(), Variant::String(queue_id.to_owned()));
    c
}

async fn run_outcome(runner: &dyn SubActionRunner, config: &SubActionConfig) -> SubActionOutcome {
    let stack = ArgStack::new();
    let ctx = RunContext::leaf(&stack, 0, EventId::new(), &NullPublisher);
    let (telemetry, _) = runner.execute(config, &ctx).await;
    telemetry.outcome
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
async fn pause_runner_flips_registered_queue_to_paused() {
    let q_id = QueueId::new();
    let (cell, handle, _bus) = live_scheduler(nonblocking(q_id)).await;
    let runner = CoreQueuePauseRunner::new(cell);

    let outcome = run_outcome(&runner, &cfg_queue(&q_id.to_string())).await;
    assert!(matches!(outcome, SubActionOutcome::Success));

    let states = handle.queue_states().await.unwrap();
    assert_eq!(
        states.get(&q_id).map(|s| s.mode),
        Some(QueueMode::PAUSED),
        "pause runner must flip the queue into Paused via the live scheduler"
    );
    handle.shutdown();
}

#[tokio::test]
async fn pause_runner_with_empty_cell_reports_scheduler_not_ready() {
    let runner = CoreQueuePauseRunner::new(SchedulerCell::new());
    let outcome = run_outcome(&runner, &cfg_queue(&QueueId::new().to_string())).await;
    assert!(
        matches!(&outcome, SubActionOutcome::Failed(m) if m.contains("queue scheduler not ready")),
        "empty cell must fail with the not-ready message, got {outcome:?}"
    );
}

#[tokio::test]
async fn pause_runner_with_unparseable_queue_id_fails() {
    let (cell, handle, _bus) = live_scheduler(nonblocking(QueueId::new())).await;
    let runner = CoreQueuePauseRunner::new(cell);

    let outcome = run_outcome(&runner, &cfg_queue("not-a-queue-id")).await;
    assert!(
        matches!(&outcome, SubActionOutcome::Failed(m) if m.contains("invalid queue_id")),
        "unparseable queue_id must fail, got {outcome:?}"
    );
    handle.shutdown();
}

#[tokio::test]
async fn pause_runner_with_unregistered_queue_propagates_not_found() {
    let (cell, handle, _bus) = live_scheduler(nonblocking(QueueId::new())).await;
    let runner = CoreQueuePauseRunner::new(cell);

    let outcome = run_outcome(&runner, &cfg_queue(&QueueId::new().to_string())).await;
    assert!(
        matches!(&outcome, SubActionOutcome::Failed(m) if m.contains("queue not found")),
        "unregistered queue must surface QueueNotFound, got {outcome:?}"
    );
    handle.shutdown();
}

#[tokio::test]
async fn resume_runner_clears_paused_state_on_registered_queue() {
    let q_id = QueueId::new();
    let (cell, handle, _bus) = live_scheduler(nonblocking(q_id)).await;
    handle.set_mode(q_id, QueueMode::PAUSED).await.unwrap();
    let runner = CoreQueueResumeRunner::new(cell);

    let outcome = run_outcome(&runner, &cfg_queue(&q_id.to_string())).await;
    assert!(matches!(outcome, SubActionOutcome::Success));

    let states = handle.queue_states().await.unwrap();
    assert_eq!(
        states.get(&q_id).map(|s| s.mode),
        Some(QueueMode::RUNNING),
        "resume runner must return the queue to Running via the live scheduler"
    );
    handle.shutdown();
}

#[tokio::test]
async fn resume_runner_with_empty_cell_reports_scheduler_not_ready() {
    let runner = CoreQueueResumeRunner::new(SchedulerCell::new());
    let outcome = run_outcome(&runner, &cfg_queue(&QueueId::new().to_string())).await;
    assert!(
        matches!(&outcome, SubActionOutcome::Failed(m) if m.contains("queue scheduler not ready")),
        "empty cell must fail with the not-ready message, got {outcome:?}"
    );
}

#[tokio::test]
async fn resume_runner_with_unregistered_queue_propagates_not_found() {
    let (cell, handle, _bus) = live_scheduler(nonblocking(QueueId::new())).await;
    let runner = CoreQueueResumeRunner::new(cell);

    let outcome = run_outcome(&runner, &cfg_queue(&QueueId::new().to_string())).await;
    assert!(
        matches!(&outcome, SubActionOutcome::Failed(m) if m.contains("queue not found")),
        "unregistered queue must surface QueueNotFound, got {outcome:?}"
    );
    handle.shutdown();
}

#[tokio::test]
async fn clear_runner_succeeds_and_emits_cleared_event() {
    let q_id = QueueId::new();
    let (cell, handle, bus) = live_scheduler(nonblocking(q_id)).await;
    let mut sub = bus.subscribe();
    let runner = CoreQueueClearRunner::new(cell);

    let outcome = run_outcome(&runner, &cfg_queue(&q_id.to_string())).await;
    assert!(matches!(outcome, SubActionOutcome::Success));
    assert!(
        await_queue_event(&mut sub, "queue.cleared", q_id, 20).await,
        "clear runner must drive a queue.cleared event on the live scheduler"
    );
    handle.shutdown();
}

#[tokio::test]
async fn clear_runner_with_empty_cell_reports_scheduler_not_ready() {
    let runner = CoreQueueClearRunner::new(SchedulerCell::new());
    let outcome = run_outcome(&runner, &cfg_queue(&QueueId::new().to_string())).await;
    assert!(
        matches!(&outcome, SubActionOutcome::Failed(m) if m.contains("queue scheduler not ready")),
        "empty cell must fail with the not-ready message, got {outcome:?}"
    );
}

#[tokio::test]
async fn clear_runner_with_unregistered_queue_propagates_not_found() {
    let (cell, handle, _bus) = live_scheduler(nonblocking(QueueId::new())).await;
    let runner = CoreQueueClearRunner::new(cell);

    let outcome = run_outcome(&runner, &cfg_queue(&QueueId::new().to_string())).await;
    assert!(
        matches!(&outcome, SubActionOutcome::Failed(m) if m.contains("queue not found")),
        "unregistered queue must surface QueueNotFound, got {outcome:?}"
    );
    handle.shutdown();
}
