#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::Arc;

use forge_storage::{GlobalsExport, GlobalsRepo, StorageError};
use forge_storage_sqlite::{SqliteBackend, SqliteGlobalsRepo, apply_migrations};
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

const TEST_KEY: [u8; 32] = [0xab; 32];

async fn setup_file_backed() -> (tempfile::TempDir, Arc<SqliteBackend>) {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let path = dir.path().join("test.sqlite");
    let url = format!("sqlite://{}", path.display());
    let backend = SqliteBackend::open_with_key(&url, TEST_KEY)
        .await
        .expect("open file-backed db");
    (dir, Arc::new(backend))
}

#[tokio::test]
async fn incr_on_int_global_returns_updated_value() {
    let repo = setup().await;
    repo.set("n", Variant::Int(10), false).await.expect("set");
    let result = repo.incr("n", 5).await.expect("incr");
    assert_eq!(result, Variant::Int(15));
}

#[tokio::test]
async fn incr_with_negative_amount_decrements() {
    let repo = setup().await;
    repo.set("n", Variant::Int(10), false).await.expect("set");
    let result = repo.incr("n", -3).await.expect("incr");
    assert_eq!(result, Variant::Int(7));
}

#[tokio::test]
async fn incr_updates_writes_counter() {
    let repo = setup().await;
    repo.set("n", Variant::Int(0), false).await.expect("set");
    repo.incr("n", 1).await.expect("incr 1");
    repo.incr("n", 1).await.expect("incr 2");
    let entries = repo.list().await.expect("list");
    let entry = entries.iter().find(|e| e.name == "n").expect("entry");
    assert_eq!(entry.writes, 3, "set + 2 incr = 3 writes");
}

#[tokio::test]
async fn incr_on_missing_key_returns_not_found() {
    let repo = setup().await;
    let err = repo.incr("ghost", 1).await.expect_err("must fail");
    assert!(
        matches!(err, StorageError::NotFound { .. }),
        "expected NotFound, got {err:?}"
    );
}

#[tokio::test]
async fn incr_on_string_global_returns_type_mismatch() {
    let repo = setup().await;
    repo.set("s", Variant::String("hello".into()), false)
        .await
        .expect("set");
    let err = repo.incr("s", 1).await.expect_err("must fail");
    assert!(
        matches!(err, StorageError::TypeMismatch { .. }),
        "expected TypeMismatch, got {err:?}"
    );
}

#[tokio::test]
async fn concurrent_reads_counter_exact() {
    let (_dir, backend) = setup_file_backed().await;
    backend.set("x", Variant::Int(0), false).await.expect("set");

    let mut handles = Vec::with_capacity(100);
    for _ in 0..100 {
        let b = Arc::clone(&backend);
        handles.push(tokio::spawn(async move { b.get("x").await.expect("get") }));
    }
    for h in handles {
        h.await.expect("task panicked");
    }

    let entries = backend.list().await.expect("list");
    let entry = entries.iter().find(|e| e.name == "x").expect("entry");
    assert_eq!(
        entry.reads, 100,
        "100 concurrent get() calls must yield reads == 100"
    );
    assert_eq!(
        entry.writes, 1,
        "only the initial set contributes to writes"
    );
}

#[tokio::test]
async fn concurrent_incr_no_lost_updates() {
    let (_dir, backend) = setup_file_backed().await;
    backend
        .set("counter", Variant::Int(0), false)
        .await
        .expect("set");

    let mut handles = Vec::with_capacity(50);
    for _ in 0..50 {
        let b = Arc::clone(&backend);
        handles.push(tokio::spawn(async move {
            b.incr("counter", 1).await.expect("incr")
        }));
    }
    for h in handles {
        h.await.expect("task panicked");
    }

    let got = backend.get("counter").await.expect("get");
    assert_eq!(
        got,
        Some(Variant::Int(50)),
        "50 concurrent incr(1) must sum to exactly 50"
    );

    let entries = backend.list().await.expect("list");
    let entry = entries.iter().find(|e| e.name == "counter").expect("entry");
    assert_eq!(entry.reads, 1, "one get at end increments reads to 1");
    assert_eq!(entry.writes, 51, "one set + 50 incr = 51 writes");
}

#[tokio::test]
async fn export_all_empty_db_returns_empty_vec() {
    let repo = setup().await;
    let result = repo.export_all().await.expect("export_all");
    assert!(result.is_empty());
}

#[tokio::test]
async fn export_all_returns_sorted_by_name_with_correct_fields() {
    let repo = setup().await;
    repo.set("gamma", Variant::Int(3), false)
        .await
        .expect("set gamma");
    repo.set("alpha", Variant::String("hello".into()), true)
        .await
        .expect("set alpha");
    repo.set("beta", Variant::Bool(true), true)
        .await
        .expect("set beta");

    let exports = repo.export_all().await.expect("export_all");
    assert_eq!(exports.len(), 3);

    assert_eq!(exports[0].name, "alpha");
    assert_eq!(exports[0].value, Variant::String("hello".into()));
    assert!(exports[0].persisted);
    assert_eq!(exports[0].writes, 1);

    assert_eq!(exports[1].name, "beta");
    assert_eq!(exports[1].value, Variant::Bool(true));
    assert!(exports[1].persisted);

    assert_eq!(exports[2].name, "gamma");
    assert_eq!(exports[2].value, Variant::Int(3));
    assert!(!exports[2].persisted);
}

