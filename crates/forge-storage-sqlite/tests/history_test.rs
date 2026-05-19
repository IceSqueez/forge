#![allow(clippy::expect_used, clippy::unwrap_used)]

use forge_storage::DataProvider;
use forge_storage_sqlite::SqliteBackend;
use forge_types::{ActionId, EventId, ExecutionContext, ExecutionMetadata, ExecutionOutcome};
use std::collections::BTreeMap;
use time::OffsetDateTime;

const TEST_KEY: [u8; 32] = [0xab; 32];

async fn setup() -> SqliteBackend {
    SqliteBackend::open_with_key("sqlite::memory:", TEST_KEY)
        .await
        .expect("open")
}

fn make_ctx(action_id: ActionId, event_id: EventId) -> ExecutionContext {
    ExecutionContext {
        action_id,
        metadata: ExecutionMetadata::Trigger { event_id },
        arg_stack_snapshot: BTreeMap::new(),
        started_at: OffsetDateTime::now_utc(),
        completed_at: Some(OffsetDateTime::now_utc()),
        telemetry: vec![],
        outcome: ExecutionOutcome::Success,
    }
}

#[tokio::test]
async fn save_then_recent_for_action_roundtrips() {
    let backend = setup().await;
    let action_id = ActionId::new();
    let event_id = EventId::new();
    let ctx = make_ctx(action_id, event_id);

    backend.history_repo().save(&ctx).await.expect("save");

    let records = backend
        .history_repo()
        .recent_for_action(action_id, 10)
        .await
        .expect("recent_for_action");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].action_id, action_id);
    assert!(
        matches!(&records[0].metadata, ExecutionMetadata::Trigger { event_id: eid } if *eid == event_id)
    );
    assert_eq!(records[0].outcome, ExecutionOutcome::Success);
}

#[tokio::test]
async fn recent_for_action_returns_empty_for_unknown_action() {
    let backend = setup().await;
    let records = backend
        .history_repo()
        .recent_for_action(ActionId::new(), 10)
        .await
        .expect("recent_for_action");
    assert!(records.is_empty());
}

#[tokio::test]
async fn recent_for_action_respects_limit() {
    let backend = setup().await;
    let action_id = ActionId::new();
    for _ in 0..5 {
        backend
            .history_repo()
            .save(&make_ctx(action_id, EventId::new()))
            .await
            .expect("save");
    }
    let records = backend
        .history_repo()
        .recent_for_action(action_id, 3)
        .await
        .expect("recent_for_action");
    assert_eq!(records.len(), 3);
}

#[tokio::test]
async fn recent_for_action_scoped_to_action() {
    let backend = setup().await;
    let action_a = ActionId::new();
    let action_b = ActionId::new();

    backend
        .history_repo()
        .save(&make_ctx(action_a, EventId::new()))
        .await
        .expect("save a");
    backend
        .history_repo()
        .save(&make_ctx(action_a, EventId::new()))
        .await
        .expect("save a2");
    backend
        .history_repo()
        .save(&make_ctx(action_b, EventId::new()))
        .await
        .expect("save b");

    let for_a = backend
        .history_repo()
        .recent_for_action(action_a, 100)
        .await
        .expect("recent_for_action a");
    assert_eq!(for_a.len(), 2);
    assert!(for_a.iter().all(|c| c.action_id == action_a));
}

#[tokio::test]
async fn save_failed_outcome_roundtrips() {
    let backend = setup().await;
    let action_id = ActionId::new();
    let mut ctx = make_ctx(action_id, EventId::new());
    ctx.outcome = ExecutionOutcome::Failed("rhai error".to_owned());

    backend.history_repo().save(&ctx).await.expect("save");

    let records = backend
        .history_repo()
        .recent_for_action(action_id, 1)
        .await
        .expect("recent_for_action");
    assert_eq!(records.len(), 1);
    assert!(matches!(&records[0].outcome, ExecutionOutcome::Failed(msg) if msg == "rhai error"));
}
