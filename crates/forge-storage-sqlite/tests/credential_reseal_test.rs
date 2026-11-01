#![expect(clippy::expect_used, reason = "test fixtures")]
use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use forge_storage::{CredentialId, DataProvider, StorageError};
use forge_storage_sqlite::SqliteBackend;
use sqlx::SqlitePool;

const NONCE: [u8; 12] = [7; 12];

struct Fixture {
    dir: tempfile::TempDir,
    url: String,
    key: [u8; 32],
}

impl Fixture {
    async fn new() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let url = format!("sqlite:{}", dir.path().join("test.db").display());
        let fixture = Self {
            dir,
            url,
            key: [0; 32],
        };
        drop(fixture.open().await);
        let hex = std::fs::read_to_string(fixture.dir.path().join("credentials-key"))
            .expect("read key file");
        let mut key = [0u8; 32];
        for (i, chunk) in hex.trim().as_bytes().chunks(2).enumerate() {
            let pair = std::str::from_utf8(chunk).expect("utf8");
            key[i] = u8::from_str_radix(pair, 16).expect("hex");
        }
        Self { key, ..fixture }
    }

    async fn open(&self) -> SqliteBackend {
        SqliteBackend::open(
            &self.url,
            &forge_paths::AppPaths::at(self.dir.path().to_path_buf()),
        )
        .await
        .expect("open backend")
    }

    async fn pool(&self) -> SqlitePool {
        SqlitePool::connect(&self.url).await.expect("connect")
    }

    async fn insert_raw(&self, id: &str, key: &[u8; 32], aad: &str, plaintext: &str) {
        let sealed = seal(key, aad, plaintext);
        let pool = self.pool().await;
        sqlx::query(
            "INSERT INTO credentials (id, encrypted, nonce, last_refresh) VALUES (?, ?, ?, 0)",
        )
        .bind(id)
        .bind(sealed)
        .bind(NONCE.to_vec())
        .execute(&pool)
        .await
        .expect("insert");
        pool.close().await;
    }

    async fn raw_row(&self, id: &str) -> (Vec<u8>, Vec<u8>) {
        let pool = self.pool().await;
        let row = sqlx::query_as("SELECT encrypted, nonce FROM credentials WHERE id = ?")
            .bind(id)
            .fetch_one(&pool)
            .await
            .expect("row");
        pool.close().await;
        row
    }
}

fn seal(key: &[u8; 32], aad: &str, plaintext: &str) -> Vec<u8> {
    Aes256Gcm::new_from_slice(key)
        .expect("cipher")
        .encrypt(
            <&Nonce<_>>::try_from(NONCE.as_slice()).expect("nonce"),
            Payload {
                msg: plaintext.as_bytes(),
                aad: aad.as_bytes(),
            },
        )
        .expect("encrypt")
}

#[tokio::test]
async fn an_unbound_row_becomes_readable_and_a_second_boot_leaves_it_untouched() {
    let fixture = Fixture::new().await;
    fixture
        .insert_raw("twitch:broadcaster", &fixture.key, "", "old-token")
        .await;
    let id = CredentialId::new("twitch:broadcaster");

    let backend = fixture.open().await;
    assert_eq!(
        backend
            .credentials_repo()
            .load(&id)
            .await
            .expect("load")
            .as_deref(),
        Some("old-token")
    );
    drop(backend);
    let after_first = fixture.raw_row("twitch:broadcaster").await;

    let backend = fixture.open().await;
    assert_eq!(
        backend
            .credentials_repo()
            .load(&id)
            .await
            .expect("load again")
            .as_deref(),
        Some("old-token")
    );
    drop(backend);
    assert_eq!(fixture.raw_row("twitch:broadcaster").await, after_first);
}

#[tokio::test]
async fn a_row_sealed_with_another_key_is_left_as_it_is() {
    let fixture = Fixture::new().await;
    fixture
        .insert_raw("kick:broadcaster", &[9; 32], "", "foreign")
        .await;
    let before = fixture.raw_row("kick:broadcaster").await;

    let backend = fixture.open().await;
    let loaded = backend
        .credentials_repo()
        .load(&CredentialId::new("kick:broadcaster"))
        .await;
    drop(backend);

    assert!(matches!(loaded, Err(StorageError::Decryption)));
    assert_eq!(fixture.raw_row("kick:broadcaster").await, before);
}

#[tokio::test]
async fn a_bound_copy_under_another_id_is_still_refused() {
    let fixture = Fixture::new().await;
    fixture
        .insert_raw("youtube:a", &fixture.key, "youtube:a", "secret")
        .await;
    fixture
        .insert_raw("youtube:b", &fixture.key, "youtube:a", "secret")
        .await;
    let before = fixture.raw_row("youtube:b").await;

    let backend = fixture.open().await;
    let repo = backend.credentials_repo();
    let own = repo.load(&CredentialId::new("youtube:a")).await;
    let copied = repo.load(&CredentialId::new("youtube:b")).await;
    drop(backend);

    assert_eq!(own.expect("own").as_deref(), Some("secret"));
    assert!(matches!(copied, Err(StorageError::Decryption)));
    assert_eq!(fixture.raw_row("youtube:b").await, before);
}
