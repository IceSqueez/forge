#![allow(clippy::expect_used)]
use std::collections::{BTreeMap, BTreeSet};

use forge_storage::{DataProvider, StorageError, TriggerChanges};
use forge_storage_sqlite::SqliteBackend;
use forge_types::{
    Action, ActionId, ExecutionMode, PermissionRung, PlatformId, PlatformScope, QueueId,
    TriggerInstance, TriggerInstanceId, Variant,
};

mod common;
use common::Sandboxed;

const TEST_KEY: [u8; 32] = [0xcd; 32];

async fn setup() -> Sandboxed<SqliteBackend> {
    common::sandboxed_backend("sqlite::memory:", TEST_KEY).await
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
        cooldown_secs: 0,
        cooldown_global: true,
        permission_rung: PermissionRung::Everyone,
    }
}

async fn saved_instances_in_id_order<const N: usize>(
    backend: &SqliteBackend,
) -> [TriggerInstanceId; N] {
    let repo = backend.trigger_instance_repo();
    let mut ids = [(); N].map(|()| TriggerInstanceId::new());
    ids.sort();
    for (index, id) in ids.iter().enumerate() {
        let instance = TriggerInstance {
            id: *id,
            ..make_instance("twitch.chat.command", &format!("Linked {index}"), true)
        };
        repo.save(&instance).await.expect("save instance");
    }
    ids
}

async fn linked_ids(backend: &SqliteBackend, action_id: ActionId) -> Vec<TriggerInstanceId> {
    backend
        .trigger_instance_repo()
        .list_for_action(action_id)
        .await
        .expect("list_for_action")
        .into_iter()
        .map(|instance| instance.id)
        .collect()
}

#[tokio::test]
async fn save_and_get_roundtrip() {
    let backend = setup().await;
    let repo = backend.trigger_instance_repo();
    let inst = make_instance("twitch.chat.command", "My Trigger", true);
    let id = inst.id;
    repo.save(&inst).await.expect("save");
    let got = repo.get(id).await.expect("get");
    assert!(got.is_some());
    let got = got.unwrap();
    assert_eq!(got.id, id);
    assert_eq!(got.kind_id, "twitch.chat.command");
    assert_eq!(got.name, "My Trigger");
    assert!(got.user_defined);
    assert!(got.enabled);
}

#[tokio::test]
async fn get_missing_returns_none() {
    let backend = setup().await;
    let repo = backend.trigger_instance_repo();
    let got = repo.get(TriggerInstanceId::new()).await.expect("get");
    assert!(got.is_none());
}

#[tokio::test]
async fn save_updates_existing_row() {
    let backend = setup().await;
    let repo = backend.trigger_instance_repo();
    let mut inst = make_instance("twitch.chat.command", "Original", true);
    let id = inst.id;
    repo.save(&inst).await.expect("save 1");
    inst.name = "Updated".to_owned();
    repo.save(&inst).await.expect("save 2");
    let got = repo.get(id).await.expect("get").unwrap();
    assert_eq!(got.name, "Updated");
}

#[tokio::test]
async fn overrides_survive_roundtrip() {
    let backend = setup().await;
    let repo = backend.trigger_instance_repo();
    let mut overrides = BTreeMap::new();
    overrides.insert("min_bits".to_owned(), Variant::Int(500));
    overrides.insert("label".to_owned(), Variant::String("VIP".to_owned()));
    let inst = TriggerInstance {
        id: TriggerInstanceId::new(),
        kind_id: "twitch.support.cheer".to_owned(),
        name: "Big Cheer".to_owned(),
        overrides,
        enabled: true,
        user_defined: true,
        platform_scope: Default::default(),
        cooldown_secs: 0,
        cooldown_global: true,
        permission_rung: PermissionRung::Everyone,
    };
    let id = inst.id;
    repo.save(&inst).await.expect("save");
    let got = repo.get(id).await.expect("get").unwrap();
    assert_eq!(got.overrides.get("min_bits"), Some(&Variant::Int(500)));
    assert_eq!(
        got.overrides.get("label"),
        Some(&Variant::String("VIP".to_owned()))
    );
}

