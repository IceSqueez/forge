#![allow(clippy::expect_used)]

use forge_storage::{DataProvider, GlobalsRepo};
use forge_types::Variant;

mod common;

const TEST_KEY: [u8; 32] = [0xab; 32];

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

fn sibling(path: &std::path::Path, suffix: &str) -> std::path::PathBuf {
    let mut name = path.file_name().expect("a file name").to_os_string();
    name.push(suffix);
    path.with_file_name(name)
}

#[tokio::test]
async fn export_produces_a_standalone_file_holding_writes_still_in_the_wal() {
    let dir = tempfile::tempdir().expect("a temporary database directory");
    let source_path = dir.path().join(forge_storage::DATABASE_FILE_NAME);
    let export_dir = tempfile::tempdir().expect("a temporary export directory");
    let export_path = export_dir.path().join("export.db");

    let backend =
        common::sandboxed_backend(&format!("sqlite://{}", source_path.display()), TEST_KEY).await;
    GlobalsRepo::set(&*backend, "exported", Variant::Int(7), true)
        .await
        .expect("set must succeed");
    let wal_len = std::fs::metadata(sibling(&source_path, "-wal"))
        .expect("the source WAL exists")
        .len();
    assert!(
        wal_len > 0,
        "the write must still sit in the WAL before export"
    );

    backend
        .export(&export_path)
        .await
        .expect("export must succeed");
    backend.shutdown().await;

    let exported_files: Vec<_> = std::fs::read_dir(export_dir.path())
        .expect("the export directory is readable")
        .map(|entry| entry.expect("a directory entry").file_name())
        .collect();
    assert_eq!(exported_files, vec![std::ffi::OsString::from("export.db")]);

    let reopened =
        common::sandboxed_backend(&format!("sqlite://{}", export_path.display()), TEST_KEY).await;
    let got = GlobalsRepo::get(&*reopened, "exported")
        .await
        .expect("get must succeed");
    assert_eq!(got, Some(Variant::Int(7)));
}

#[tokio::test]
async fn export_to_an_existing_file_fails_and_leaves_that_file_untouched() {
    let dir = tempfile::tempdir().expect("a temporary database directory");
    let source_path = dir.path().join(forge_storage::DATABASE_FILE_NAME);
    let export_path = dir.path().join("existing.db");
    let existing_bytes = b"an unrelated file the user already had".to_vec();
    std::fs::write(&export_path, &existing_bytes).expect("seed the existing file");

    let backend =
        common::sandboxed_backend(&format!("sqlite://{}", source_path.display()), TEST_KEY).await;
    let result = backend.export(&export_path).await;

    assert!(
        matches!(result, Err(forge_storage::StorageError::Connection { .. })),
        "export over an existing file must fail with a storage error, got {result:?}"
    );
    assert_eq!(
        std::fs::read(&export_path).expect("the existing file is readable"),
        existing_bytes
    );
}

#[tokio::test]
async fn schema_version_of_a_freshly_migrated_backend_is_the_expected_version() {
    let backend = common::sandboxed_backend("sqlite::memory:", TEST_KEY).await;

    let version = backend
        .schema_version()
        .await
        .expect("schema version must be readable");

    assert_eq!(version, forge_storage::EXPECTED_SCHEMA_VERSION);
}

#[tokio::test]
async fn database_unit_file_names_are_exactly_the_files_an_open_backend_keeps_on_disk() {
    let dir = tempfile::tempdir().expect("a temporary database directory");
    let db_path = dir.path().join(forge_storage::DATABASE_FILE_NAME);
    let backend =
        common::sandboxed_backend(&format!("sqlite://{}", db_path.display()), TEST_KEY).await;
    GlobalsRepo::set(&*backend, "touch", Variant::Bool(true), false)
        .await
        .expect("set must succeed");

    let mut on_disk: Vec<String> = std::fs::read_dir(dir.path())
        .expect("the database directory is readable")
        .map(|entry| {
            entry
                .expect("a directory entry")
                .file_name()
                .into_string()
                .expect("utf-8 file name")
        })
        .collect();
    on_disk.sort();
    let mut expected = forge_storage::database_unit_file_names().to_vec();
    expected.sort();

    assert_eq!(on_disk, expected);
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
