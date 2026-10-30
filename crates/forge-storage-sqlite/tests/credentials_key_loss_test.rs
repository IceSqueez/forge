#![allow(clippy::expect_used)]
use forge_storage::{CredentialId, CredentialsKeyLoss, CredentialsRepo, take_credentials_key_loss};
use forge_storage_sqlite::SqliteBackend;

const DATA_DIR_ENV: &str = "FORGE_DATA_DIR";
const KEY_FILE_ENV: &str = "FORGE_CREDENTIAL_KEY_FILE";

static ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

struct ScratchEnv {
    _held: tokio::sync::MutexGuard<'static, ()>,
}

impl ScratchEnv {
    #[expect(unsafe_code, reason = "env mutation is unsafe in edition 2024")]
    async fn pointing_at(dir: &std::path::Path, key_file: &std::path::Path) -> Self {
        let held = ENV_LOCK.lock().await;
        // SAFETY: ENV_LOCK stays held for the life of the returned guard and every test in this binary takes it first, so no other thread touches the environment here.
        unsafe {
            std::env::set_var(DATA_DIR_ENV, dir);
            std::env::set_var(KEY_FILE_ENV, key_file);
        }
        Self { _held: held }
    }
}

impl Drop for ScratchEnv {
    #[expect(unsafe_code, reason = "env mutation is unsafe in edition 2024")]
    fn drop(&mut self) {
        // SAFETY: the `_held` field drops after this body, so ENV_LOCK still guards the environment here.
        unsafe {
            std::env::remove_var(DATA_DIR_ENV);
            std::env::remove_var(KEY_FILE_ENV);
        }
    }
}

async fn open(url: &str) -> SqliteBackend {
    SqliteBackend::open(url).await.expect("open backend")
}

#[tokio::test]
async fn missing_key_file_over_an_existing_credential_reports_the_loss_once_and_keeps_the_row() {
    let dir = tempfile::tempdir().expect("tempdir");
    let key_file = dir.path().join("credentials-key");
    let _env = ScratchEnv::pointing_at(dir.path(), &key_file).await;
    let url = format!("sqlite:{}", dir.path().join("test.db").display());
    let id = CredentialId::new("twitch:broadcaster");

    {
        let backend = open(&url).await;
        backend
            .store(&id, "secret")
            .await
            .expect("seed a credential under the first key");
        assert_eq!(
            take_credentials_key_loss(&backend).await.expect("take"),
            None,
            "the first boot mints the key with nothing stranded yet"
        );
    }
    std::fs::remove_file(&key_file).expect("drop the key file to simulate a lost key");

    let backend = open(&url).await;

    assert_eq!(
        take_credentials_key_loss(&backend).await.expect("take"),
        Some(CredentialsKeyLoss { stranded: 1 }),
        "the row written under the old key must be reported once"
    );
    assert_eq!(
        take_credentials_key_loss(&backend)
            .await
            .expect("take again"),
        None,
        "the report must clear itself"
    );
    let ids = backend.list_ids().await.expect("list_ids");
    assert!(
        ids.contains(&id),
        "the stranded row must stay in place, not be deleted"
    );
}

#[tokio::test]
async fn missing_key_file_over_an_empty_credentials_table_reports_nothing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let key_file = dir.path().join("credentials-key");
    let _env = ScratchEnv::pointing_at(dir.path(), &key_file).await;
    let url = format!("sqlite:{}", dir.path().join("test.db").display());

    let backend = open(&url).await;

    assert_eq!(
        take_credentials_key_loss(&backend).await.expect("take"),
        None
    );
}

#[tokio::test]
async fn an_existing_key_file_over_an_existing_credential_reports_nothing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let key_file = dir.path().join("credentials-key");
    let _env = ScratchEnv::pointing_at(dir.path(), &key_file).await;
    let url = format!("sqlite:{}", dir.path().join("test.db").display());
    let id = CredentialId::new("twitch:broadcaster");

    {
        let backend = open(&url).await;
        backend
            .store(&id, "secret")
            .await
            .expect("seed a credential");
    }
    let backend = open(&url).await;

    assert_eq!(
        take_credentials_key_loss(&backend).await.expect("take"),
        None
    );
}

#[tokio::test]
async fn missing_key_file_over_only_server_issued_tokens_reports_nothing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let key_file = dir.path().join("credentials-key");
    let _env = ScratchEnv::pointing_at(dir.path(), &key_file).await;
    let url = format!("sqlite:{}", dir.path().join("test.db").display());

    {
        let backend = open(&url).await;
        for id in [
            forge_storage::SERVER_BEARER_CREDENTIAL_ID,
            forge_storage::SERVER_OVERLAY_TOKEN_CREDENTIAL_ID,
        ] {
            backend
                .store(&CredentialId::new(id), "server-issued")
                .await
                .expect("seed a server-issued token");
        }
    }
    std::fs::remove_file(&key_file).expect("drop the key file to simulate a lost key");

    let backend = open(&url).await;

    assert_eq!(
        take_credentials_key_loss(&backend).await.expect("take"),
        None,
        "the server re-issues its own tokens, so no login was lost"
    );
}
