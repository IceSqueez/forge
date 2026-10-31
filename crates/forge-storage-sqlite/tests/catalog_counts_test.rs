#![allow(clippy::expect_used)]
use std::collections::BTreeMap;

use forge_storage::{DataProvider, GlobalsRepo};
use forge_storage_sqlite::SqliteBackend;
use forge_types::{PermissionRung, TriggerInstance, TriggerInstanceId, Variant};

mod common;
use common::{insert_action, setup};

const COMMAND: &str = "twitch.command";
const FOLLOW: &str = "twitch.follow";

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
    insert_action(&backend, "kept").await;
    let archived = insert_action(&backend, "archived").await;
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