#[tokio::test]
async fn export_all_preserves_reads_writes_counters() {
    let repo = setup().await;
    repo.set("tracked", Variant::Int(0), true)
        .await
        .expect("set");
    repo.set("tracked", Variant::Int(1), true)
        .await
        .expect("set again");
    repo.get("tracked").await.expect("get 1");
    repo.get("tracked").await.expect("get 2");
    repo.get("tracked").await.expect("get 3");

    let exports = repo.export_all().await.expect("export_all");
    let entry = exports.iter().find(|e| e.name == "tracked").expect("entry");
    assert_eq!(entry.writes, 2, "two set calls");
    assert_eq!(entry.reads, 3, "three get calls");
}

#[tokio::test]
async fn export_all_does_not_increment_reads() {
    let repo = setup().await;
    repo.set("noread", Variant::Int(7), true)
        .await
        .expect("set");

    repo.export_all().await.expect("export 1");
    repo.export_all().await.expect("export 2");
    repo.export_all().await.expect("export 3");

    repo.get("noread").await.expect("get");

    let entries = repo.list().await.expect("list");
    let entry = entries.iter().find(|e| e.name == "noread").expect("entry");
    assert_eq!(entry.reads, 1, "only the single get call incremented reads");
}

#[tokio::test]
async fn export_all_round_trips_via_json_envelope() {
    let repo = setup().await;
    repo.set("counter", Variant::Int(7), true)
        .await
        .expect("set int");
    repo.set("flag", Variant::Bool(false), true)
        .await
        .expect("set bool");
    repo.set("label", Variant::String("forge".into()), false)
        .await
        .expect("set string");
    repo.set("ratio", Variant::Float(1.5), false)
        .await
        .expect("set float");

    let globals = repo.export_all().await.expect("export_all");
    assert_eq!(globals.len(), 4);

    let envelope = GlobalsExport::new(globals);
    let json = serde_json::to_string(&envelope).expect("serialize");

    assert!(
        json.contains("\"format_version\":1"),
        "format_version must be 1"
    );
    assert!(json.contains("\"type\":\"int\""), "int variant present");
    assert!(json.contains("\"type\":\"bool\""), "bool variant present");
    assert!(
        json.contains("\"type\":\"string\""),
        "string variant present"
    );
    assert!(json.contains("\"type\":\"float\""), "float variant present");

    let decoded: GlobalsExport = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(decoded.format_version, 1);
    assert_eq!(decoded.globals.len(), 4);

    let counter = decoded
        .globals
        .iter()
        .find(|g| g.name == "counter")
        .expect("counter");
    assert_eq!(counter.value, Variant::Int(7));
    assert!(counter.persisted);
    assert_eq!(counter.writes, 1);
}

#[tokio::test]
async fn set_stores_the_configured_persisted_flag() {
    let repo = setup().await;
    for (name, flag) in [("kept", true), ("ephemeral", false)] {
        repo.set(name, Variant::Int(1), flag).await.expect("set");
        assert_eq!(
            repo.persisted(name).await.expect("persisted"),
            Some(flag),
            "persisted({name}) must report the flag passed to set()"
        );
        let entries = repo.list().await.expect("list");
        let entry = entries.iter().find(|e| e.name == name).expect("entry");
        assert_eq!(entry.persisted, flag, "list() must agree for {name}");
    }
}

#[tokio::test]
async fn set_on_existing_key_updates_the_persisted_flag_both_directions() {
    let repo = setup().await;
    for (name, first, second) in [("promote", false, true), ("demote", true, false)] {
        repo.set(name, Variant::Int(0), first).await.expect("set 1");
        repo.set(name, Variant::Int(1), second)
            .await
            .expect("set 2");
        assert_eq!(
            repo.persisted(name).await.expect("persisted"),
            Some(second),
            "overwrite of {name} must move persisted {first} -> {second}"
        );
    }
}

#[tokio::test]
async fn persisted_returns_none_for_absent_key() {
    let repo = setup().await;
    assert_eq!(repo.persisted("ghost").await.expect("persisted"), None);
}

#[tokio::test]
async fn persisted_query_does_not_count_as_a_read() {
    let repo = setup().await;
    repo.set("k", Variant::Int(0), true).await.expect("set");
    repo.persisted("k").await.expect("persisted 1");
    repo.persisted("k").await.expect("persisted 2");
    repo.persisted("k").await.expect("persisted 3");
    repo.get("k").await.expect("get");

    let entries = repo.list().await.expect("list");
    let entry = entries.iter().find(|e| e.name == "k").expect("entry");
    assert_eq!(
        entry.reads, 1,
        "persisted() must not increment reads; only get() does"
    );
}

#[tokio::test]
async fn backend_persisted_delegates_to_globals_repo() {
    let backend = SqliteBackend::open_with_key(":memory:", [0xab; 32])
        .await
        .expect("open backend");
    backend
        .set("live", Variant::Int(1), true)
        .await
        .expect("set");

    assert_eq!(
        backend.persisted("live").await.expect("persisted"),
        Some(true)
    );
    assert_eq!(backend.persisted("nope").await.expect("persisted"), None);
}

#[tokio::test]
async fn backend_export_all_delegates_correctly() {
    let backend = SqliteBackend::open_with_key(":memory:", [0xab; 32])
        .await
        .expect("open backend");

    backend
        .set("x", Variant::Int(100), true)
        .await
        .expect("set x");
    backend
        .set("y", Variant::Bool(true), false)
        .await
        .expect("set y");

    let exports = backend.export_all().await.expect("export_all via backend");
    assert_eq!(exports.len(), 2);

    let x = exports.iter().find(|e| e.name == "x").expect("x");
    assert_eq!(x.value, Variant::Int(100));
    assert!(x.persisted);

    let y = exports.iter().find(|e| e.name == "y").expect("y");
    assert_eq!(y.value, Variant::Bool(true));
    assert!(!y.persisted);
}