#[tokio::test]
async fn list_user_defined_filters_correctly() {
    let backend = setup().await;
    let repo = backend.trigger_instance_repo();

    let user = make_instance("twitch.chat.command", "User Trigger", true);
    let default = make_instance("twitch.chat.command", "Default", false);
    repo.save(&user).await.expect("save user");
    repo.save(&default).await.expect("save default");

    let list = repo.list_user_defined().await.expect("list");
    assert_eq!(list.len(), 1);
    assert!(list[0].user_defined);
    assert_eq!(list[0].id, user.id);
}

#[tokio::test]
async fn list_for_action_returns_instances_in_position_order() {
    let backend = setup().await;
    let repo = backend.trigger_instance_repo();
    let action_id = insert_action(&backend, "ordered action").await;

    let inst_a = make_instance("twitch.chat.command", "First", true);
    let inst_b = make_instance("twitch.support.cheer", "Second", true);
    repo.save(&inst_a).await.expect("save a");
    repo.save(&inst_b).await.expect("save b");

    backend
        .insert_action_trigger_instance_for_test(action_id, inst_a.id, 1)
        .await
        .expect("join a");
    backend
        .insert_action_trigger_instance_for_test(action_id, inst_b.id, 0)
        .await
        .expect("join b");

    let list = repo.list_for_action(action_id).await.expect("list");
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].id, inst_b.id, "position 0 must come first");
    assert_eq!(list[1].id, inst_a.id, "position 1 must come second");
}

#[tokio::test]
async fn list_for_action_empty_when_no_instances_assigned() {
    let backend = setup().await;
    let repo = backend.trigger_instance_repo();
    let action_id = insert_action(&backend, "empty action").await;
    let list = repo.list_for_action(action_id).await.expect("list");
    assert!(list.is_empty());
}

#[tokio::test]
async fn actions_using_returns_correct_action_ids() {
    let backend = setup().await;
    let repo = backend.trigger_instance_repo();
    let action_a = insert_action(&backend, "action a").await;
    let action_b = insert_action(&backend, "action b").await;
    let inst = make_instance("twitch.chat.command", "Shared", true);
    repo.save(&inst).await.expect("save");

    backend
        .insert_action_trigger_instance_for_test(action_a, inst.id, 0)
        .await
        .expect("join a");
    backend
        .insert_action_trigger_instance_for_test(action_b, inst.id, 0)
        .await
        .expect("join b");

    let mut using = repo.actions_using(inst.id).await.expect("actions_using");
    using.sort();
    let mut expected = vec![action_a, action_b];
    expected.sort();
    assert_eq!(using, expected);
}

#[tokio::test]
async fn actions_using_empty_when_not_referenced() {
    let backend = setup().await;
    let repo = backend.trigger_instance_repo();
    let inst = make_instance("twitch.chat.command", "Lonely", true);
    repo.save(&inst).await.expect("save");
    let using = repo.actions_using(inst.id).await.expect("actions_using");
    assert!(using.is_empty());
}

#[tokio::test]
async fn delete_unreferenced_instance_returns_true() {
    let backend = setup().await;
    let repo = backend.trigger_instance_repo();
    let inst = make_instance("twitch.chat.command", "To Delete", true);
    let id = inst.id;
    repo.save(&inst).await.expect("save");
    assert!(repo.delete(id).await.expect("delete"));
    assert!(repo.get(id).await.expect("get after delete").is_none());
}

#[tokio::test]
async fn delete_missing_instance_returns_false() {
    let backend = setup().await;
    let repo = backend.trigger_instance_repo();
    let deleted = repo.delete(TriggerInstanceId::new()).await.expect("delete");
    assert!(!deleted);
}

