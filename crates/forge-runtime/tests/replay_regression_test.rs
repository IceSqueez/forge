//! Regression tests for EventBus::replay_and_publish correctness.
//!
//! Covered invariants:
//! - The replayed root event has a fresh `EventId` and `replay: true`.
//! - Downstream pipeline events driven by a replayed trigger have IDs that are
//!   entirely distinct from the original run's IDs.
//! - The causation chain of downstream replay events is rooted at the replayed
//!   root event — not at the original event that was replayed.
//! - Replay-of-a-replay: replaying the result of a previous replay still produces
//!   a root event with `replay: true` and a fresh `EventId`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;
use std::time::Duration;

use forge_events::{Event, EventSource, EventsError};
use forge_runtime::{
    CommandParser, EventBus, EventSubscription, NullEventLogRepo, QueueScheduler, ScriptRegistry,
    spawn_action_engine,
};
use forge_storage::DataProvider;
use forge_storage_sqlite::SqliteBackend;
use forge_types::{
    Action, ActionId, Command, CommandId, CommandPermission, LogLevel, SubActionSpec,
};

const TEST_KEY: [u8; 32] = [0xab; 32];
const PIPELINE_TIMEOUT_MS: u64 = 2_000;

async fn make_dp() -> Arc<dyn DataProvider> {
    Arc::new(
        SqliteBackend::open_with_key(":memory:", TEST_KEY)
            .await
            .unwrap(),
    )
}

async fn collect_until_kind(
    sub: &mut EventSubscription,
    target: &str,
    timeout_ms: u64,
) -> Vec<Event> {
    let mut collected = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_millis(timeout_ms);
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            break;
        }
        match tokio::time::timeout(remaining, sub.recv()).await {
            Ok(Ok(ev)) => {
                let is_target = ev.kind == target;
                collected.push(ev);
                if is_target {
                    break;
                }
            }
            Ok(Err(EventsError::LaggingReceiver)) => {}
            Ok(Err(_)) | Err(_) => break,
        }
    }
    collected
}

async fn recv_next_event(sub: &mut EventSubscription, timeout_ms: u64) -> Option<Event> {
    let deadline = tokio::time::Instant::now() + Duration::from_millis(timeout_ms);
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return None;
        }
        match tokio::time::timeout(remaining, sub.recv()).await {
            Ok(Ok(ev)) => return Some(ev),
            Ok(Err(EventsError::LaggingReceiver)) => {}
            Ok(Err(_)) | Err(_) => return None,
        }
    }
}

struct PipelineFixture {
    bus: Arc<EventBus>,
}

async fn spawn_pipeline() -> PipelineFixture {
    let dp = make_dp().await;

    let queue = dp
        .queue_repo()
        .get_by_name("Default")
        .await
        .unwrap()
        .expect("Default queue must be seeded by migration 0002_action_engine.sql");

    let action_id = ActionId::new();
    let action = Action {
        id: action_id,
        name: "replay-test action".into(),
        group: None,
        queue_id: queue.id,
        enabled: true,
        concurrent: false,
        bypass_pause: false,
        execution_mode: forge_types::ExecutionMode::Sequential,
        description: None,
        sub_actions: vec![SubActionSpec::Log {
            level: LogLevel::Info,
            message: "replay test step".into(),
        }],
    };
    dp.action_repo().save(&action).await.unwrap();

    let command = Command {
        id: CommandId::new(),
        action_id,
        name: "!replaytest".into(),
        cooldown_secs: 0,
        permission: CommandPermission::Everyone,
    };
    dp.command_repo().save(&command).await.unwrap();

    let bus = EventBus::new(Arc::new(NullEventLogRepo));
    let engine = spawn_action_engine(
        Arc::clone(&bus),
        Arc::clone(&dp),
        Arc::new(ScriptRegistry::new()),
        None,
        None,
        None,
    );
    let scheduler = QueueScheduler::spawn(engine, Arc::clone(&bus), vec![queue]);
    let _parser = CommandParser::spawn(Arc::clone(&bus), Arc::clone(&dp), scheduler);

    tokio::time::sleep(Duration::from_millis(10)).await;

    PipelineFixture { bus }
}

fn make_chat_event(command: &str) -> Event {
    Event::new(
        EventSource::Twitch,
        "chat.message",
        serde_json::json!({
            "message": command,
            "user_login": "tester_ua",
        }),
    )
}

/// Replay sets `replay: true` and produces a fresh `EventId` on the root event.
/// Downstream pipeline events driven by the replayed trigger have IDs entirely
/// distinct from the original run, proving the pipeline re-ran as new events.
#[tokio::test]
async fn replay_sets_flag_and_produces_fresh_downstream_ids() {
    let fixture = spawn_pipeline().await;
    let bus = &fixture.bus;
    let mut sub = bus.subscribe();

    let original_chat = make_chat_event("!replaytest arg1");
    let original_chat_id = original_chat.id;
    bus.publish(original_chat);

    let original_run = collect_until_kind(&mut sub, "action.done", PIPELINE_TIMEOUT_MS).await;
    assert!(
        original_run.iter().any(|e| e.kind == "action.done"),
        "original run must reach action.done within timeout"
    );

    let orig_ids: std::collections::HashSet<_> = original_run.iter().map(|e| e.id).collect();

    bus.replay_and_publish(original_chat_id)
        .await
        .expect("replay_and_publish must succeed for event in ring");

    let replayed_root = recv_next_event(&mut sub, 500)
        .await
        .expect("replayed root event must arrive within 500ms");
    assert_eq!(
        replayed_root.source,
        EventSource::Twitch,
        "replayed root event must preserve original source"
    );
    assert_eq!(
        replayed_root.kind, "chat.message",
        "replayed root event must preserve original kind"
    );
    assert!(
        replayed_root.replay,
        "replayed root event must have replay=true"
    );
    assert_ne!(
        replayed_root.id, original_chat_id,
        "replayed root event must have a fresh EventId"
    );

    let replay_run = collect_until_kind(&mut sub, "action.done", PIPELINE_TIMEOUT_MS).await;
    assert!(
        replay_run.iter().any(|e| e.kind == "action.done"),
        "replay run must reach action.done within timeout"
    );

    for ev in &replay_run {
        assert!(
            !orig_ids.contains(&ev.id),
            "replay run event '{}' id must not reuse an id from the original run",
            ev.kind
        );
    }
}

