#![allow(clippy::expect_used, clippy::unwrap_used)]

use forge_storage::ActionRecord;
use forge_storage::ActionRepo;
use forge_storage_sqlite::{SqliteActionRepo, apply_migrations};
use forge_types::ActionId;

async fn setup() -> SqliteActionRepo {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:")
        .await
        .expect("in-memory pool");
    apply_migrations(&pool).await.expect("migrations apply");
    SqliteActionRepo::new(pool)
}

fn make_record(name: &str) -> ActionRecord {
    ActionRecord {
        id: ActionId::new(),
        name: name.to_owned(),
        config_json: r#"{"sub_actions":[]}"#.to_owned(),
        created_at: time::OffsetDateTime::now_utc(),
        last_modified: time::OffsetDateTime::now_utc(),
    }
}

#[tokio::test]
async fn upsert_then_get_roundtrips_record() {
    let repo = setup().await;
    let record = make_record("greet");
    let id = record.id;
    repo.upsert(record).await.expect("upsert");
    let got = repo.get(id).await.expect("get");
    assert!(got.is_some());
    let got = got.unwrap();
    assert_eq!(got.id, id);
    assert_eq!(got.name, "greet");
    assert_eq!(got.config_json, r#"{"sub_actions":[]}"#);
}

#[tokio::test]
async fn get_missing_id_returns_none() {
    let repo = setup().await;
    let got = repo.get(ActionId::new()).await.expect("get");
    assert!(got.is_none());
}

#[tokio::test]
async fn get_by_name_finds_existing_action() {
    let repo = setup().await;
    let record = make_record("farewell");
    repo.upsert(record).await.expect("upsert");
    let got = repo.get_by_name("farewell").await.expect("get_by_name");
    assert!(got.is_some());
    assert_eq!(got.unwrap().name, "farewell");
}

#[tokio::test]
async fn get_by_name_missing_returns_none() {
    let repo = setup().await;
    let got = repo.get_by_name("ghost").await.expect("get_by_name");
    assert!(got.is_none());
}

#[tokio::test]
async fn upsert_updates_existing_config_json() {
    let repo = setup().await;
    let mut record = make_record("updatable");
    let id = record.id;
    repo.upsert(record.clone()).await.expect("upsert 1");

    record.config_json = r#"{"sub_actions":["a"]}"#.to_owned();
    repo.upsert(record).await.expect("upsert 2");

    let got = repo.get(id).await.expect("get").unwrap();
    assert_eq!(got.config_json, r#"{"sub_actions":["a"]}"#);
}

#[tokio::test]
async fn delete_existing_returns_true_and_removes() {
    let repo = setup().await;
    let record = make_record("to_delete");
    let id = record.id;
    repo.upsert(record).await.expect("upsert");
    let deleted = repo.delete(id).await.expect("delete");
    assert!(deleted);
    assert!(repo.get(id).await.expect("get").is_none());
}

#[tokio::test]
async fn delete_missing_returns_false() {
    let repo = setup().await;
    let deleted = repo.delete(ActionId::new()).await.expect("delete missing");
    assert!(!deleted);
}

#[tokio::test]
async fn list_returns_all_actions() {
    let repo = setup().await;
    repo.upsert(make_record("alpha")).await.expect("upsert a");
    repo.upsert(make_record("beta")).await.expect("upsert b");
    let records = repo.list().await.expect("list");
    assert_eq!(records.len(), 2);
    assert!(records.iter().any(|r| r.name == "alpha"));
    assert!(records.iter().any(|r| r.name == "beta"));
}
