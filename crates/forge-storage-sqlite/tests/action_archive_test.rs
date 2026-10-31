#![allow(clippy::expect_used)]
use forge_storage::DataProvider;

use forge_types::{Action, ActionId, QueueId};

mod common;
use common::{action_named, default_queue_id, setup};

fn make_action(name: &str, group: Option<&str>, queue_id: QueueId) -> Action {
    Action {
        group: group.map(str::to_owned),
        ..action_named(name, queue_id)
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
}
