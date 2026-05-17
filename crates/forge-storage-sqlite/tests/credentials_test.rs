#![allow(clippy::expect_used, clippy::unwrap_used)]

use forge_storage::{CredentialId, CredentialsRepo};
use forge_storage_sqlite::{SqliteCredentialsRepo, apply_migrations};

const TEST_KEY: [u8; 32] = [
    0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10,
    0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f, 0x20,
];

const WRONG_KEY: [u8; 32] = [
    0xde, 0xad, 0xbe, 0xef, 0xde, 0xad, 0xbe, 0xef, 0xde, 0xad, 0xbe, 0xef, 0xde, 0xad, 0xbe, 0xef,
    0xde, 0xad, 0xbe, 0xef, 0xde, 0xad, 0xbe, 0xef, 0xde, 0xad, 0xbe, 0xef, 0xde, 0xad, 0xbe, 0xef,
];

async fn setup() -> SqliteCredentialsRepo {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:")
        .await
        .expect("in-memory pool");
    apply_migrations(&pool).await.expect("migrations apply");
    SqliteCredentialsRepo::new_with_key(pool, TEST_KEY)
}

#[tokio::test]
async fn store_then_load_decrypts_to_original() {
    let repo = setup().await;
    let id = CredentialId::new("twitch:bot");
    repo.store(&id, "secret_token").await.expect("store");
    let loaded = repo.load(&id).await.expect("load");
    assert_eq!(loaded, Some("secret_token".to_string()));
}

#[tokio::test]
async fn load_missing_returns_none() {
    let repo = setup().await;
    let id = CredentialId::new("nonexistent");
    let loaded = repo.load(&id).await.expect("load");
    assert!(loaded.is_none());
}

#[tokio::test]
async fn wrong_key_fails_decryption() {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:")
        .await
        .expect("in-memory pool");
    apply_migrations(&pool).await.expect("migrations apply");

    let writer = SqliteCredentialsRepo::new_with_key(pool.clone(), TEST_KEY);
    let id = CredentialId::new("twitch:bot");
    writer.store(&id, "secret_token").await.expect("store");

    let reader = SqliteCredentialsRepo::new_with_key(pool, WRONG_KEY);
    let result = reader.load(&id).await;
    assert!(
        result.is_err(),
        "expected decryption failure with wrong key"
    );
}

#[tokio::test]
async fn list_ids_returns_stored_ids_not_values() {
    let repo = setup().await;

    repo.store(&CredentialId::new("twitch:broadcaster"), "token_a")
        .await
        .expect("store a");
    repo.store(&CredentialId::new("youtube:main"), "token_b")
        .await
        .expect("store b");

    let ids = repo.list_ids().await.expect("list_ids");
    assert_eq!(ids.len(), 2);

    let id_strs: Vec<&str> = ids.iter().map(|id| id.as_str()).collect();
    assert!(id_strs.contains(&"twitch:broadcaster"));
    assert!(id_strs.contains(&"youtube:main"));
}

#[tokio::test]
async fn delete_existing_returns_true_and_clears_entry() {
    let repo = setup().await;
    let id = CredentialId::new("twitch:bot");
    repo.store(&id, "token").await.expect("store");

    let removed = repo.delete(&id).await.expect("delete");
    assert!(removed);

    let loaded = repo.load(&id).await.expect("load after delete");
    assert!(loaded.is_none());
}

#[tokio::test]
async fn delete_missing_returns_false() {
    let repo = setup().await;
    let id = CredentialId::new("nonexistent");
    let removed = repo.delete(&id).await.expect("delete missing");
    assert!(!removed);
}

#[tokio::test]
async fn mark_refreshed_updates_last_refresh_timestamp() {
    let repo = setup().await;
    let id = CredentialId::new("twitch:broadcaster");
    repo.store(&id, "token").await.expect("store");

    let before = repo.last_refresh(&id).await.expect("last_refresh before");
    assert!(before.is_some());

    tokio::time::sleep(tokio::time::Duration::from_millis(2)).await;

    repo.mark_refreshed(&id).await.expect("mark_refreshed");

    let after = repo.last_refresh(&id).await.expect("last_refresh after");
    assert!(after.is_some());
    assert!(
        after.unwrap() >= before.unwrap(),
        "mark_refreshed must not go backward"
    );
}

#[tokio::test]
async fn last_refresh_missing_returns_none() {
    let repo = setup().await;
    let id = CredentialId::new("nonexistent");
    let ts = repo.last_refresh(&id).await.expect("last_refresh");
    assert!(ts.is_none());
}

#[tokio::test]
async fn store_overwrites_existing_credential() {
    let repo = setup().await;
    let id = CredentialId::new("twitch:bot");
    repo.store(&id, "old_token").await.expect("store old");
    repo.store(&id, "new_token").await.expect("store new");

    let loaded = repo.load(&id).await.expect("load");
    assert_eq!(loaded, Some("new_token".to_string()));
}
