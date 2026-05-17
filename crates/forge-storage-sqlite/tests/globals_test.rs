#![allow(clippy::expect_used, clippy::unwrap_used)]

use forge_storage::GlobalsRepo;
use forge_storage_sqlite::{SqliteGlobalsRepo, apply_migrations};
use forge_types::Variant;

async fn setup() -> SqliteGlobalsRepo {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:")
        .await
        .expect("in-memory pool");
    apply_migrations(&pool).await.expect("migrations apply");
    SqliteGlobalsRepo::new(pool)
}

#[tokio::test]
async fn set_then_get_roundtrips_value() {
    let repo = setup().await;
    repo.set("counter", Variant::Int(7), true)
        .await
        .expect("set");
    let got = repo.get("counter").await.expect("get");
    assert_eq!(got, Some(Variant::Int(7)));
}

#[tokio::test]
async fn get_missing_key_returns_none() {
    let repo = setup().await;
    let got = repo.get("nonexistent").await.expect("get");
    assert!(got.is_none());
}

#[tokio::test]
async fn set_increments_writes_on_update() {
    let repo = setup().await;
    repo.set("x", Variant::Bool(false), false)
        .await
        .expect("set 1");
    repo.set("x", Variant::Bool(true), false)
        .await
        .expect("set 2");

    let entries = repo.list().await.expect("list");
    let entry = entries.iter().find(|e| e.name == "x").expect("entry");
    assert_eq!(entry.writes, 2, "writes must be 2 after two sets");
}

#[tokio::test]
async fn get_increments_reads() {
    let repo = setup().await;
    repo.set("y", Variant::String("hello".into()), true)
        .await
        .expect("set");
    repo.get("y").await.expect("get 1");
    repo.get("y").await.expect("get 2");

    let entries = repo.list().await.expect("list");
    let entry = entries.iter().find(|e| e.name == "y").expect("entry");
    assert_eq!(entry.reads, 2, "reads must be 2 after two gets");
}

#[tokio::test]
async fn delete_existing_key_returns_true() {
    let repo = setup().await;
    repo.set("to_delete", Variant::Int(1), true)
        .await
        .expect("set");
    let deleted = repo.delete("to_delete").await.expect("delete");
    assert!(deleted);
    assert!(repo.get("to_delete").await.expect("get").is_none());
}

#[tokio::test]
async fn delete_missing_key_returns_false() {
    let repo = setup().await;
    let deleted = repo.delete("ghost").await.expect("delete missing");
    assert!(!deleted);
}

#[tokio::test]
async fn storage_bytes_grows_after_insert() {
    let repo = setup().await;
    let before = repo.storage_bytes().await.expect("bytes before");
    repo.set("big_key", Variant::String("some value".into()), true)
        .await
        .expect("set");
    let after = repo.storage_bytes().await.expect("bytes after");
    assert!(after > before, "storage_bytes must grow after insert");
}

#[tokio::test]
async fn last_save_at_none_when_no_persisted_globals() {
    let repo = setup().await;
    repo.set("mem", Variant::Int(0), false).await.expect("set");
    let ts = repo.last_save_at().await.expect("last_save_at");
    assert!(
        ts.is_none(),
        "no persisted globals means last_save_at is None"
    );
}

#[tokio::test]
async fn last_save_at_some_when_persisted_global_exists() {
    let repo = setup().await;
    repo.set("persisted_key", Variant::Int(42), true)
        .await
        .expect("set");
    let ts = repo.last_save_at().await.expect("last_save_at");
    assert!(ts.is_some(), "persisted global must produce a timestamp");
}

#[tokio::test]
async fn list_returns_all_stored_globals() {
    let repo = setup().await;
    repo.set("a", Variant::Int(1), true).await.expect("set a");
    repo.set("b", Variant::Bool(false), false)
        .await
        .expect("set b");

    let entries = repo.list().await.expect("list");
    assert_eq!(entries.len(), 2);
    assert!(entries.iter().any(|e| e.name == "a"));
    assert!(entries.iter().any(|e| e.name == "b"));
}

#[tokio::test]
async fn set_overwrites_existing_value() {
    let repo = setup().await;
    repo.set("key", Variant::Int(1), true).await.expect("set 1");
    repo.set("key", Variant::Int(2), true).await.expect("set 2");
    let got = repo.get("key").await.expect("get");
    assert_eq!(got, Some(Variant::Int(2)));
}
