use std::sync::Arc;

use forge_paths::CREDENTIALS_KEY_FILE_NAME;
use forge_storage::{CredentialId, CredentialsRepo, DataProvider};
use forge_storage_sqlite::{SqliteCredentialsRepo, SqliteDatabaseFileSchema};
use forge_transfer::{
    BackupDestination, FullBackupRequest, Passphrase, StageRequest, stage_full_restore,
    write_full_backup,
};

mod common;
use common::SQLITE_KEY;

const PASSPHRASE: &str = "correct horse battery staple";

const KEY_OVERRIDE_TEST: &str =
    "full_backup_packs_the_overridden_credentials_key_instead_of_the_data_dir_one";

#[tokio::test]
async fn full_backup_packs_the_overridden_credentials_key_instead_of_the_data_dir_one() {
    let Some(overridden) = common::key_file_of_this_child_run() else {
        let keys = tempfile::tempdir().unwrap();
        common::rerun_in_child_with_key_file(KEY_OVERRIDE_TEST, &keys.path().join("forge.key"));
        return;
    };
    let data = tempfile::tempdir().unwrap();
    std::fs::write(&overridden, b"live overridden key").unwrap();
    std::fs::write(
        data.path().join(CREDENTIALS_KEY_FILE_NAME),
        b"stale data dir key",
    )
    .unwrap();
    let url = common::database_url(data.path());
    let backend = common::open_backend(data.path()).await;
    let pool = forge_storage_sqlite::connect(&url).await.unwrap();
    SqliteCredentialsRepo::new_with_key(pool.clone(), SQLITE_KEY)
        .store(&CredentialId::new("twitch:broadcaster"), "token")
        .await
        .unwrap();
    pool.close().await;
    let provider: Arc<dyn DataProvider> = Arc::new(backend);

    let written = write_full_backup(FullBackupRequest {
        provider: provider.clone(),
        data_dir: data.path().to_path_buf(),
        destination: BackupDestination::BackupsDir,
        passphrase: Passphrase::new(PASSPHRASE.to_owned()),
    })
    .await;

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
