#![allow(clippy::expect_used)]

use forge_storage::{DataProvider, GlobalsRepo};
use forge_storage_sqlite::SqliteBackend;
use forge_types::Variant;

const TEST_KEY: [u8; 32] = [0xab; 32];

#[tokio::test]
async fn open_succeeds_with_in_memory_db() {
    SqliteBackend::open_with_key("sqlite::memory:", TEST_KEY)
        .await
        .expect("SqliteBackend::open_with_key must succeed on sqlite::memory:");
}

#[tokio::test]
async fn schema_version_returns_3_after_all_migrations() {
    let backend = SqliteBackend::open_with_key("sqlite::memory:", TEST_KEY)
        .await
        .expect("open");

    let version = backend.schema_version().await.expect("schema_version");
    assert_eq!(version, 3, "expected 3 applied migrations");
}

#[tokio::test]
async fn dataprovider_coercion_compiles_and_delegates() {
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
