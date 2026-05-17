#![allow(clippy::expect_used)]

use loom_storage::{DataProvider, GlobalsRepo};
use loom_storage_sqlite::SqliteBackend;
use loom_types::Variant;

fn init_keyring() {
    keyring::use_sample_store(&std::collections::HashMap::new())
        .expect("sample keyring must initialize");
}

#[tokio::test]
async fn open_succeeds_with_in_memory_db() {
    init_keyring();
    SqliteBackend::open("sqlite::memory:")
        .await
        .expect("SqliteBackend::open must succeed on sqlite::memory:");
}

#[tokio::test]
async fn schema_version_returns_3_after_all_migrations() {
    init_keyring();
    let backend = SqliteBackend::open("sqlite::memory:").await.expect("open");

    let version = backend.schema_version().await.expect("schema_version");
    assert_eq!(version, 3, "expected 3 applied migrations");
}

#[tokio::test]
async fn dataprovider_coercion_compiles_and_delegates() {
    init_keyring();
    let backend = SqliteBackend::open("sqlite::memory:").await.expect("open");

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
    init_keyring();
    let pid = std::process::id();
    let source_path = std::env::temp_dir().join(format!("loom_source_{pid}.sqlite"));
    let export_path = std::env::temp_dir().join(format!("loom_export_{pid}.sqlite"));

    let url = format!("sqlite://{}", source_path.display());
    let backend = SqliteBackend::open(&url)
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
