#![allow(clippy::expect_used)]

use forge_storage::{DataProvider, GlobalsRepo};
use forge_storage_sqlite::SqliteBackend;
use forge_types::Variant;

const TEST_KEY: [u8; 32] = [0xab; 32];

#[tokio::test]
async fn schema_version_is_at_least_2_after_all_migrations() {
    let backend = SqliteBackend::open_with_key("sqlite::memory:", TEST_KEY)
        .await
        .expect("open");

    let version = backend.schema_version().await.expect("schema_version");
    assert!(version >= 2, "expected at least 2 applied migrations");
}

#[tokio::test]
async fn dataprovider_action_repo_accessor_is_reachable() {
    use forge_types::{Action, ActionId};

    let backend = SqliteBackend::open_with_key("sqlite::memory:", TEST_KEY)
        .await
        .expect("open");

    let dp: &dyn DataProvider = &backend;

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
    let backend = SqliteBackend::open_with_key("sqlite::memory:", TEST_KEY)
        .await
        .expect("open");

    let dp: &dyn DataProvider = &backend;

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
    let pid = std::process::id();
    let source_path = std::env::temp_dir().join(format!("forge_source_{pid}.sqlite"));
    let export_path = std::env::temp_dir().join(format!("forge_export_{pid}.sqlite"));

    let url = format!("sqlite://{}", source_path.display());
    let backend = SqliteBackend::open_with_key(&url, TEST_KEY)
        .await
        .expect("open file-backed db");

    backend
        .export(&export_path)
        .await
        .expect("export must succeed");

    let meta = std::fs::metadata(&export_path).expect("export file must exist");
    assert!(meta.len() > 0, "export file must be non-empty");

    let _ = std::fs::remove_file(&source_path);
    let _ = std::fs::remove_file(source_path.with_extension("sqlite-wal"));
    let _ = std::fs::remove_file(source_path.with_extension("sqlite-shm"));
    let _ = std::fs::remove_file(&export_path);
}

#[tokio::test]
async fn shutdown_closes_the_pool_so_later_queries_fail_instead_of_hanging() {
    use forge_storage::SettingsRepo;

    let backend = SqliteBackend::open_with_key("sqlite::memory:", TEST_KEY)
        .await
        .expect("open");
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