#[tokio::test]
async fn delete_referenced_instance_returns_reference_block() {
    let backend = setup().await;
    let repo = backend.trigger_instance_repo();
    let action_id = insert_action(&backend, "blocking action").await;
    let inst = make_instance("twitch.chat.command", "Referenced", true);
    repo.save(&inst).await.expect("save");
    backend
        .insert_action_trigger_instance_for_test(action_id, inst.id, 0)
        .await
        .expect("join");

    let err = repo.delete(inst.id).await.expect_err("should fail");
    match err {
        StorageError::ReferenceBlock {
            used_in_count,
            sample_action_names,
        } => {
            assert_eq!(used_in_count, 1);
            assert_eq!(sample_action_names, vec!["blocking action"]);
        }
        other => panic!("expected ReferenceBlock, got: {other:?}"),
    }
}

#[tokio::test]
async fn delete_succeeds_after_action_removed() {
    let backend = setup().await;
    let repo = backend.trigger_instance_repo();
    let action_id = insert_action(&backend, "temporary action").await;
    let inst = make_instance("twitch.chat.command", "Unblocked", true);
    repo.save(&inst).await.expect("save");
    backend
        .insert_action_trigger_instance_for_test(action_id, inst.id, 0)
        .await
        .expect("join");

    backend
        .action_repo()
        .delete(action_id)
        .await
        .expect("delete action");

    assert!(
        repo.delete(inst.id)
            .await
            .expect("delete after action removed")
    );
}

#[tokio::test]
async fn set_enabled_toggles_flag() {
    let backend = setup().await;
    let repo = backend.trigger_instance_repo();
    let inst = make_instance("twitch.chat.command", "Toggle Me", true);
    let id = inst.id;
    repo.save(&inst).await.expect("save");

    repo.set_enabled(id, false).await.expect("disable");
    let got = repo.get(id).await.expect("get after disable").unwrap();
    assert!(!got.enabled);

    repo.set_enabled(id, true).await.expect("enable");
    let got = repo.get(id).await.expect("get after enable").unwrap();
    assert!(got.enabled);
}

#[tokio::test]
async fn set_enabled_on_a_missing_instance_returns_not_found() {
    let backend = setup().await;

    let err = backend
        .trigger_instance_repo()
        .set_enabled(TriggerInstanceId::new(), true)
        .await
        .expect_err("a missing instance must not be silently ignored");

    assert!(matches!(err, StorageError::NotFound { .. }), "got {err:?}");
}

#[tokio::test]
async fn toggle_flips_the_flag_and_reports_the_new_state() {
    let backend = setup().await;
    let repo = backend.trigger_instance_repo();
    let inst = make_instance("twitch.chat.command", "Toggle Me", true);
    repo.save(&inst).await.expect("save");

    let first = repo.toggle_enabled(inst.id).await.expect("toggle off");
    let second = repo.toggle_enabled(inst.id).await.expect("toggle on");

    assert_eq!((first, second), (Some(false), Some(true)));
}

#[tokio::test]
async fn toggle_of_a_missing_or_archived_instance_changes_nothing() {
    let backend = setup().await;
    let repo = backend.trigger_instance_repo();
    let archived = make_instance("twitch.chat.command", "Gone", true);
    repo.save(&archived).await.expect("save");
    repo.archive(archived.id).await.expect("archive");

    for id in [TriggerInstanceId::new(), archived.id] {
        assert_eq!(repo.toggle_enabled(id).await.expect("toggle"), None);
    }
}

