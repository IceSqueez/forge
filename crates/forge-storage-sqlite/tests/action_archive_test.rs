#![allow(clippy::expect_used, clippy::unwrap_used)]

use forge_storage::DataProvider;
use forge_storage_sqlite::SqliteBackend;

use forge_types::{Action, ActionId, ExecutionMode, QueueId};

const TEST_KEY: [u8; 32] = [0xab; 32];

async fn setup() -> SqliteBackend {
    SqliteBackend::open_with_key("sqlite::memory:", TEST_KEY)
        .await
        .expect("open")
}

async fn default_queue_id(backend: &SqliteBackend) -> QueueId {
    backend
        .queue_repo()
        .get_by_name("Default")
        .await
        .expect("get default queue")
        .expect("default queue must exist after migration")
        .id
}

fn make_action(name: &str, group: Option<&str>, queue_id: QueueId) -> Action {
    Action {
        id: ActionId::new(),
        name: name.to_owned(),
        group: group.map(str::to_owned),
        queue_id,
        enabled: true,
        concurrent: false,
        bypass_pause: false,
        execution_mode: ExecutionMode::Sequential,
        description: None,
        sub_actions: vec![],
    }
}

#[tokio::test]
async fn archive_transitions_live_to_archived_and_is_false_otherwise() {
    let backend = setup().await;
    let queue_id = default_queue_id(&backend).await;
    let repo = backend.action_repo();

    assert!(
        !repo
            .archive(ActionId::new())
            .await
            .expect("archive missing"),
        "archiving an unknown action id must return false"
    );

    let action = make_action("greet", None, queue_id);
    let id = action.id;
    repo.save(&action).await.expect("save");

    assert!(
        repo.archive(id).await.expect("archive live"),
        "live -> true"
    );
    assert!(
        !repo.archive(id).await.expect("archive again"),
        "already-archived -> false"
    );
}

#[tokio::test]
async fn restore_transitions_archived_to_live_and_is_false_otherwise() {
    let backend = setup().await;
    let queue_id = default_queue_id(&backend).await;
    let repo = backend.action_repo();

    assert!(
        !repo
            .restore(ActionId::new())
            .await
            .expect("restore missing"),
        "restoring an unknown action id must return false"
    );

    let action = make_action("greet", None, queue_id);
    let id = action.id;
    repo.save(&action).await.expect("save");
    assert!(
        !repo.restore(id).await.expect("restore live"),
        "never-archived -> false"
    );

    repo.archive(id).await.expect("archive");
    assert!(
        repo.restore(id).await.expect("restore archived"),
        "archived -> true"
    );
    assert!(
        !repo.restore(id).await.expect("restore again"),
        "already-live -> false"
    );
}

#[tokio::test]
async fn archive_hides_action_from_list_get_and_by_group_and_lists_it_in_archived() {
    let backend = setup().await;
    let queue_id = default_queue_id(&backend).await;
    let repo = backend.action_repo();

    let action = make_action("chat_greet", Some("Chat"), queue_id);
    let id = action.id;
    repo.save(&action).await.expect("save");
    repo.archive(id).await.expect("archive");

    assert!(repo.get(id).await.expect("get").is_none(), "get() hidden");
    assert!(
        repo.list().await.expect("list").iter().all(|a| a.id != id),
        "list() excludes archived action"
    );
    assert!(
        repo.list_by_group(Some("Chat"))
            .await
            .expect("list_by_group")
            .iter()
            .all(|a| a.id != id),
        "list_by_group() excludes archived action"
    );

    let archived = repo.list_archived().await.expect("list_archived");
    let entry = archived
        .iter()
        .find(|a| a.id == id)
        .expect("archived action must appear in list_archived");
    assert_eq!(entry.name, "chat_greet", "row content survives archiving");
}

#[tokio::test]
async fn restore_returns_archived_action_to_visibility() {
    let backend = setup().await;
    let queue_id = default_queue_id(&backend).await;
    let repo = backend.action_repo();

    let action = make_action("restored", None, queue_id);
    let id = action.id;
    repo.save(&action).await.expect("save");
    repo.archive(id).await.expect("archive");
    repo.restore(id).await.expect("restore");

    assert!(
        repo.get(id).await.expect("get").is_some(),
        "get() visible again"
    );
    assert!(
        repo.list().await.expect("list").iter().any(|a| a.id == id),
        "list() shows restored action"
    );
    assert!(
        repo.list_archived()
            .await
            .expect("list_archived")
            .iter()
            .all(|a| a.id != id),
        "restored action must leave list_archived"
    );
}
