#![allow(clippy::expect_used)]

use forge_storage::{DataProvider, GlobalsRepo};
use forge_types::Variant;

mod common;

const TEST_KEY: [u8; 32] = [0xab; 32];

#[tokio::test]
async fn schema_version_is_at_least_2_after_all_migrations() {
    let backend = common::sandboxed_backend("sqlite::memory:", TEST_KEY).await;

    let version = backend.schema_version().await.expect("schema_version");
    assert!(version >= 2, "expected at least 2 applied migrations");
}

#[tokio::test]
async fn dataprovider_action_repo_accessor_is_reachable() {
    use forge_types::{Action, ActionId};

    let backend = common::sandboxed_backend("sqlite::memory:", TEST_KEY).await;

    let dp: &dyn DataProvider = &*backend;

    let queue = dp
        .queue_repo()
        .get_by_name("Default")
        .await
        .expect("get default queue");
    let queue_id = queue.expect("default queue seeded").id;

    let action = Action {
        id: ActionId::new(),
        name: "test_action".to_owned(),
        group: None,
        queue_id,
        enabled: true,
        concurrent: false,
        bypass_pause: false,
        execution_mode: forge_types::ExecutionMode::Sequential,
        description: None,
        sub_actions: vec![],
    };
    dp.action_repo()
        .save(&action)
        .await
        .expect("save must succeed");

    let got = dp
        .action_repo()
        .get(action.id)
        .await
        .expect("get must succeed");
    let action = got.expect("action must exist");
    assert_eq!(action.name, "test_action");
}

#[tokio::test]
async fn dataprovider_globals_repo_roundtrip() {
    let backend = common::sandboxed_backend("sqlite::memory:", TEST_KEY).await;

    let dp: &dyn DataProvider = &*backend;

    GlobalsRepo::set(dp, "round_trip", Variant::Int(42), false)
        .await
        .expect("set must succeed");

    let got = GlobalsRepo::get(dp, "round_trip")
        .await
        .expect("get must succeed");
    assert_eq!(got, Some(Variant::Int(42)));
}

#[tokio::test]
async fn export_writes_a_non_empty_file() {
    let dir = tempfile::tempdir().expect("a temporary database directory");
    let source_path = dir.path().join("source.sqlite");
    let export_path = dir.path().join("export.sqlite");

    let url = format!("sqlite://{}", source_path.display());
    let backend = common::sandboxed_backend(&url, TEST_KEY).await;

    backend
        .export(&export_path)
        .await
        .expect("export must succeed");

    let meta = std::fs::metadata(&export_path).expect("export file must exist");
    assert!(meta.len() > 0, "export file must be non-empty");
}

#[tokio::test]
async fn shutdown_closes_the_pool_so_later_queries_fail_instead_of_hanging() {
    use forge_storage::SettingsRepo;

    let backend = common::sandboxed_backend("sqlite::memory:", TEST_KEY).await;
    backend
        .set_string("theme", "catppuccin_mocha")
        .await
        .expect("write before shutdown succeeds");

    backend.shutdown().await;

    let result = backend.get_string("theme").await;
    assert!(
        result.is_err(),
        "queries after shutdown must error, not hang"
    );
}
