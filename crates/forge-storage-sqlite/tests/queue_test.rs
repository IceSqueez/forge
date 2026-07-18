#![allow(clippy::expect_used, clippy::unwrap_used)]

use forge_storage::DataProvider;
use forge_storage_sqlite::SqliteBackend;
use forge_types::{Queue, QueueId};

const TEST_KEY: [u8; 32] = [0xab; 32];

async fn setup() -> SqliteBackend {
    SqliteBackend::open_with_key("sqlite::memory:", TEST_KEY)
        .await
        .expect("open")
}

fn make_queue(name: &str, blocking: bool) -> Queue {
    Queue {
        id: QueueId::new(),
        name: name.to_owned(),
        description: String::new(),
        blocking,
    }
}

#[tokio::test]
async fn default_queue_exists_after_migration() {
    let backend = setup().await;
    let got = backend
        .queue_repo()
        .get_by_name("Default")
        .await
        .expect("get_by_name");
    assert!(got.is_some());
    assert_eq!(got.unwrap().name, "Default");
}

#[tokio::test]
async fn save_then_get_roundtrips_queue() {
    let backend = setup().await;
    let queue = make_queue("priority", true);
    let id = queue.id;
    backend.queue_repo().save(&queue).await.expect("save");
    let got = backend.queue_repo().get(id).await.expect("get");
    assert!(got.is_some());
    let got = got.unwrap();
    assert_eq!(got.id, id);
    assert_eq!(got.name, "priority");
    assert!(got.blocking);
}

#[tokio::test]
async fn get_missing_queue_returns_none() {
    let backend = setup().await;
    let got = backend.queue_repo().get(QueueId::new()).await.expect("get");
    assert!(got.is_none());
}

#[tokio::test]
async fn get_by_name_finds_existing_queue() {
    let backend = setup().await;
    backend
        .queue_repo()
        .save(&make_queue("slow", false))
        .await
        .expect("save");
    let got = backend
        .queue_repo()
        .get_by_name("slow")
        .await
        .expect("get_by_name");
    assert!(got.is_some());
    assert_eq!(got.unwrap().name, "slow");
}

#[tokio::test]
async fn get_by_name_missing_returns_none() {
    let backend = setup().await;
    let got = backend
        .queue_repo()
        .get_by_name("ghost")
        .await
        .expect("get_by_name");
    assert!(got.is_none());
}

#[tokio::test]
async fn save_updates_existing_queue() {
    let backend = setup().await;
    let mut queue = make_queue("updatable", false);
    let id = queue.id;
    backend.queue_repo().save(&queue).await.expect("save 1");

    queue.blocking = true;
    backend.queue_repo().save(&queue).await.expect("save 2");

    let got = backend.queue_repo().get(id).await.expect("get").unwrap();
    assert!(got.blocking);
}

#[tokio::test]
async fn delete_existing_queue_returns_true() {
    let backend = setup().await;
    let queue = make_queue("to_delete", false);
    let id = queue.id;
    backend.queue_repo().save(&queue).await.expect("save");
    assert!(backend.queue_repo().delete(id).await.expect("delete"));
    assert!(backend.queue_repo().get(id).await.expect("get").is_none());
}

#[tokio::test]
async fn delete_missing_queue_returns_false() {
    let backend = setup().await;
    assert!(
        !backend
            .queue_repo()
            .delete(QueueId::new())
            .await
            .expect("delete")
    );
}

#[tokio::test]
async fn list_returns_all_queues_including_default() {
    let backend = setup().await;
    backend
        .queue_repo()
        .save(&make_queue("q1", false))
        .await
        .expect("save q1");
    backend
        .queue_repo()
        .save(&make_queue("q2", true))
        .await
        .expect("save q2");
    let all = backend.queue_repo().list().await.expect("list");
    assert!(all.len() >= 3);
    assert!(all.iter().any(|q| q.name == "q1"));
    assert!(all.iter().any(|q| q.name == "q2"));
    assert!(all.iter().any(|q| q.name == "Default"));
}
