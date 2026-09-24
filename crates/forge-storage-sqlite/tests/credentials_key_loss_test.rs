#![allow(clippy::expect_used, clippy::unwrap_used)]

use forge_storage::{CredentialId, CredentialsKeyLoss, CredentialsRepo, take_credentials_key_loss};
use forge_storage_sqlite::SqliteBackend;

// Why: `crypto::load_or_create_key` reads `FORGE_CREDENTIAL_KEY_FILE` from process-global
// environment, so every test in this file that opens a backend must control it. Kept in its
// own integration test binary (one process per `tests/*.rs` file) so no other test in the
// crate is exposed to the mutation, and serialized here on ENV_LOCK so this file's own tests
// never observe each other's key-file path.
static ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

// Test-only, single-purpose env mutation for a process-global read
// (`crypto::load_or_create_key`) that has no other injection point; every caller in this file
// holds ENV_LOCK for the duration, so the workspace-wide `unsafe_code` deny is narrowly lifted
// here rather than for the crate.
#[allow(unsafe_code)]
async fn open_with_key_file(url: &str, key_file: &std::path::Path) -> SqliteBackend {
    // SAFETY: the caller holds ENV_LOCK for the duration of the open below (see ENV_LOCK's
    // doc comment); no other thread in this process reads or writes
    // FORGE_CREDENTIAL_KEY_FILE while the guard is held.
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

    // The key file does not exist yet, so this open mints one; the credentials table is
    // empty (first-ever boot), so there is nothing to strand.
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
    // The key file still exists, so the second open must not mint a new one and must not
    // report a loss even though a credential row is present.
    let backend = open_with_key_file(&url, &key_file).await;

    assert_eq!(
        take_credentials_key_loss(&backend).await.expect("take"),
        None
    );
}