/// The causation chain of downstream replay events is coherent and transitively
/// rooted at the replayed root event. The full chain must be:
///   replayed_chat → command.matched → action.start → {subaction.run, action.done}
///
/// The command.matched event must be caused_by the replayed root (not the original
/// chat event), anchoring the entire chain to the replay.
#[tokio::test]
async fn replay_causation_chain_is_rooted_at_replayed_event() {
    let fixture = spawn_pipeline().await;
    let bus = &fixture.bus;
    let mut sub = bus.subscribe();

    let original_chat = make_chat_event("!replaytest chain");
    let original_chat_id = original_chat.id;
    bus.publish(original_chat);

    collect_until_kind(&mut sub, "action.done", PIPELINE_TIMEOUT_MS).await;

    bus.replay_and_publish(original_chat_id)
        .await
        .expect("replay_and_publish must succeed");

    let replayed_root = recv_next_event(&mut sub, 500)
        .await
        .expect("replayed root event must arrive");
    let replayed_root_id = replayed_root.id;
    assert!(replayed_root.replay, "replayed root must have replay=true");

    let replay_run = collect_until_kind(&mut sub, "action.done", PIPELINE_TIMEOUT_MS).await;
    assert!(
        replay_run.iter().any(|e| e.kind == "action.done"),
        "replay run must reach action.done"
    );

    // The command.matched event anchors the chain to the replayed root.
    let replay_cmd_matched = replay_run
        .iter()
        .find(|e| e.kind == "command.matched")
        .expect("command.matched must appear in replay run");

    assert_eq!(
        replay_cmd_matched.caused_by,
        Some(replayed_root_id),
        "command.matched must be caused_by the replayed root event — this anchors the whole chain"
    );
    assert_ne!(
        replay_cmd_matched.caused_by,
        Some(original_chat_id),
        "command.matched must not link back to the original (non-replay) chat event id"
    );

    // action.start is triggered by the scheduler using cmd_event_id as trigger.
    let replay_action_start = replay_run
        .iter()
        .find(|e| e.kind == "action.start")
        .expect("action.start must appear in replay run");

    assert_eq!(
        replay_action_start.caused_by,
        Some(replay_cmd_matched.id),
        "action.start must be caused_by the replay run's command.matched"
    );

    let replay_subaction = replay_run
        .iter()
        .find(|e| e.kind == "subaction.run")
        .expect("subaction.run must appear in replay run");

    assert_eq!(
        replay_subaction.caused_by,
        Some(replay_action_start.id),
        "subaction.run must be caused_by its action.start"
    );

    let replay_done = replay_run
        .iter()
        .find(|e| e.kind == "action.done")
        .expect("action.done must appear in replay run");

    assert_eq!(
        replay_done.caused_by,
        Some(replay_action_start.id),
        "action.done must be caused_by action.start, closing the chain"
    );
}

/// Replaying the result of a previous replay (replay-of-a-replay) must still
/// produce a root event with `replay: true` and a fresh `EventId`. There is no
/// special-case handling that would break nested replays.
#[tokio::test]
async fn replay_of_replay_still_has_replay_flag() {
    let fixture = spawn_pipeline().await;
    let bus = &fixture.bus;
    let mut sub = bus.subscribe();

    let original_chat = make_chat_event("!replaytest nested");
    let original_chat_id = original_chat.id;
    bus.publish(original_chat);

    collect_until_kind(&mut sub, "action.done", PIPELINE_TIMEOUT_MS).await;

    bus.replay_and_publish(original_chat_id)
        .await
        .expect("first replay must succeed");

    let first_replay_root = recv_next_event(&mut sub, 500)
        .await
        .expect("first replayed root must arrive");
    let first_replay_id = first_replay_root.id;
    assert!(
        first_replay_root.replay,
        "first replay root must have replay=true"
    );
    assert_ne!(first_replay_id, original_chat_id);

    collect_until_kind(&mut sub, "action.done", PIPELINE_TIMEOUT_MS).await;

    bus.replay_and_publish(first_replay_id)
        .await
        .expect("replay-of-a-replay must succeed; replayed event must be in ring");

    let second_replay_root = recv_next_event(&mut sub, 500)
        .await
        .expect("second replayed root must arrive");

    assert!(
        second_replay_root.replay,
        "replay-of-a-replay must still produce replay=true on the root event"
    );
    assert_ne!(
        second_replay_root.id, first_replay_id,
        "replay-of-a-replay must produce a fresh EventId"
    );
    assert_ne!(
        second_replay_root.id, original_chat_id,
        "replay-of-a-replay id must not match the original"
    );
    assert_eq!(second_replay_root.kind, "chat.message");
}
