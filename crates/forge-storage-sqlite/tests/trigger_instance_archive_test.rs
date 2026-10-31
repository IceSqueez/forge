#![allow(clippy::expect_used)]

use forge_storage::{DataProvider, StorageError};
use forge_types::TriggerInstanceId;

mod common;
use common::{insert_action, setup, trigger_instance};

#[tokio::test]
async fn archive_transitions_live_to_archived_and_is_false_otherwise() {
    let backend = setup().await;
    let repo = backend.trigger_instance_repo();

    assert!(
        !repo
            .archive(TriggerInstanceId::new())
            .await
            .expect("archive missing"),
        "archiving an unknown instance id must return false"
    );

    let inst = trigger_instance("twitch.chat.command", "Trigger", true);
    let id = inst.id;
    repo.save(&inst).await.expect("save");

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
    let repo = backend.trigger_instance_repo();

    assert!(
        !repo
            .restore(TriggerInstanceId::new())
            .await
            .expect("restore missing"),
        "restoring an unknown instance id must return false"
    );

    let inst = trigger_instance("twitch.chat.command", "Trigger", true);
    let id = inst.id;
    repo.save(&inst).await.expect("save");
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
async fn archive_hides_instance_from_reads_and_lists_it_in_archived() {
    let backend = setup().await;
    let repo = backend.trigger_instance_repo();
    let action_id = insert_action(&backend, "host action").await;

    let inst = trigger_instance("twitch.chat.command", "Hidden", true);
    let id = inst.id;
    repo.save(&inst).await.expect("save");
    backend
        .insert_action_trigger_instance_for_test(action_id, id, 0)
        .await
        .expect("link");
    repo.archive(id).await.expect("archive");

    assert!(repo.get(id).await.expect("get").is_none(), "get() hidden");
    assert!(
        repo.list_all()
            .await
            .expect("list_all")
            .iter()
            .all(|i| i.id != id),
        "list_all() excludes archived instance"
    );
    assert!(
        repo.list_user_defined()
            .await
            .expect("list_user_defined")
            .iter()
            .all(|i| i.id != id),
        "list_user_defined() excludes archived instance"
    );
    assert!(
        repo.list_for_action(action_id)
            .await
            .expect("list_for_action")
            .iter()
            .all(|i| i.id != id),
        "list_for_action() excludes archived instance"
    );
}

#[tokio::test]
async fn restore_returns_archived_instance_to_visibility() {
    let backend = setup().await;
    let repo = backend.trigger_instance_repo();

    let inst = trigger_instance("twitch.chat.command", "Restored", true);
    let id = inst.id;
    repo.save(&inst).await.expect("save");
    repo.archive(id).await.expect("archive");
    repo.restore(id).await.expect("restore");

    assert!(
        repo.get(id).await.expect("get").is_some(),
        "get() visible again"
    );
    assert!(
        repo.list_all()
            .await
            .expect("list_all")
            .iter()
            .any(|i| i.id == id),
        "list_all() shows restored instance"
    );
}

#[tokio::test]
async fn delete_succeeds_when_only_archived_actions_use_the_instance() {
    let backend = setup().await;
    let repo = backend.trigger_instance_repo();
    let action_id = insert_action(&backend, "archived user").await;

    let inst = trigger_instance("twitch.chat.command", "Referenced", true);
    repo.save(&inst).await.expect("save");
    backend
        .insert_action_trigger_instance_for_test(action_id, inst.id, 0)
        .await
        .expect("link");
    backend
        .action_repo()
        .archive(action_id)
        .await
        .expect("archive action");

    assert!(repo.delete(inst.id).await.expect("delete"));
    assert!(repo.get(inst.id).await.expect("get").is_none());
}

#[tokio::test]
async fn delete_is_blocked_by_live_users_and_counts_only_them() {
    let backend = setup().await;
    let repo = backend.trigger_instance_repo();
    let live = insert_action(&backend, "live user").await;
    let archived = insert_action(&backend, "archived user").await;

    let inst = trigger_instance("twitch.chat.command", "Shared", true);
    repo.save(&inst).await.expect("save");
    backend
        .insert_action_trigger_instance_for_test(live, inst.id, 0)
        .await
        .expect("link live");
    backend
        .insert_action_trigger_instance_for_test(archived, inst.id, 1)
        .await
        .expect("link archived");
    backend
        .action_repo()
        .archive(archived)
        .await
        .expect("archive action");

    let err = repo
        .delete(inst.id)
        .await
        .expect_err("delete must be blocked");

    match err {
        StorageError::ReferenceBlock {
            used_in_count,
            sample_action_names,
        } => {
            assert_eq!(used_in_count, 1);
            assert_eq!(sample_action_names, vec!["live user"]);
        }
        other => panic!("expected ReferenceBlock, got {other:?}"),
    }
    assert!(repo.get(inst.id).await.expect("get").is_some());
}

#[tokio::test]
async fn actions_using_excludes_archived_actions() {
    let backend = setup().await;
    let repo = backend.trigger_instance_repo();
    let live = insert_action(&backend, "live user").await;
    let archived = insert_action(&backend, "archived user").await;

    let inst = trigger_instance("twitch.chat.command", "Shared", true);
    repo.save(&inst).await.expect("save");
    backend
        .insert_action_trigger_instance_for_test(live, inst.id, 0)
        .await
        .expect("link live");
    backend
        .insert_action_trigger_instance_for_test(archived, inst.id, 1)
        .await
        .expect("link archived");

    backend
        .action_repo()
        .archive(archived)
        .await
        .expect("archive action");

    let using = repo.actions_using(inst.id).await.expect("actions_using");
    assert_eq!(using, vec![live], "only the live action is reported");
}