#[tokio::test]
async fn parallel_toggles_each_flip_the_flag_exactly_once() {
    const TOGGLES: usize = 40;
    let backend = setup().await;
    let repo = backend.trigger_instance_repo();
    let inst = make_instance("twitch.chat.command", "Busy", true);
    repo.save(&inst).await.expect("save");

    let toggles: Vec<_> = (0..TOGGLES)
        .map(|_| {
            let repo = std::sync::Arc::clone(&repo);
            tokio::spawn(async move { repo.toggle_enabled(inst.id).await })
        })
        .collect();
    let mut turned_on = 0;
    for toggle in toggles {
        if toggle.await.expect("join").expect("toggle") == Some(true) {
            turned_on += 1;
        }
    }

    assert_eq!(turned_on, TOGGLES / 2);
    let stored = repo.get(inst.id).await.expect("get").expect("instance");
    assert!(
        stored.enabled,
        "an even number of toggles ends where it began"
    );
}

#[tokio::test]
async fn upsert_default_inserts_on_first_call() {
    let backend = setup().await;
    let repo = backend.trigger_instance_repo();
    let id = repo
        .upsert_default("twitch.chat.message", "Chat Message")
        .await
        .expect("upsert_default");
    let got = repo.get(id).await.expect("get").unwrap();
    assert_eq!(got.kind_id, "twitch.chat.message");
    assert!(!got.user_defined);
    assert!(got.enabled);
}

#[tokio::test]
async fn upsert_default_is_idempotent() {
    let backend = setup().await;
    let repo = backend.trigger_instance_repo();
    let id1 = repo
        .upsert_default("twitch.chat.message", "Chat Message")
        .await
        .expect("first upsert");
    let id2 = repo
        .upsert_default("twitch.chat.message", "Chat Message Again")
        .await
        .expect("second upsert");
    assert_eq!(
        id1, id2,
        "idempotent: same id must be returned on every call"
    );
}

#[tokio::test]
async fn upsert_default_allows_multiple_kinds() {
    let backend = setup().await;
    let repo = backend.trigger_instance_repo();
    let id_a = repo
        .upsert_default("twitch.chat.message", "Chat")
        .await
        .expect("upsert a");
    let id_b = repo
        .upsert_default("twitch.support.cheer", "Cheer")
        .await
        .expect("upsert b");
    assert_ne!(id_a, id_b);
}

#[tokio::test]
async fn reference_block_sample_names_capped_at_three() {
    let backend = setup().await;
    let repo = backend.trigger_instance_repo();
    let inst = make_instance("twitch.chat.command", "Many Refs", true);
    repo.save(&inst).await.expect("save");

    for i in 0..5u32 {
        let action_id = insert_action(&backend, &format!("action {i}")).await;
        backend
            .insert_action_trigger_instance_for_test(action_id, inst.id, 0)
            .await
            .expect("join");
    }

    let err = repo.delete(inst.id).await.expect_err("should fail");
    match err {
        StorageError::ReferenceBlock {
            used_in_count,
            sample_action_names,
        } => {
            assert_eq!(used_in_count, 5);
            assert!(
                sample_action_names.len() <= 3,
                "sample names must not exceed 3"
            );
        }
        other => panic!("expected ReferenceBlock, got: {other:?}"),
    }
}

#[tokio::test]
async fn link_action_inserts_row() {
    let backend = setup().await;
    let repo = backend.trigger_instance_repo();
    let action_id = insert_action(&backend, "link test action").await;
    let inst = make_instance("twitch.chat.command", "Link Me", true);
    repo.save(&inst).await.expect("save");

    repo.link_action(action_id, inst.id)
        .await
        .expect("link_action");

    let list = repo.list_for_action(action_id).await.expect("list");
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, inst.id);
}

#[tokio::test]
async fn duplicate_links_are_ignored_and_keep_the_original_position() {
    let backend = setup().await;
    let repo = backend.trigger_instance_repo();
    let action_id = insert_action(&backend, "idempotent action").await;
    let [first, second] = saved_instances_in_id_order(&backend).await;
    repo.link_action(action_id, first)
        .await
        .expect("link first");
    repo.link_action(action_id, second)
        .await
        .expect("link second");

    repo.link_action(action_id, first)
        .await
        .expect("an appended duplicate must succeed");
    repo.link_action_at(action_id, first, 99)
        .await
        .expect("a positioned duplicate must succeed");

    assert_eq!(linked_ids(&backend, action_id).await, vec![first, second]);
}

