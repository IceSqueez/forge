#![allow(clippy::expect_used, clippy::unwrap_used)]

use loom_storage::UserGlobalsRepo;
use loom_storage_sqlite::{SqliteUserGlobalsRepo, apply_migrations};
use loom_types::Variant;

async fn setup() -> SqliteUserGlobalsRepo {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:")
        .await
        .expect("in-memory pool");
    apply_migrations(&pool).await.expect("migrations apply");
    SqliteUserGlobalsRepo::new(pool)
}

#[tokio::test]
async fn set_then_get_roundtrips_value() {
    let repo = setup().await;
    repo.set("broadcaster1", "user1", "score", Variant::Int(42))
        .await
        .expect("set");
    let got = repo
        .get("broadcaster1", "user1", "score")
        .await
        .expect("get");
    assert_eq!(got, Some(Variant::Int(42)));
}

#[tokio::test]
async fn get_missing_key_returns_none() {
    let repo = setup().await;
    let got = repo
        .get("broadcaster1", "user1", "nonexistent")
        .await
        .expect("get");
    assert!(got.is_none());
}

#[tokio::test]
async fn list_for_user_returns_only_that_users_entries() {
    let repo = setup().await;
    repo.set("bc1", "userA", "x", Variant::Int(1))
        .await
        .expect("set A/x");
    repo.set("bc1", "userA", "y", Variant::Bool(true))
        .await
        .expect("set A/y");
    repo.set("bc1", "userB", "x", Variant::Int(99))
        .await
        .expect("set B/x");

    let entries = repo.list_for_user("bc1", "userA").await.expect("list");
    assert_eq!(entries.len(), 2);
    assert!(entries.iter().any(|e| e.name == "x"));
    assert!(entries.iter().any(|e| e.name == "y"));
    assert!(entries.iter().all(|e| e.user_id == "userA"));
}

#[tokio::test]
async fn list_for_broadcaster_returns_all_users() {
    let repo = setup().await;
    repo.set("bc1", "userA", "pts", Variant::Int(10))
        .await
        .expect("set A");
    repo.set("bc1", "userB", "pts", Variant::Int(20))
        .await
        .expect("set B");
    repo.set("bc2", "userC", "pts", Variant::Int(30))
        .await
        .expect("set C other broadcaster");

    let entries = repo
        .list_for_broadcaster("bc1")
        .await
        .expect("list broadcaster");
    assert_eq!(entries.len(), 2);
    assert!(entries.iter().all(|e| e.broadcaster_id == "bc1"));
}

#[tokio::test]
async fn delete_returns_true_and_entry_is_gone() {
    let repo = setup().await;
    repo.set("bc1", "u1", "temp", Variant::String("val".into()))
        .await
        .expect("set");
    let deleted = repo.delete("bc1", "u1", "temp").await.expect("delete");
    assert!(deleted);
    let entries = repo.list_for_user("bc1", "u1").await.expect("list");
    assert!(entries.is_empty());
}

#[tokio::test]
async fn delete_missing_returns_false() {
    let repo = setup().await;
    let deleted = repo
        .delete("bc1", "u1", "ghost")
        .await
        .expect("delete missing");
    assert!(!deleted);
}

#[tokio::test]
async fn set_overwrites_existing_value() {
    let repo = setup().await;
    repo.set("bc1", "u1", "k", Variant::Int(1))
        .await
        .expect("set 1");
    repo.set("bc1", "u1", "k", Variant::Int(2))
        .await
        .expect("set 2");
    let got = repo.get("bc1", "u1", "k").await.expect("get");
    assert_eq!(got, Some(Variant::Int(2)));
}
