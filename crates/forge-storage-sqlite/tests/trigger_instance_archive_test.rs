#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::collections::BTreeMap;

use forge_storage::{DataProvider, StorageError};
use forge_storage_sqlite::SqliteBackend;
use forge_types::{Action, ActionId, ExecutionMode, QueueId, TriggerInstance, TriggerInstanceId};

const TEST_KEY: [u8; 32] = [0xcd; 32];

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

async fn insert_action(backend: &SqliteBackend, name: &str) -> ActionId {
    let queue_id = default_queue_id(backend).await;
    let action = Action {
        id: ActionId::new(),
        name: name.to_owned(),
        group: None,
        queue_id,
        enabled: true,
        concurrent: false,
        bypass_pause: false,
        execution_mode: ExecutionMode::Sequential,
        description: None,
        sub_actions: vec![],
    };
    let id = action.id;
    backend
        .action_repo()
        .save(&action)
        .await
        .expect("save action");
    id
}

fn make_instance(kind_id: &str, name: &str, user_defined: bool) -> TriggerInstance {
    TriggerInstance {
        id: TriggerInstanceId::new(),
        kind_id: kind_id.to_owned(),
        name: name.to_owned(),
        overrides: BTreeMap::new(),
        enabled: true,
        user_defined,
        platform_scope: Default::default(),
    }
}

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

    let inst = make_instance("twitch.chat.command", "Trigger", true);
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

    let inst = make_instance("twitch.chat.command", "Trigger", true);
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

    let inst = make_instance("twitch.chat.command", "Hidden", true);
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

    let archived = repo.list_archived().await.expect("list_archived");
    let entry = archived
        .iter()
        .find(|i| i.id == id)
        .expect("archived instance must appear in list_archived");
    assert_eq!(entry.name, "Hidden", "row content survives archiving");
}

#[tokio::test]
async fn restore_returns_archived_instance_to_visibility() {
    let backend = setup().await;
    let repo = backend.trigger_instance_repo();

    let inst = make_instance("twitch.chat.command", "Restored", true);
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
    assert!(
        repo.list_archived()
            .await
            .expect("list_archived")
            .iter()
            .all(|i| i.id != id),
        "restored instance must leave list_archived"
    );
}

#[tokio::test]
async fn delete_is_blocked_by_link_from_archived_action() {
    // EDGE: delete()'s reference probe queries action_trigger_instances directly,
    // unfiltered by archived_at, because the FK is ON DELETE RESTRICT regardless
    // of the linked action's archive state. Archiving the action must NOT unblock
    // the instance's hard delete.
    let backend = setup().await;
    let repo = backend.trigger_instance_repo();
    let action_id = insert_action(&backend, "archived blocker").await;

    let inst = make_instance("twitch.chat.command", "Referenced", true);
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

    let err = repo
        .delete(inst.id)
        .await
        .expect_err("delete must be blocked");
    match err {
        StorageError::ReferenceBlock {
            used_in_count,
            sample_action_names,
        } => {
            assert_eq!(
                used_in_count, 1,
                "the archived action still counts as a reference"
            );
            assert_eq!(sample_action_names, vec!["archived blocker"]);
        }
        other => panic!("expected ReferenceBlock, got {other:?}"),
    }
}

#[tokio::test]
async fn actions_using_excludes_archived_actions() {
    // actions_using() joins on live actions only (a.archived_at IS NULL), so an
    // archived action drops out of the usage list — contrast with delete()'s
    // unfiltered probe above.
    let backend = setup().await;
    let repo = backend.trigger_instance_repo();
    let live = insert_action(&backend, "live user").await;
    let archived = insert_action(&backend, "archived user").await;

    let inst = make_instance("twitch.chat.command", "Shared", true);
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
