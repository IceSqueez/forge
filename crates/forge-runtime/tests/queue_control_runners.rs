#![allow(clippy::unwrap_used)]
mod common;
use common::{code_of, detail_of, failure_message};
use std::sync::Arc;
use std::time::Duration;

use forge_events::EventSubscription;
use forge_events::{Event, EventPublisher};
use forge_registry::{RunContext, SubActionRunner};
use forge_runtime::{EventBus, FrozenRuntime, NullEventLogRepo, QueueMode};
use forge_storage::DataProvider;
use forge_storage_sqlite::SqliteBackend;
use forge_types::{ArgStack, EventId, Queue, QueueId, SubActionConfig, SubActionOutcome, Variant};
use tempfile::TempDir;

const PAUSE: &str = "core.queue.pause";
const RESUME: &str = "core.queue.resume";
const CLEAR: &str = "core.queue.clear";

struct NullPublisher;
impl EventPublisher for NullPublisher {
    fn publish(&self, _event: Event) {}
}

async fn make_dp() -> (Arc<dyn DataProvider>, TempDir) {
    let media = tempfile::tempdir().unwrap();
    let backend =
        SqliteBackend::open_for_test(":memory:", [0xcd; 32], media.path().join("media"), None)
            .await
            .unwrap();
    (Arc::new(backend), media)
}

fn nonblocking(id: QueueId) -> Queue {
    Queue {
        id,
        name: "default".to_string(),
        description: String::new(),
        concurrency: 8,
    }
}

struct Rig {
    runtime: FrozenRuntime,
    bus: Arc<EventBus>,
    _media: TempDir,
}

impl Rig {
    fn step(&self, kind_id: &str) -> &dyn SubActionRunner {
        self.runtime.sub_action_registry.get(kind_id).unwrap()
    }
}

async fn live_runtime(queue: Queue) -> Rig {
    let (dp, media) = make_dp().await;
    dp.queue_repo().save(&queue).await.unwrap();
    let bus = EventBus::new(Arc::new(NullEventLogRepo));
    let runtime = common::frozen_runtime(common::wiring_parts(dp, Arc::clone(&bus)).await).await;
    Rig {
        runtime,
        bus,
        _media: media,
    }
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
        match tokio::time::timeout(Duration::from_secs(30), sub.recv()).await {
            Ok(Ok(ev))
                if ev.kind == kind
                    && ev.payload.get("queue_id").and_then(|v| v.as_str())
                        == Some(target.as_str()) =>
            {
                return true;
            }
            Ok(Ok(_)) => {}
            _ => return false,
        }
    }
    false
}

#[tokio::test]
async fn pause_runner_flips_registered_queue_to_paused() {
    let q_id = QueueId::new();
    let rig = live_runtime(nonblocking(q_id)).await;
    let handle = &rig.runtime.scheduler;
    let runner = rig.step(PAUSE);

    let outcome = run_outcome(runner, &cfg_queue(&q_id.to_string())).await;
    assert!(matches!(outcome, SubActionOutcome::Success));

    let states = handle.queue_states().await.unwrap();
    assert_eq!(
        states.get(&q_id).map(|s| s.mode),
        Some(QueueMode::PAUSED),
        "pause runner must flip the queue into Paused via the live scheduler"
    );
}

#[tokio::test]
async fn pause_runner_with_unparseable_queue_id_fails() {
    let rig = live_runtime(nonblocking(QueueId::new())).await;
    let runner = rig.step(PAUSE);

    let outcome = run_outcome(runner, &cfg_queue("not-a-queue-id")).await;
    assert!(
        matches!(&outcome, SubActionOutcome::Failed(m) if common::is_field_problem(m, "queue_id", "config.unknown_entry")),
        "unparseable queue_id must fail, got {outcome:?}"
    );
}

#[tokio::test]
async fn pause_runner_with_unregistered_queue_propagates_not_found() {
    let rig = live_runtime(nonblocking(QueueId::new())).await;
    let runner = rig.step(PAUSE);

    let outcome = run_outcome(runner, &cfg_queue(&QueueId::new().to_string())).await;
    let refusal = failure_message(&outcome);
    assert_eq!(code_of(refusal), "core.scheduler_failed");
    assert!(detail_of(refusal).is_some_and(|d| d.contains("queue not found")));
}

#[tokio::test]
async fn resume_runner_clears_paused_state_on_registered_queue() {
    let q_id = QueueId::new();
    let rig = live_runtime(nonblocking(q_id)).await;
    let handle = &rig.runtime.scheduler;
    handle.set_mode(q_id, QueueMode::PAUSED).await.unwrap();
    let runner = rig.step(RESUME);

    let outcome = run_outcome(runner, &cfg_queue(&q_id.to_string())).await;
    assert!(matches!(outcome, SubActionOutcome::Success));

    let states = handle.queue_states().await.unwrap();
    assert_eq!(
        states.get(&q_id).map(|s| s.mode),
        Some(QueueMode::RUNNING),
        "resume runner must return the queue to Running via the live scheduler"
    );
}

#[tokio::test]
async fn resume_runner_with_unregistered_queue_propagates_not_found() {
    let rig = live_runtime(nonblocking(QueueId::new())).await;
    let runner = rig.step(RESUME);

    let outcome = run_outcome(runner, &cfg_queue(&QueueId::new().to_string())).await;
    let refusal = failure_message(&outcome);
    assert_eq!(code_of(refusal), "core.scheduler_failed");
    assert!(detail_of(refusal).is_some_and(|d| d.contains("queue not found")));
}

#[tokio::test]
async fn clear_runner_succeeds_and_emits_cleared_event() {
    let q_id = QueueId::new();
    let rig = live_runtime(nonblocking(q_id)).await;
    let mut sub = rig.bus.subscribe();
    let runner = rig.step(CLEAR);

    let outcome = run_outcome(runner, &cfg_queue(&q_id.to_string())).await;
    assert!(matches!(outcome, SubActionOutcome::Success));
    assert!(
        await_queue_event(&mut sub, "queue.cleared", q_id, 10_000).await,
        "clear runner must drive a queue.cleared event on the live scheduler"
    );
}

#[tokio::test]
async fn clear_runner_with_unregistered_queue_propagates_not_found() {
    let rig = live_runtime(nonblocking(QueueId::new())).await;
    let runner = rig.step(CLEAR);

    let outcome = run_outcome(runner, &cfg_queue(&QueueId::new().to_string())).await;
    let refusal = failure_message(&outcome);
    assert_eq!(code_of(refusal), "core.scheduler_failed");
    assert!(detail_of(refusal).is_some_and(|d| d.contains("queue not found")));
}