#[tokio::test]
async fn link_action_appends_after_the_highest_position_left_by_an_unlink() {
    let backend = setup().await;
    let repo = backend.trigger_instance_repo();
    let action_id = insert_action(&backend, "gapped action").await;
    let [late, first, middle, last] = saved_instances_in_id_order(&backend).await;
    for instance in [first, middle, last] {
        repo.link_action(action_id, instance).await.expect("link");
    }
    repo.unlink_action(action_id, middle).await.expect("unlink");

    repo.link_action(action_id, late).await.expect("relink");

    assert_eq!(
        linked_ids(&backend, action_id).await,
        vec![first, last, late],
        "the new link must not reuse the position of the last surviving link"
    );
}

#[tokio::test]
async fn link_action_counts_archived_links_when_choosing_the_next_position() {
    let backend = setup().await;
    let repo = backend.trigger_instance_repo();
    let action_id = insert_action(&backend, "archive action").await;
    let [late, first, archived] = saved_instances_in_id_order(&backend).await;
    repo.link_action(action_id, first)
        .await
        .expect("link first");
    repo.link_action(action_id, archived)
        .await
        .expect("link archived");
    assert!(repo.archive(archived).await.expect("archive"));

    repo.link_action(action_id, late).await.expect("link late");
    assert!(repo.restore(archived).await.expect("restore"));

    assert_eq!(
        linked_ids(&backend, action_id).await,
        vec![first, archived, late],
        "a restored link must keep its slot ahead of links added while it was archived"
    );
}

#[tokio::test]
async fn link_action_at_places_the_instance_at_the_given_position() {
    let backend = setup().await;
    let repo = backend.trigger_instance_repo();
    let action_id = insert_action(&backend, "restore action").await;
    let [first, second, third] = saved_instances_in_id_order(&backend).await;
    for instance in [first, second, third] {
        repo.link_action(action_id, instance).await.expect("link");
    }
    repo.unlink_action(action_id, first).await.expect("unlink");

    repo.link_action_at(action_id, first, 0)
        .await
        .expect("relink at position");

    assert_eq!(
        linked_ids(&backend, action_id).await,
        vec![first, second, third]
    );
}

#[tokio::test]
async fn list_for_action_orders_equal_positions_by_instance_id() {
    let backend = setup().await;
    let repo = backend.trigger_instance_repo();
    let action_id = insert_action(&backend, "tied action").await;
    let [lower, higher] = saved_instances_in_id_order(&backend).await;
    repo.link_action_at(action_id, higher, 0)
        .await
        .expect("link higher");
    repo.link_action_at(action_id, lower, 0)
        .await
        .expect("link lower");

    assert_eq!(linked_ids(&backend, action_id).await, vec![lower, higher]);
}

#[tokio::test]
async fn unlink_action_removes_row_returns_true() {
    let backend = setup().await;
    let repo = backend.trigger_instance_repo();
    let action_id = insert_action(&backend, "unlink action").await;
    let inst = make_instance("twitch.chat.command", "Unlink Me", true);
    repo.save(&inst).await.expect("save");

    repo.link_action(action_id, inst.id).await.expect("link");
    let removed = repo
        .unlink_action(action_id, inst.id)
        .await
        .expect("unlink");
    assert!(removed);

    let list = repo
        .list_for_action(action_id)
        .await
        .expect("list after unlink");
    assert!(list.is_empty());
}

#[tokio::test]
async fn unlink_action_returns_false_when_no_row() {
    let backend = setup().await;
    let repo = backend.trigger_instance_repo();
    let action_id = insert_action(&backend, "ghost action").await;
    let inst = make_instance("twitch.chat.command", "Ghost", true);
    repo.save(&inst).await.expect("save");

    let removed = repo
        .unlink_action(action_id, inst.id)
        .await
        .expect("unlink of absent row must not error");
    assert!(!removed);
}

