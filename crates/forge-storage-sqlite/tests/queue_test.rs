#![allow(clippy::expect_used, clippy::unwrap_used)]

use forge_storage::{QueueRecord, QueueRepo};
use forge_storage_sqlite::{SqliteQueueRepo, apply_migrations};
use forge_types::QueueId;

async fn setup() -> SqliteQueueRepo {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:")
        .await
        .expect("in-memory pool");
    apply_migrations(&pool).await.expect("migrations apply");
    SqliteQueueRepo::new(pool)
}

fn make_queue(name: &str) -> QueueRecord {
    QueueRecord {
        id: QueueId::new(),
        name: name.to_owned(),
        blocking: false,
        enabled: true,
        paused: false,
        created_at: time::OffsetDateTime::now_utc(),
        last_modified: time::OffsetDateTime::now_utc(),
    }
}

#[tokio::test]
async fn upsert_then_get_roundtrips_queue() {
    let repo = setup().await;
    let queue = make_queue("default");
    let id = queue.id;
    repo.upsert(queue).await.expect("upsert");
    let got = repo.get(id).await.expect("get");
    assert!(got.is_some());
    let got = got.unwrap();
    assert_eq!(got.id, id);
    assert_eq!(got.name, "default");
    assert!(!got.blocking);
    assert!(got.enabled);
    assert!(!got.paused);
}

#[tokio::test]
async fn get_missing_queue_returns_none() {
    let repo = setup().await;
    let got = repo.get(QueueId::new()).await.expect("get");
    assert!(got.is_none());
}

#[tokio::test]
async fn get_by_name_finds_existing_queue() {
    let repo = setup().await;
    repo.upsert(make_queue("priority")).await.expect("upsert");
    let got = repo.get_by_name("priority").await.expect("get_by_name");
    assert!(got.is_some());
    assert_eq!(got.unwrap().name, "priority");
}

#[tokio::test]
async fn get_by_name_missing_returns_none() {
    let repo = setup().await;
    let got = repo.get_by_name("ghost").await.expect("get_by_name");
    assert!(got.is_none());
}

#[tokio::test]
async fn set_paused_flips_paused_flag() {
    let repo = setup().await;
    let queue = make_queue("pausable");
    let id = queue.id;
    repo.upsert(queue).await.expect("upsert");

    repo.set_paused(id, true).await.expect("set_paused true");
    let got = repo.get(id).await.expect("get").unwrap();
    assert!(got.paused);

    repo.set_paused(id, false).await.expect("set_paused false");
    let got = repo.get(id).await.expect("get").unwrap();
    assert!(!got.paused);
}

#[tokio::test]
async fn delete_existing_queue_returns_true() {
    let repo = setup().await;
    let queue = make_queue("to_delete");
    let id = queue.id;
    repo.upsert(queue).await.expect("upsert");
    assert!(repo.delete(id).await.expect("delete"));
    assert!(repo.get(id).await.expect("get").is_none());
}

#[tokio::test]
async fn delete_missing_queue_returns_false() {
    let repo = setup().await;
    assert!(!repo.delete(QueueId::new()).await.expect("delete"));
}

#[tokio::test]
async fn list_returns_all_queues() {
    let repo = setup().await;
    repo.upsert(make_queue("q1")).await.expect("upsert 1");
    repo.upsert(make_queue("q2")).await.expect("upsert 2");
    let all = repo.list().await.expect("list");
    assert_eq!(all.len(), 2);
    assert!(all.iter().any(|q| q.name == "q1"));
    assert!(all.iter().any(|q| q.name == "q2"));
}
