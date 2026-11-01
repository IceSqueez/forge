#![expect(clippy::expect_used, reason = "test fixtures")]
use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use forge_storage::{CredentialId, CredentialsRepo, DataProvider, StorageError};
use forge_storage_sqlite::{SqliteBackend, SqliteCredentialsRepo};
use sqlx::SqlitePool;

mod common;

const TEST_KEY: [u8; 32] = [
    0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10,
    0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f, 0x20,
];

const WRONG_KEY: [u8; 32] = [
    0xde, 0xad, 0xbe, 0xef, 0xde, 0xad, 0xbe, 0xef, 0xde, 0xad, 0xbe, 0xef, 0xde, 0xad, 0xbe, 0xef,
    0xde, 0xad, 0xbe, 0xef, 0xde, 0xad, 0xbe, 0xef, 0xde, 0xad, 0xbe, 0xef, 0xde, 0xad, 0xbe, 0xef,
];

async fn setup() -> common::Sandboxed<SqliteCredentialsRepo> {
    let pools = common::migrated_pools().await;
    pools.map(|pools| SqliteCredentialsRepo::new_with_key(pools, TEST_KEY))
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
async fn wrong_key_returns_decryption_error() {
    let pools = common::migrated_pools().await;

    let writer = SqliteCredentialsRepo::new_with_key(pools.clone(), TEST_KEY);
    let id = CredentialId::new("twitch:bot");
    writer.store(&id, "secret_token").await.expect("store");

    let reader = SqliteCredentialsRepo::new_with_key(pools.clone(), WRONG_KEY);
    let result = reader.load(&id).await;
    assert!(
        matches!(result, Err(StorageError::Decryption)),
        "expected StorageError::Decryption, got {result:?}"
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
async fn mark_refreshed_on_a_missing_credential_returns_not_found() {
    let repo = setup().await;

    let err = repo
        .mark_refreshed(&CredentialId::new("never:stored"))
        .await
        .expect_err("a missing credential must not be silently ignored");

    assert!(
        matches!(&err, StorageError::NotFound { key } if key == "never:stored"),
        "got {err:?}"
    );
}

#[tokio::test]
async fn mark_refreshed_updates_last_refresh_timestamp() {
    let pools = common::migrated_pools().await;
    let repo = SqliteCredentialsRepo::new_with_key(pools.clone(), TEST_KEY);
    let id = CredentialId::new("twitch:broadcaster");
    repo.store(&id, "token").await.expect("store");
    sqlx::query("UPDATE credentials SET last_refresh = 1 WHERE id = ?")
        .bind(id.as_str())
        .execute(pools.writer())
        .await
        .expect("backdate the stamp");
    let before = repo.last_refresh(&id).await.expect("last_refresh before");

    repo.mark_refreshed(&id).await.expect("mark_refreshed");

    let after = repo.last_refresh(&id).await.expect("last_refresh after");
    assert!(after > before, "{before:?} -> {after:?}");
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

const LEGACY_NONCE: [u8; 12] = [0x07; 12];
const BROADCASTER: &str = "twitch:broadcaster";
const BOT: &str = "twitch:bot";

fn legacy_sealed(key: &[u8; 32], plaintext: &str) -> Vec<u8> {
    Aes256Gcm::new_from_slice(key)
        .expect("a 32-byte key")
        .encrypt(
            &Nonce::try_from(LEGACY_NONCE.as_slice()).expect("a 12-byte nonce"),
            plaintext.as_bytes(),
        )
        .expect("the legacy sealing")
}

async fn seed_legacy(pool: &SqlitePool, id: &str, key: &[u8; 32], plaintext: &str) {
    sqlx::query("INSERT INTO credentials (id, encrypted, nonce, last_refresh) VALUES (?, ?, ?, 0)")
        .bind(id)
        .bind(legacy_sealed(key, plaintext))
        .bind(LEGACY_NONCE.as_slice())
        .execute(pool)
        .await
        .expect("a legacy row");
}

struct Disk {
    dir: tempfile::TempDir,
    url: String,
}

impl Disk {
    async fn with_empty_schema() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let url = format!("sqlite:{}", dir.path().join("test.db").display());
        let disk = Self { dir, url };
        disk.boot(TEST_KEY).await.shutdown().await;
        disk
    }

    async fn boot(&self, key: [u8; 32]) -> SqliteBackend {
        SqliteBackend::open_for_test(&self.url, key, self.dir.path().join("media"), None)
            .await
            .expect("the backend opens")
    }

    async fn side(&self) -> SqlitePool {
        SqlitePool::connect(&self.url).await.expect("a side pool")
    }
}

#[tokio::test]
async fn a_row_bound_to_one_id_does_not_decrypt_under_another() {
    let pools = common::migrated_pools().await;
    let repo = SqliteCredentialsRepo::new_with_key(pools.clone(), TEST_KEY);
    repo.store(&CredentialId::new(BROADCASTER), "broadcaster_token")
        .await
        .expect("store");
    sqlx::query(
        "INSERT INTO credentials (id, encrypted, nonce, last_refresh)
         SELECT ?, encrypted, nonce, last_refresh FROM credentials WHERE id = ?",
    )
    .bind(BOT)
    .bind(BROADCASTER)
    .execute(pools.writer())
    .await
    .expect("the row is copied under the other id");

    let result = repo.load(&CredentialId::new(BOT)).await;

    assert!(
        matches!(result, Err(StorageError::Decryption)),
        "expected StorageError::Decryption, got {result:?}"
    );
}

#[tokio::test]
async fn a_row_sealed_before_id_binding_fails_with_decryption_so_the_user_reauthenticates() {
    let disk = Disk::with_empty_schema().await;
    let side = disk.side().await;
    seed_legacy(&side, BOT, &TEST_KEY, "legacy_token").await;

    let backend = disk.boot(TEST_KEY).await;
    let result = backend
        .credentials_repo()
        .load(&CredentialId::new(BOT))
        .await;

    assert!(
        matches!(&result, Err(e) if e.stored_secret_unrecoverable()),
        "{result:?}"
    );
}

#[tokio::test]
async fn storing_over_an_unreadable_legacy_row_makes_it_readable_again() {
    let pools = common::migrated_pools().await;
    seed_legacy(pools.writer(), BOT, &TEST_KEY, "legacy_token").await;
    let repo = SqliteCredentialsRepo::new_with_key(pools.clone(), TEST_KEY);
    let id = CredentialId::new(BOT);

    repo.store(&id, "fresh_token").await.expect("store");

    assert_eq!(
        repo.load(&id).await.expect("load").as_deref(),
        Some("fresh_token")
    );
}