#[tokio::test]
async fn list_all_empty_table_returns_empty_vec() {
    let backend = setup().await;
    let repo = backend.trigger_instance_repo();
    let all = repo.list_all().await.expect("list_all");
    assert!(all.is_empty());
}

#[tokio::test]
async fn list_all_returns_default_and_user_defined_ordered() {
    let backend = setup().await;
    let repo = backend.trigger_instance_repo();

    let default_id = repo
        .upsert_default("twitch.chat.message", "Chat Message")
        .await
        .expect("upsert_default");

    let user_inst = make_instance("twitch.support.cheer", "Cheer Trigger", true);
    let user_id = user_inst.id;
    repo.save(&user_inst).await.expect("save user-defined");

    let all = repo.list_all().await.expect("list_all");
    assert_eq!(all.len(), 2);
    assert_eq!(all[0].id, default_id, "default instance must come first");
    assert!(!all[0].user_defined);
    assert_eq!(all[1].id, user_id, "user-defined instance must come second");
    assert!(all[1].user_defined);
}

#[tokio::test]
async fn platform_scope_default_round_trips_as_any() {
    let backend = setup().await;
    let repo = backend.trigger_instance_repo();
    let inst = make_instance("twitch.chat.message", "Scope Any", true);
    assert_eq!(inst.platform_scope, PlatformScope::Any);
    let id = inst.id;
    repo.save(&inst).await.expect("save");
    let got = repo.get(id).await.expect("get").unwrap();
    assert_eq!(got.platform_scope, PlatformScope::Any);
}

#[tokio::test]
async fn platform_scope_only_subset_round_trips() {
    let backend = setup().await;
    let repo = backend.trigger_instance_repo();
    let mut set = BTreeSet::new();
    set.insert(PlatformId::Twitch);
    let scope = PlatformScope::Only(set);
    let inst = TriggerInstance {
        id: TriggerInstanceId::new(),
        kind_id: "twitch.chat.message".to_owned(),
        name: "Twitch Only".to_owned(),
        overrides: BTreeMap::new(),
        enabled: true,
        user_defined: true,
        platform_scope: scope.clone(),
        cooldown_secs: 0,
        cooldown_global: true,
        permission_rung: PermissionRung::Everyone,
    };
    let id = inst.id;
    repo.save(&inst).await.expect("save");
    let got = repo.get(id).await.expect("get").unwrap();
    assert_eq!(got.platform_scope, scope);
}

#[tokio::test]
async fn every_permission_rung_survives_a_save_and_get_roundtrip() {
    let backend = setup().await;
    let repo = backend.trigger_instance_repo();

    for rung in [
        PermissionRung::Everyone,
        PermissionRung::Subscriber,
        PermissionRung::Vip,
        PermissionRung::Moderator,
        PermissionRung::Broadcaster,
    ] {
        let mut inst = make_instance("twitch.chat.command", "Gated", true);
        inst.permission_rung = rung;
        let id = inst.id;
        repo.save(&inst).await.expect("save");
        let got = repo.get(id).await.expect("get").expect("row must exist");
        assert_eq!(got.permission_rung, rung);
    }
}

#[tokio::test]
async fn legacy_rows_without_a_permission_rung_read_as_everyone() {
    let backend = setup().await;
    let repo = backend.trigger_instance_repo();

    let instance_id = TriggerInstanceId::new();
    backend
        .insert_trigger_instance_without_scope_for_test(
            &instance_id.to_string(),
            "twitch.chat.command",
            "Legacy",
        )
        .await
        .expect("raw insert without permission_rung column");

    let got = repo
        .get(instance_id)
        .await
        .expect("get")
        .expect("row must exist");
    assert_eq!(
        got.permission_rung,
        PermissionRung::Everyone,
        "the backfill default must keep every pre-existing trigger open to everyone"
    );
}

