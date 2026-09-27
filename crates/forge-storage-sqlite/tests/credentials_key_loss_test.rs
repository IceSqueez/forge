#![allow(clippy::expect_used, clippy::unwrap_used)]

use forge_storage::{CredentialId, CredentialsKeyLoss, CredentialsRepo, take_credentials_key_loss};
use forge_storage_sqlite::SqliteBackend;

static ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[allow(unsafe_code)]
async fn open_with_key_file(url: &str, key_file: &std::path::Path) -> SqliteBackend {
    // SAFETY: the caller holds ENV_LOCK for the duration of the open below (see ENV_LOCK's
    unsafe {
        std::env::set_var("FORGE_CREDENTIAL_KEY_FILE", key_file);
    }
    SqliteBackend::open(url).await.expect("open backend")
}

#[tokio::test]
async fn missing_key_file_over_an_existing_credential_reports_the_loss_once_and_keeps_the_row() {
    let _guard = ENV_LOCK.lock().await;
    let dir = tempfile::tempdir().expect("tempdir");
    let key_file = dir.path().join("credentials-key");
    let url = format!("sqlite:{}", dir.path().join("test.db").display());
    let id = CredentialId::new("twitch:broadcaster");

    {
        let backend = open_with_key_file(&url, &key_file).await;
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

    let backend = open_with_key_file(&url, &key_file).await;

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
    let _guard = ENV_LOCK.lock().await;
    let dir = tempfile::tempdir().expect("tempdir");
    let key_file = dir.path().join("credentials-key");
    let url = format!("sqlite:{}", dir.path().join("test.db").display());

    let backend = open_with_key_file(&url, &key_file).await;

    assert_eq!(
        take_credentials_key_loss(&backend).await.expect("take"),
        None
    );
}

#[tokio::test]
async fn an_existing_key_file_over_an_existing_credential_reports_nothing() {
    let _guard = ENV_LOCK.lock().await;
    let dir = tempfile::tempdir().expect("tempdir");
    let key_file = dir.path().join("credentials-key");
    let url = format!("sqlite:{}", dir.path().join("test.db").display());
    let id = CredentialId::new("twitch:broadcaster");

    {
        let backend = open_with_key_file(&url, &key_file).await;
        backend
            .store(&id, "secret")
            .await
            .expect("seed a credential");
    }
    let backend = open_with_key_file(&url, &key_file).await;

    assert_eq!(
        take_credentials_key_loss(&backend).await.expect("take"),
        None
    );
}
