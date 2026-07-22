#![allow(clippy::expect_used, clippy::unwrap_used)]

use forge_storage::{GlobalsRepo, StorageError};
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
async fn archive_transitions_live_to_archived_and_is_false_otherwise() {
    let repo = setup().await;

    assert!(
        !repo.archive("ghost").await.expect("archive missing"),
        "archiving a name that never existed must return false"
    );

    repo.set("counter", Variant::Int(1), true)
        .await
        .expect("set");

    assert!(
        repo.archive("counter").await.expect("archive live"),
        "archiving a live global must return true"
    );
    assert!(
        !repo.archive("counter").await.expect("archive again"),
        "archiving an already-archived global must return false"
    );
}

#[tokio::test]
async fn restore_transitions_archived_to_live_and_is_false_otherwise() {
    let repo = setup().await;

    assert!(
        !repo.restore("ghost").await.expect("restore missing"),
        "restoring a name that never existed must return false"
    );

    repo.set("counter", Variant::Int(1), true)
        .await
        .expect("set");
    assert!(
        !repo.restore("counter").await.expect("restore live"),
        "restoring a never-archived global must return false"
    );

    repo.archive("counter").await.expect("archive");
    assert!(
        repo.restore("counter").await.expect("restore archived"),
        "restoring an archived global must return true"
    );
    assert!(
        !repo.restore("counter").await.expect("restore again"),
        "restoring an already-live global must return false"
    );
}

#[tokio::test]
async fn archive_hides_global_across_read_surface_and_lists_it_in_archived() {
    let repo = setup().await;
    repo.set("secret", Variant::Int(42), true)
        .await
        .expect("set");
    repo.archive("secret").await.expect("archive");

    assert_eq!(repo.get("secret").await.expect("get"), None, "get() hidden");
    assert_eq!(
        repo.persisted("secret").await.expect("persisted"),
        None,
        "persisted() hidden"
    );
    assert!(
        repo.list()
            .await
            .expect("list")
            .iter()
            .all(|e| e.name != "secret"),
        "list() must exclude the archived global"
    );

    let archived = repo.list_archived().await.expect("list_archived");
    let entry = archived
        .iter()
        .find(|e| e.name == "secret")
        .expect("archived global must appear in list_archived");
    assert_eq!(entry.value, Variant::Int(42), "value survives archiving");
}

#[tokio::test]
async fn restore_returns_archived_global_to_visibility() {
    let repo = setup().await;
    repo.set("item", Variant::String("v".into()), false)
        .await
        .expect("set");
    repo.archive("item").await.expect("archive");
    repo.restore("item").await.expect("restore");

    assert_eq!(
        repo.get("item").await.expect("get"),
        Some(Variant::String("v".into())),
        "restored global is readable again"
    );
    assert!(
        repo.list()
            .await
            .expect("list")
            .iter()
            .any(|e| e.name == "item"),
        "restored global reappears in list()"
    );
    assert!(
        repo.list_archived()
            .await
            .expect("list_archived")
            .iter()
            .all(|e| e.name != "item"),
        "restored global must leave list_archived"
    );
}

#[tokio::test]
async fn set_on_archived_global_resurrects_it_with_new_value() {
    let repo = setup().await;
    repo.set("dup", Variant::Int(1), true).await.expect("set 1");
    repo.archive("dup").await.expect("archive");

    repo.set("dup", Variant::Int(99), true)
        .await
        .expect("set on archived name");

    assert_eq!(
        repo.get("dup").await.expect("get"),
        Some(Variant::Int(99)),
        "set() on an archived name resurrects it with the new value"
    );
    assert!(
        repo.list()
            .await
            .expect("list")
            .iter()
            .any(|e| e.name == "dup"),
        "resurrected global reappears in list()"
    );
    assert!(
        repo.list_archived()
            .await
            .expect("list_archived")
            .iter()
            .all(|e| e.name != "dup"),
        "resurrected global must leave list_archived"
    );
}

#[tokio::test]
async fn incr_on_archived_numeric_global_reports_not_found() {
    let repo = setup().await;
    repo.set("n", Variant::Int(10), false).await.expect("set");
    repo.archive("n").await.expect("archive");

    let err = repo.incr("n", 5).await.expect_err("must fail");
    assert!(
        matches!(err, StorageError::NotFound { .. }),
        "incr on archived numeric global must be NotFound, got {err:?}"
    );
}
