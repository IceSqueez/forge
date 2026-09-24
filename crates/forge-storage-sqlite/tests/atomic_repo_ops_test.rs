#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::sync::Arc;

use forge_storage::{DataProvider, GlobalsRepo, StorageError, UserGlobalsRepo};
use forge_storage_sqlite::SqliteBackend;
use forge_types::{Action, ActionId, ExecutionMode, SubActionConfig, SubActionStep, Variant};

mod common;
use common::{Sandboxed, TEST_KEY};

const WRITERS: usize = 40;

async fn file_backed() -> (tempfile::TempDir, Sandboxed<Arc<SqliteBackend>>) {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let url = format!("sqlite://{}", dir.path().join("atomic.sqlite").display());
    let backend = common::sandboxed_backend(&url, TEST_KEY)
        .await
        .map(Arc::new);
    (dir, backend)
}

fn globals(backend: &SqliteBackend) -> &dyn GlobalsRepo {
    backend
}

fn users(backend: &SqliteBackend) -> &dyn UserGlobalsRepo {
    backend
}

fn ints(values: &[i64]) -> Variant {
    Variant::Array(values.iter().copied().map(Variant::Int).collect())
}

fn type_mismatch_of(err: &StorageError) -> Option<&str> {
    match err {
        StorageError::TypeMismatch { actual, .. } => Some(actual.as_str()),
        _ => None,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_array_appends_keep_every_item() {
    let (_dir, backend) = file_backed().await;
    let handles: Vec<_> = (0..WRITERS as i64)
        .map(|i| {
            let b = Arc::clone(&backend);
            tokio::spawn(async move {
                globals(&b)
                    .array_append("queue", Variant::Int(i), None)
                    .await
                    .expect("append")
            })
        })
        .collect();
    let mut lengths = Vec::new();
    for h in handles {
        lengths.push(h.await.expect("task"));
    }

    lengths.sort_unstable();
    assert_eq!(lengths, (1..=WRITERS).collect::<Vec<_>>());
    let Some(Variant::Array(items)) = globals(&backend).get("queue").await.expect("get") else {
        panic!("queue must be an array");
    };
    let mut got: Vec<i64> = items
        .iter()
        .map(|v| match v {
            Variant::Int(i) => *i,
            other => panic!("unexpected item {other:?}"),
        })
        .collect();
    got.sort_unstable();
    assert_eq!(got, (0..WRITERS as i64).collect::<Vec<_>>());
}

#[tokio::test]
async fn bounded_array_append_drops_the_oldest_items_past_the_limit() {
    let (_dir, backend) = file_backed().await;
    for (max_len, expected) in [
        (Some(4), vec![1, 2, 3, 4]),
        (Some(3), vec![2, 3, 4]),
        (Some(2), vec![3, 4]),
        (Some(1), vec![4]),
        (None, vec![1, 2, 3, 4]),
    ] {
        globals(&backend)
            .set("q", ints(&[1, 2, 3]), false)
            .await
            .expect("seed");
        let len = globals(&backend)
            .array_append("q", Variant::Int(4), max_len)
            .await
            .expect("append");
        assert_eq!(len, expected.len(), "max_len {max_len:?}");
        assert_eq!(
            globals(&backend).get("q").await.expect("get"),
            Some(ints(&expected)),
            "max_len {max_len:?}"
        );
    }
}

#[tokio::test]
async fn array_append_creates_a_session_array_and_keeps_an_existing_persisted_flag() {
    let (_dir, backend) = file_backed().await;
    globals(&backend)
        .array_append("fresh", Variant::Int(1), None)
        .await
        .expect("append fresh");
    assert_eq!(
        globals(&backend).get("fresh").await.expect("get"),
        Some(ints(&[1]))
    );
    assert_eq!(
        globals(&backend).persisted("fresh").await.expect("flag"),
        Some(false)
    );

    globals(&backend)
        .set("kept", ints(&[1]), true)
        .await
        .expect("seed");
    globals(&backend)
        .array_append("kept", Variant::Int(2), None)
        .await
        .expect("append kept");
    assert_eq!(
        globals(&backend).persisted("kept").await.expect("flag"),
        Some(true)
    );
}

#[tokio::test]
async fn array_ops_reject_a_non_array_global_without_touching_it() {
    let (_dir, backend) = file_backed().await;
    globals(&backend)
        .set("s", Variant::String("text".to_owned()), false)
        .await
        .expect("seed");
    let expected_tag = Variant::String(String::new()).type_tag().to_string();

    let append = globals(&backend)
        .array_append("s", Variant::Int(1), None)
        .await
        .unwrap_err();
    let remove = globals(&backend)
        .array_remove("s", &Variant::Int(1), false)
        .await
        .unwrap_err();

    assert_eq!(
        type_mismatch_of(&append),
        Some(expected_tag.as_str()),
        "{append:?}"
    );
    assert_eq!(
        type_mismatch_of(&remove),
        Some(expected_tag.as_str()),
        "{remove:?}"
    );
    assert_eq!(
        globals(&backend).get("s").await.expect("get"),
        Some(Variant::String("text".to_owned()))
    );
}

#[tokio::test]
async fn array_remove_drops_the_first_or_every_match() {
    let (_dir, backend) = file_backed().await;
    for (remove_all, expected) in [(false, vec![2, 1, 1]), (true, vec![2])] {
        globals(&backend)
            .set("a", ints(&[1, 2, 1, 1]), false)
            .await
            .expect("seed");
        let len = globals(&backend)
            .array_remove("a", &Variant::Int(1), remove_all)
            .await
            .expect("remove");
        assert_eq!(len, expected.len(), "remove_all {remove_all}");
        assert_eq!(
            globals(&backend).get("a").await.expect("get"),
            Some(ints(&expected))
        );
    }
}

#[tokio::test]
async fn array_remove_and_toggle_of_an_absent_global_are_not_found() {
    let (_dir, backend) = file_backed().await;
    let remove = globals(&backend)
        .array_remove("ghost", &Variant::Int(1), false)
        .await
        .unwrap_err();
    let toggle = globals(&backend).toggle("ghost").await.unwrap_err();
    assert!(
        matches!(remove, StorageError::NotFound { .. }),
        "{remove:?}"
    );
    assert!(
        matches!(toggle, StorageError::NotFound { .. }),
        "{toggle:?}"
    );
    assert_eq!(globals(&backend).get("ghost").await.expect("get"), None);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_array_removes_each_take_their_own_item() {
    let (_dir, backend) = file_backed().await;
    let all: Vec<i64> = (0..WRITERS as i64).collect();
    globals(&backend)
        .set("pool", ints(&all), false)
        .await
        .expect("seed");

    let handles: Vec<_> = all
        .iter()
        .map(|i| {
            let b = Arc::clone(&backend);
            let item = Variant::Int(*i);
            tokio::spawn(async move {
                globals(&b)
                    .array_remove("pool", &item, false)
                    .await
                    .expect("remove")
            })
        })
        .collect();
    for h in handles {
        h.await.expect("task");
    }

    assert_eq!(
        globals(&backend).get("pool").await.expect("get"),
        Some(ints(&[]))
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_even_number_of_concurrent_toggles_restores_the_original_value() {
    let (_dir, backend) = file_backed().await;
    globals(&backend)
        .set("flag", Variant::Bool(false), true)
        .await
        .expect("seed");

    let handles: Vec<_> = (0..WRITERS)
        .map(|_| {
            let b = Arc::clone(&backend);
            tokio::spawn(async move { globals(&b).toggle("flag").await.expect("toggle") })
        })
        .collect();
    let mut turned_on = 0;
    for h in handles {
        if h.await.expect("task") {
            turned_on += 1;
        }
    }

    assert_eq!(
        turned_on,
        WRITERS / 2,
        "each toggle must see the previous one"
    );
    assert_eq!(
        globals(&backend).get("flag").await.expect("get"),
        Some(Variant::Bool(false))
    );
    assert_eq!(
        globals(&backend).persisted("flag").await.expect("flag"),
        Some(true)
    );
}

#[tokio::test]
async fn toggle_rejects_a_non_bool_global_naming_its_type() {
    let (_dir, backend) = file_backed().await;
    globals(&backend)
        .set("n", Variant::Int(1), false)
        .await
        .expect("seed");
    let err = globals(&backend).toggle("n").await.unwrap_err();
    let expected_tag = Variant::Int(0).type_tag().to_string();
    assert_eq!(
        type_mismatch_of(&err),
        Some(expected_tag.as_str()),
        "{err:?}"
    );
    assert_eq!(
        globals(&backend).get("n").await.expect("get"),
        Some(Variant::Int(1))
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_user_variable_increments_sum_exactly() {
    let (_dir, backend) = file_backed().await;
    let handles: Vec<_> = (0..WRITERS)
        .map(|_| {
            let b = Arc::clone(&backend);
            tokio::spawn(async move {
                users(&b)
                    .incr("bc", "viewer", "points", 5)
                    .await
                    .expect("incr")
            })
        })
        .collect();
    for h in handles {
        h.await.expect("task");
    }

    let got = users(&backend)
        .get("bc", "viewer", "points")
        .await
        .expect("get");
    assert_eq!(got, Some(Variant::Int(5 * WRITERS as i64)));
}

#[tokio::test]
async fn user_variable_increment_by_stored_kind() {
    let (_dir, backend) = file_backed().await;
    for (seed, amount, expected) in [
        (None, 3, Variant::Int(3)),
        (Some(Variant::Int(i64::MAX - 1)), 5, Variant::Int(i64::MAX)),
        (Some(Variant::Int(i64::MIN + 1)), -5, Variant::Int(i64::MIN)),
        (Some(Variant::Float(1.5)), 2, Variant::Float(3.5)),
    ] {
        users(&backend).delete("bc", "u", "v").await.expect("reset");
        if let Some(seed) = seed.clone() {
            users(&backend)
                .set("bc", "u", "v", seed)
                .await
                .expect("seed");
        }
        let got = users(&backend)
            .incr("bc", "u", "v", amount)
            .await
            .expect("incr");
        assert_eq!(got, expected, "{seed:?} + {amount}");
        assert_eq!(
            users(&backend).get("bc", "u", "v").await.expect("get"),
            Some(expected),
        );
    }
}

#[tokio::test]
async fn user_variable_increment_rejects_a_non_numeric_value_without_touching_it() {
    let (_dir, backend) = file_backed().await;
    let text = Variant::String("gold".to_owned());
    users(&backend)
        .set("bc", "u", "rank", text.clone())
        .await
        .expect("seed");
    let err = users(&backend)
        .incr("bc", "u", "rank", 1)
        .await
        .unwrap_err();
    assert!(type_mismatch_of(&err).is_some(), "{err:?}");
    assert_eq!(
        users(&backend).get("bc", "u", "rank").await.expect("get"),
        Some(text)
    );
}

async fn seeded_action(backend: &SqliteBackend) -> Action {
    let queue_id = backend
        .queue_repo()
        .get_by_name("Default")
        .await
        .expect("queue lookup")
        .expect("default queue")
        .id;
    let action = Action {
        id: ActionId::new(),
        name: "original".to_owned(),
        group: None,
        queue_id,
        enabled: true,
        concurrent: false,
        bypass_pause: false,
        execution_mode: ExecutionMode::Sequential,
        description: None,
        sub_actions: vec![],
    };
    backend.action_repo().save(&action).await.expect("save");
    action
}

fn edited(action: &Action, round: usize) -> Action {
    let mut next = action.clone();
    next.name = format!("edited {round}");
    next.sub_actions = vec![SubActionStep {
        kind_id: format!("core.log.write.{round}"),
        config: SubActionConfig::new(),
        enabled: true,
        continue_on_error: false,
        condition: None,
        label: None,
    }];
    next
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn enabled_flag_writes_never_clobber_a_concurrent_editor_save() {
    let (_dir, backend) = file_backed().await;
    let action = seeded_action(&backend).await;
    let repo = backend.action_repo();

    for round in 0..WRITERS {
        let edit = edited(&action, round);
        let (toggle_repo, set_repo, save_repo) = (repo.clone(), repo.clone(), repo.clone());
        let id = action.id;
        let (toggled, set, saved) = tokio::join!(
            tokio::spawn(async move { toggle_repo.toggle_enabled(id).await }),
            tokio::spawn(async move { set_repo.set_enabled(id, round % 2 == 0).await }),
            tokio::spawn(async move { save_repo.save(&edit).await }),
        );
        toggled.expect("task").expect("toggle");
        set.expect("task").expect("set");
        saved.expect("task").expect("save");

        let stored = repo.get(action.id).await.expect("get").expect("row");
        assert_eq!(stored.name, format!("edited {round}"), "round {round}");
        assert_eq!(
            stored.sub_actions[0].kind_id,
            format!("core.log.write.{round}"),
            "round {round}"
        );
    }
}

#[tokio::test]
async fn enabled_flag_writes_report_a_missing_or_archived_action() {
    let (_dir, backend) = file_backed().await;
    let repo = backend.action_repo();
    let archived = seeded_action(&backend).await;
    assert!(repo.archive(archived.id).await.expect("archive"));

    for id in [ActionId::new(), archived.id] {
        assert_eq!(repo.toggle_enabled(id).await.expect("toggle"), None);
        assert!(!repo.set_enabled(id, false).await.expect("set"));
    }
    let stored = repo.list_archived().await.expect("archived");
    assert!(stored.iter().any(|a| a.id == archived.id && a.enabled));
}

#[tokio::test]
async fn enabled_flag_writes_return_the_stored_state() {
    let (_dir, backend) = file_backed().await;
    let repo = backend.action_repo();
    let action = seeded_action(&backend).await;

    assert_eq!(
        repo.toggle_enabled(action.id).await.expect("toggle"),
        Some(false)
    );
    assert!(
        !repo
            .get(action.id)
            .await
            .expect("get")
            .expect("row")
            .enabled
    );
    assert_eq!(
        repo.toggle_enabled(action.id).await.expect("toggle"),
        Some(true)
    );
    assert!(repo.set_enabled(action.id, false).await.expect("set"));
    assert!(repo.set_enabled(action.id, false).await.expect("set again"));
    assert!(
        !repo
            .get(action.id)
            .await
            .expect("get")
            .expect("row")
            .enabled
    );
}
