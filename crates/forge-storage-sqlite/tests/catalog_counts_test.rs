#![allow(clippy::expect_used)]
use std::collections::BTreeMap;

use forge_storage::{DataProvider, GlobalsRepo};
use forge_storage_sqlite::SqliteBackend;
use forge_types::{
    Action, ActionId, ExecutionMode, PermissionRung, TriggerInstance, TriggerInstanceId, Variant,
};

mod common;
use common::Sandboxed;

const TEST_KEY: [u8; 32] = [0xcd; 32];
const COMMAND: &str = "twitch.command";
const FOLLOW: &str = "twitch.follow";

async fn setup() -> Sandboxed<SqliteBackend> {
    common::sandboxed_backend("sqlite::memory:", TEST_KEY).await
}

async fn saved_action(backend: &SqliteBackend, name: &str) -> ActionId {
    let queue_id = backend
        .queue_repo()
        .get_by_name("Default")
        .await
        .expect("get default queue")
        .expect("default queue exists after migration")
        .id;
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
    backend
        .action_repo()
        .save(&action)
        .await
        .expect("save action");
    action.id
}

async fn saved_trigger(backend: &SqliteBackend, kind_id: &str, name: &str) -> TriggerInstanceId {
    let instance = TriggerInstance {
        id: TriggerInstanceId::new(),
        kind_id: kind_id.to_owned(),
        name: name.to_owned(),
        overrides: BTreeMap::new(),
        enabled: true,
        user_defined: true,
        platform_scope: Default::default(),
        cooldown_secs: 0,
        cooldown_global: true,
        permission_rung: PermissionRung::Everyone,
    };
    backend
        .trigger_instance_repo()
        .save(&instance)
        .await
        .expect("save trigger");
    instance.id
}

#[tokio::test]
async fn action_count_matches_the_list_and_leaves_out_archived_actions() {
    let backend = setup().await;
    let repo = backend.action_repo();
    let before = repo.count().await.expect("count");
    saved_action(&backend, "kept").await;
    let archived = saved_action(&backend, "archived").await;
    repo.archive(archived).await.expect("archive");

    let count = repo.count().await.expect("count");

    assert_eq!(count, before + 1);
    assert_eq!(count, repo.list().await.expect("list").len());
}

#[tokio::test]
async fn globals_count_matches_the_list_and_leaves_out_archived_globals() {
    let backend = setup().await;
    let repo: &SqliteBackend = &backend;
    GlobalsRepo::set(repo, "kept", Variant::Int(1), true)
        .await
        .expect("set");
    GlobalsRepo::set(repo, "archived", Variant::Int(2), true)
        .await
        .expect("set");
    GlobalsRepo::archive(repo, "archived")
        .await
        .expect("archive");

    let count = GlobalsRepo::count(repo).await.expect("count");

    assert_eq!(count, 1);
    assert_eq!(count, GlobalsRepo::list(repo).await.expect("list").len());
}

#[tokio::test]
async fn trigger_count_covers_exactly_the_named_kinds_and_leaves_out_archived_triggers() {
    let backend = setup().await;
    let repo = backend.trigger_instance_repo();
    saved_trigger(&backend, COMMAND, "kept").await;
    saved_trigger(&backend, FOLLOW, "other kind").await;
    let archived = saved_trigger(&backend, COMMAND, "archived").await;
    repo.archive(archived).await.expect("archive");
    let listed = repo.list_all().await.expect("list");

    for kinds in [
        vec![],
        vec![COMMAND.to_owned()],
        vec![COMMAND.to_owned(), FOLLOW.to_owned()],
        vec!["never.stored".to_owned()],
    ] {
        let listed_of_kinds = listed
            .iter()
            .filter(|instance| kinds.contains(&instance.kind_id))
            .count();

        assert_eq!(
            repo.count_of_kinds(&kinds).await.expect("count"),
            listed_of_kinds,
            "{kinds:?}"
        );
    }
    assert_eq!(
        repo.count_of_kinds(&[COMMAND.to_owned()])
            .await
            .expect("count"),
        1
    );
}
