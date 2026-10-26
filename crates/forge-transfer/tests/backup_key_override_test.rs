use std::sync::Arc;

use forge_storage::paths::CREDENTIALS_KEY_FILE_NAME;
use forge_storage::paths::MEDIA_DIR_NAME;
use forge_storage::{CredentialId, CredentialsRepo, DATABASE_FILE_NAME, DataProvider};
use forge_storage_sqlite::{SqliteBackend, SqliteCredentialsRepo, SqliteDatabaseFileSchema};
use forge_transfer::{
    BackupDestination, FullBackupRequest, Passphrase, StageRequest, stage_full_restore,
    write_full_backup,
};

const KEY_FILE_ENV: &str = "FORGE_CREDENTIAL_KEY_FILE";
const PASSPHRASE: &str = "correct horse battery staple";
const SQLITE_KEY: [u8; 32] = [0x42; 32];

#[expect(unsafe_code, reason = "env mutation is unsafe in edition 2024")]
#[tokio::test]
async fn full_backup_packs_the_overridden_credentials_key_instead_of_the_data_dir_one() {
    let data = tempfile::tempdir().unwrap();
    let keys = tempfile::tempdir().unwrap();
    let overridden = keys.path().join("forge.key");
    std::fs::write(&overridden, b"live overridden key").unwrap();
    std::fs::write(
        data.path().join(CREDENTIALS_KEY_FILE_NAME),
        b"stale data dir key",
    )
    .unwrap();
    let url = format!(
        "sqlite://{}",
        data.path().join(DATABASE_FILE_NAME).display()
    );
    let backend =
        SqliteBackend::open_for_test(&url, SQLITE_KEY, data.path().join(MEDIA_DIR_NAME), None)
            .await
            .unwrap();
    let pool = forge_storage_sqlite::connect(&url).await.unwrap();
    SqliteCredentialsRepo::new_with_key(pool.clone(), SQLITE_KEY)
        .store(&CredentialId::new("twitch:broadcaster"), "token")
        .await
        .unwrap();
    pool.close().await;
    let provider: Arc<dyn DataProvider> = Arc::new(backend);
    // SAFETY: this is the only test in this binary, so no other thread reads or writes the environment while it runs.
    unsafe { std::env::set_var(KEY_FILE_ENV, &overridden) };

    let written = write_full_backup(FullBackupRequest {
        provider: provider.clone(),
        data_dir: data.path().to_path_buf(),
        destination: BackupDestination::BackupsDir,
        passphrase: Passphrase::new(PASSPHRASE.to_owned()),
    })
    .await;

    // SAFETY: still the only test in this binary; the write above has returned.
    unsafe { std::env::remove_var(KEY_FILE_ENV) };
    provider.shutdown().await;
    let staged = stage_full_restore(StageRequest {
        bundle: written.unwrap(),
        data_dir: data.path().to_path_buf(),
        passphrase: Some(Passphrase::new(PASSPHRASE.to_owned())),
        database_schema: Arc::new(SqliteDatabaseFileSchema),
    })
    .await
    .unwrap();
    assert_eq!(
        std::fs::read(staged.credentials_key_file().unwrap()).unwrap(),
        b"live overridden key"
    );
}
