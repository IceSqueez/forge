#![allow(clippy::expect_used, clippy::unwrap_used)]

use loom_storage::{ScriptRecord, ScriptRepo};
use loom_storage_sqlite::{SqliteScriptRepo, apply_migrations};
use loom_types::ScriptId;

async fn setup() -> SqliteScriptRepo {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:")
        .await
        .expect("in-memory pool");
    apply_migrations(&pool).await.expect("migrations apply");
    SqliteScriptRepo::new(pool)
}

fn make_record(name: &str, enabled: bool) -> ScriptRecord {
    ScriptRecord {
        id: ScriptId::new(),
        name: name.to_owned(),
        source_code: r#"print("hello");"#.to_owned(),
        description: None,
        enabled,
        created_at: time::OffsetDateTime::now_utc(),
        last_modified: time::OffsetDateTime::now_utc(),
    }
}

#[tokio::test]
async fn upsert_get_roundtrip_with_description() {
    let repo = setup().await;
    let mut record = make_record("describe_me", true);
    record.description = Some("Greets chat on follow.".to_owned());
    let id = record.id;

    repo.upsert(record).await.expect("upsert");
    let got = repo.get(id).await.expect("get").unwrap();

    assert_eq!(got.id, id);
    assert_eq!(got.name, "describe_me");
    assert_eq!(got.source_code, r#"print("hello");"#);
    assert_eq!(got.description.as_deref(), Some("Greets chat on follow."));
    assert!(got.enabled);
}

#[tokio::test]
async fn upsert_get_roundtrip_without_description() {
    let repo = setup().await;
    let record = make_record("no_desc", false);
    let id = record.id;

    repo.upsert(record).await.expect("upsert");
    let got = repo.get(id).await.expect("get").unwrap();

    assert_eq!(got.id, id);
    assert_eq!(got.description, None);
    assert!(!got.enabled);
}

#[tokio::test]
async fn get_missing_id_returns_none() {
    let repo = setup().await;
    let got = repo.get(ScriptId::new()).await.expect("get");
    assert!(got.is_none());
}

#[tokio::test]
async fn get_by_name_finds_existing_script() {
    let repo = setup().await;
    repo.upsert(make_record("known_name", true))
        .await
        .expect("upsert");
    let got = repo
        .get_by_name("known_name")
        .await
        .expect("get_by_name")
        .unwrap();
    assert_eq!(got.name, "known_name");
}

#[tokio::test]
async fn get_by_name_missing_returns_none() {
    let repo = setup().await;
    let got = repo.get_by_name("ghost").await.expect("get_by_name");
    assert!(got.is_none());
}

#[tokio::test]
async fn upsert_updates_existing_source_code() {
    let repo = setup().await;
    let mut record = make_record("mutable", true);
    let id = record.id;
    repo.upsert(record.clone()).await.expect("upsert 1");

    record.source_code = r#"let x = 42;"#.to_owned();
    repo.upsert(record).await.expect("upsert 2");

    let got = repo.get(id).await.expect("get").unwrap();
    assert_eq!(got.source_code, r#"let x = 42;"#);
}

#[tokio::test]
async fn list_enabled_filters_correctly() {
    let repo = setup().await;
    repo.upsert(make_record("active_one", true))
        .await
        .expect("upsert active");
    repo.upsert(make_record("active_two", true))
        .await
        .expect("upsert active 2");
    repo.upsert(make_record("inactive_one", false))
        .await
        .expect("upsert inactive");

    let enabled = repo.list_enabled().await.expect("list_enabled");
    assert_eq!(enabled.len(), 2);
    assert!(enabled.iter().all(|r| r.enabled));
    assert!(enabled.iter().any(|r| r.name == "active_one"));
    assert!(enabled.iter().any(|r| r.name == "active_two"));
}

#[tokio::test]
async fn delete_existing_returns_true_and_removes() {
    let repo = setup().await;
    let record = make_record("to_delete", true);
    let id = record.id;
    repo.upsert(record).await.expect("upsert");

    let deleted = repo.delete(id).await.expect("delete");
    assert!(deleted);

    let all = repo.list().await.expect("list");
    assert!(all.is_empty());
}

#[tokio::test]
async fn delete_missing_returns_false() {
    let repo = setup().await;
    let deleted = repo.delete(ScriptId::new()).await.expect("delete missing");
    assert!(!deleted);
}
