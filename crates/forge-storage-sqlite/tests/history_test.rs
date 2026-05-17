#![allow(clippy::expect_used, clippy::unwrap_used)]

use forge_storage::{HistoryOutcome, HistoryRepo, NewHistoryRecord};
use forge_storage_sqlite::{SqliteHistoryRepo, apply_migrations};
use forge_types::{ActionId, EventId};
use time::OffsetDateTime;

async fn setup() -> SqliteHistoryRepo {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:")
        .await
        .expect("in-memory pool");
    apply_migrations(&pool).await.expect("migrations apply");
    SqliteHistoryRepo::new(pool)
}

fn make_record(action_id: ActionId) -> NewHistoryRecord {
    NewHistoryRecord {
        action_id,
        triggering_event_id: None,
        started_at: OffsetDateTime::now_utc(),
        duration_ms: 42,
        outcome: HistoryOutcome::Ok,
        context_json: r#"{"steps":[]}"#.to_owned(),
    }
}

#[tokio::test]
async fn record_returns_inserted_id() {
    let repo = setup().await;
    let id = repo
        .record(make_record(ActionId::new()))
        .await
        .expect("record");
    assert!(id > 0);
}

#[tokio::test]
async fn get_retrieves_inserted_record() {
    let repo = setup().await;
    let action_id = ActionId::new();
    let new = make_record(action_id);
    let id = repo.record(new).await.expect("record");

    let got = repo.get(id).await.expect("get").expect("present");
    assert_eq!(got.id, id);
    assert_eq!(got.action_id, action_id);
    assert!(got.triggering_event_id.is_none());
    assert_eq!(got.duration_ms, 42);
    assert_eq!(got.outcome, HistoryOutcome::Ok);
    assert_eq!(got.context_json, r#"{"steps":[]}"#);
}

#[tokio::test]
async fn get_missing_id_returns_none() {
    let repo = setup().await;
    let got = repo.get(9999).await.expect("get");
    assert!(got.is_none());
}

#[tokio::test]
async fn list_for_action_filters_by_action_id() {
    let repo = setup().await;
    let target = ActionId::new();
    let other = ActionId::new();

    repo.record(make_record(target)).await.expect("record 1");
    repo.record(make_record(target)).await.expect("record 2");
    repo.record(make_record(other)).await.expect("record other");

    let records = repo.list_for_action(target, 100).await.expect("list");
    assert_eq!(records.len(), 2);
    assert!(records.iter().all(|r| r.action_id == target));
}

#[tokio::test]
async fn list_for_action_respects_limit() {
    let repo = setup().await;
    let action_id = ActionId::new();

    for _ in 0..5 {
        repo.record(make_record(action_id)).await.expect("record");
    }

    let records = repo.list_for_action(action_id, 3).await.expect("list");
    assert_eq!(records.len(), 3);
}

#[tokio::test]
async fn list_recent_includes_all_inserted_records() {
    let repo = setup().await;

    let id_a = repo
        .record(make_record(ActionId::new()))
        .await
        .expect("record a");
    let id_b = repo
        .record(make_record(ActionId::new()))
        .await
        .expect("record b");

    let records = repo.list_recent(10).await.expect("list");
    assert!(records.len() >= 2);
    assert!(records.iter().any(|r| r.id == id_a));
    assert!(records.iter().any(|r| r.id == id_b));
}

#[tokio::test]
async fn list_caused_by_filters_by_triggering_event_id() {
    let repo = setup().await;
    let event_id = EventId::new();
    let other_event = EventId::new();

    let mut with_event = make_record(ActionId::new());
    with_event.triggering_event_id = Some(event_id);
    repo.record(with_event).await.expect("record with event");

    let mut with_other = make_record(ActionId::new());
    with_other.triggering_event_id = Some(other_event);
    repo.record(with_other)
        .await
        .expect("record with other event");

    repo.record(make_record(ActionId::new()))
        .await
        .expect("record no event");

    let records = repo.list_caused_by(event_id).await.expect("list_caused_by");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].triggering_event_id, Some(event_id));
}

#[tokio::test]
async fn list_caused_by_returns_empty_when_no_match() {
    let repo = setup().await;
    repo.record(make_record(ActionId::new()))
        .await
        .expect("record");

    let records = repo
        .list_caused_by(EventId::new())
        .await
        .expect("list_caused_by");
    assert!(records.is_empty());
}

#[tokio::test]
async fn prune_older_than_returns_deletion_count() {
    let repo = setup().await;

    let past = OffsetDateTime::from_unix_timestamp(1_000_000).unwrap();
    let cutoff = OffsetDateTime::from_unix_timestamp(1_500_000).unwrap();
    let future = OffsetDateTime::from_unix_timestamp(2_000_000).unwrap();

    let mut old_rec = make_record(ActionId::new());
    old_rec.started_at = past;
    repo.record(old_rec).await.expect("record old");

    let mut new_rec = make_record(ActionId::new());
    new_rec.started_at = future;
    repo.record(new_rec).await.expect("record new");

    let deleted = repo.prune_older_than(cutoff).await.expect("prune");
    assert_eq!(deleted, 1);

    let remaining = repo.list_recent(100).await.expect("list");
    assert_eq!(remaining.len(), 1);
    assert!(remaining[0].started_at > cutoff);
}

#[tokio::test]
async fn record_stores_and_retrieves_triggering_event_id() {
    let repo = setup().await;
    let event_id = EventId::new();
    let mut new = make_record(ActionId::new());
    new.triggering_event_id = Some(event_id);

    let id = repo.record(new).await.expect("record");
    let got = repo.get(id).await.expect("get").expect("present");
    assert_eq!(got.triggering_event_id, Some(event_id));
}