#[tokio::test]
async fn migration_applies_any_default_to_legacy_rows() {
    let backend = setup().await;
    let repo = backend.trigger_instance_repo();

    let instance_id = TriggerInstanceId::new();
    backend
        .insert_trigger_instance_without_scope_for_test(
            &instance_id.to_string(),
            "core.timer",
            "Timer",
        )
        .await
        .expect("raw insert without platform_scope column");

    let got = repo
        .get(instance_id)
        .await
        .expect("get")
        .expect("row must exist");
    assert_eq!(
        got.platform_scope,
        PlatformScope::Any,
        "DEFAULT '\"any\"' must decode as PlatformScope::Any"
    );
}

async fn instance_on(
    backend: &SqliteBackend,
    action_id: ActionId,
    name: &str,
) -> TriggerInstanceId {
    let instance = make_instance("twitch.chat.command", name, true);
    let repo = backend.trigger_instance_repo();
    repo.save(&instance).await.expect("save instance");
    repo.link_action(action_id, instance.id)
        .await
        .expect("link instance");
    instance.id
}

#[tokio::test]
async fn a_change_set_failing_on_its_last_write_leaves_every_earlier_write_undone() {
    let backend = setup().await;
    let repo = backend.trigger_instance_repo();
    let action_id = insert_action(&backend, "Alert").await;
    let removed = instance_on(&backend, action_id, "Removed").await;
    let unlinked = instance_on(&backend, action_id, "Unlinked").await;
    let fresh = make_instance("twitch.chat.command", "Fresh", true);

    let outcome = repo
        .apply_changes(&TriggerChanges {
            removed: vec![removed],
            unlinked: vec![(action_id, unlinked)],
            saved: vec![fresh.clone()],
            linked: vec![(ActionId::new(), fresh.id)],
        })
        .await;

    assert!(outcome.is_err(), "got {outcome:?}");
    assert!(repo.get(removed).await.expect("get").is_some());
    assert!(repo.get(fresh.id).await.expect("get").is_none());
    assert_eq!(linked_ids(&backend, action_id).await, [removed, unlinked]);
}

#[tokio::test]
async fn a_change_set_can_link_an_instance_it_saves_in_the_same_write() {
    let backend = setup().await;
    let repo = backend.trigger_instance_repo();
    let action_id = insert_action(&backend, "Alert").await;
    let existing = instance_on(&backend, action_id, "Existing").await;
    let fresh = make_instance("twitch.chat.command", "Fresh", true);

    repo.apply_changes(&TriggerChanges {
        saved: vec![fresh.clone()],
        linked: vec![(action_id, fresh.id)],
        ..TriggerChanges::default()
    })
    .await
    .expect("apply");

    assert_eq!(linked_ids(&backend, action_id).await, [existing, fresh.id]);
}

#[tokio::test]
async fn a_change_set_removes_before_it_saves_so_one_id_can_be_replaced() {
    let backend = setup().await;
    let repo = backend.trigger_instance_repo();
    let action_id = insert_action(&backend, "Alert").await;
    let instance = make_instance("twitch.chat.command", "Before", true);
    repo.save(&instance).await.expect("save");
    repo.link_action(action_id, instance.id)
        .await
        .expect("link");
    let replacement = TriggerInstance {
        name: "After".to_owned(),
        ..instance.clone()
    };

    repo.apply_changes(&TriggerChanges {
        removed: vec![instance.id],
        saved: vec![replacement],
        ..TriggerChanges::default()
    })
    .await
    .expect("apply");

    let stored = repo.get(instance.id).await.expect("get").expect("stored");
    assert_eq!(stored.name, "After");
    assert!(linked_ids(&backend, action_id).await.is_empty());
}

