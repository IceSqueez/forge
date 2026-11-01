#![expect(clippy::expect_used, reason = "test fixtures")]
use forge_storage::{CredentialId, CredentialsKeyLoss, CredentialsRepo, take_credentials_key_loss};
use forge_storage_sqlite::SqliteBackend;

async fn open(url: &str, dir: &std::path::Path) -> SqliteBackend {
    SqliteBackend::open(url, &forge_paths::AppPaths::at(dir.to_path_buf()))
        .await
        .expect("open backend")
}

#[tokio::test]
async fn missing_key_file_over_an_existing_credential_reports_the_loss_once_and_keeps_the_row() {
    let dir = tempfile::tempdir().expect("tempdir");
    let key_file = dir.path().join("credentials-key");
    let url = format!("sqlite:{}", dir.path().join("test.db").display());
    let id = CredentialId::new("twitch:broadcaster");

    {
        let backend = open(&url, dir.path()).await;
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

    let backend = open(&url, dir.path()).await;

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
    let url = format!("sqlite:{}", dir.path().join("test.db").display());

    let backend = open(&url, dir.path()).await;

    assert_eq!(
        take_credentials_key_loss(&backend).await.expect("take"),
        None
    );
}

#[tokio::test]
async fn an_existing_key_file_over_an_existing_credential_reports_nothing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let url = format!("sqlite:{}", dir.path().join("test.db").display());
    let id = CredentialId::new("twitch:broadcaster");

    {
        let backend = open(&url, dir.path()).await;
        backend
            .store(&id, "secret")
            .await
            .expect("seed a credential");
    }
    let backend = open(&url, dir.path()).await;

    assert_eq!(
        take_credentials_key_loss(&backend).await.expect("take"),
        None
    );
}

#[tokio::test]
async fn missing_key_file_over_only_server_issued_tokens_reports_nothing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let key_file = dir.path().join("credentials-key");
    let url = format!("sqlite:{}", dir.path().join("test.db").display());

    {
        let backend = open(&url, dir.path()).await;
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

    let backend = open(&url, dir.path()).await;

    assert_eq!(
        take_credentials_key_loss(&backend).await.expect("take"),
        None,
        "the server re-issues its own tokens, so no login was lost"
    );
}