#[tokio::test]
async fn a_change_set_unlinks_before_it_links_so_a_pair_moves_to_the_end() {
    let backend = setup().await;
    let repo = backend.trigger_instance_repo();
    let action_id = insert_action(&backend, "Alert").await;
    let first = instance_on(&backend, action_id, "First").await;
    let second = instance_on(&backend, action_id, "Second").await;

    repo.apply_changes(&TriggerChanges {
        unlinked: vec![(action_id, first)],
        linked: vec![(action_id, first)],
        ..TriggerChanges::default()
    })
    .await
    .expect("apply");

    assert_eq!(linked_ids(&backend, action_id).await, [second, first]);
}

#[tokio::test]
async fn repeating_a_create_change_set_keeps_one_row_and_one_link() {
    let backend = setup().await;
    let repo = backend.trigger_instance_repo();
    let action_id = insert_action(&backend, "Alert").await;
    let fresh = make_instance("twitch.chat.command", "Fresh", true);
    let create = TriggerChanges {
        saved: vec![fresh.clone()],
        linked: vec![(action_id, fresh.id)],
        ..TriggerChanges::default()
    };

    repo.apply_changes(&create).await.expect("first apply");
    repo.apply_changes(&create).await.expect("second apply");

    assert_eq!(linked_ids(&backend, action_id).await, [fresh.id]);
    assert_eq!(repo.list_all().await.expect("list").len(), 1);
}

#[tokio::test]
async fn a_change_set_removes_an_instance_a_live_and_an_archived_action_still_link() {
    let backend = setup().await;
    let repo = backend.trigger_instance_repo();
    let live = insert_action(&backend, "Live").await;
    let archived = insert_action(&backend, "Archived").await;
    let instance = instance_on(&backend, live, "Shared").await;
    repo.link_action(archived, instance).await.expect("link");
    backend
        .action_repo()
        .archive(archived)
        .await
        .expect("archive");

    repo.apply_changes(&TriggerChanges {
        removed: vec![instance],
        ..TriggerChanges::default()
    })
    .await
    .expect("apply");

    assert!(repo.get(instance).await.expect("get").is_none());
    assert!(linked_ids(&backend, live).await.is_empty());
}

#[tokio::test]
async fn the_link_map_groups_actions_by_instance_and_leaves_unlinked_instances_out() {
    let backend = setup().await;
    let repo = backend.trigger_instance_repo();
    let first = insert_action(&backend, "First").await;
    let second = insert_action(&backend, "Second").await;
    let shared = instance_on(&backend, first, "Shared").await;
    repo.link_action(second, shared).await.expect("link");
    let single = instance_on(&backend, second, "Single").await;
    let unlinked = make_instance("twitch.chat.command", "Unlinked", true);
    repo.save(&unlinked).await.expect("save");

    let links = repo.actions_by_instance().await.expect("links");

    let sets: BTreeMap<TriggerInstanceId, BTreeSet<ActionId>> = links
        .into_iter()
        .map(|(instance, actions)| (instance, actions.into_iter().collect()))
        .collect();
    assert_eq!(
        sets,
        BTreeMap::from([
            (shared, BTreeSet::from([first, second])),
            (single, BTreeSet::from([second])),
        ])
    );
}

#[tokio::test]
async fn the_link_map_ignores_archived_actions_like_the_per_instance_read() {
    let backend = setup().await;
    let repo = backend.trigger_instance_repo();
    let live = insert_action(&backend, "Live").await;
    let archived = insert_action(&backend, "Archived").await;
    let shared = instance_on(&backend, live, "Shared").await;
    repo.link_action(archived, shared).await.expect("link");
    let orphaned = instance_on(&backend, archived, "Orphaned").await;
    backend
        .action_repo()
        .archive(archived)
        .await
        .expect("archive");

    let links = repo.actions_by_instance().await.expect("links");

    assert_eq!(links.get(&shared).map(Vec::as_slice), Some(&[live][..]));
    assert!(!links.contains_key(&orphaned));
}
