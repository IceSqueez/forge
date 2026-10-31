#![allow(clippy::unwrap_used, clippy::panic)]
mod common;
use common::{SQLITE_KEY, Tree, files_under, write_file};

use std::path::{Path, PathBuf};
use std::sync::Arc;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use forge_paths::{
    ASSETS_DIR_NAME, CREDENTIALS_KEY_FILE_NAME, MEDIA_DIR_NAME, OVERLAYS_DIR_NAME, TTS_DIR_NAME,
};
use forge_storage::{CredentialId, CredentialsRepo, DATABASE_FILE_NAME, DataProvider};
use forge_storage_sqlite::{SqliteCredentialsRepo, SqliteDatabaseFileSchema};
use forge_transfer::{
    BUNDLE_EXTENSION, BackupCategory, BundleOrigin, BundleSummary, CancelFlag, Passphrase,
    PublicManifest, RestoreOutcome, ScheduleKey, ScheduledBackupRequest, ScheduledWrite,
    StageRequest, StagedRestore, TransferError, UpdateActivity, apply_pending_restore,
    change_schedule_passphrase, create_schedule_key, read_bundle_summary, record_pending_restore,
    restore_staging_root, stage_full_restore, write_scheduled_backup,
    write_scheduled_backup_cancellable, write_scheduled_backup_unless_unchanged,
};
use tempfile::TempDir;

const PASSPHRASE: &str = "correct horse battery staple";
const NEW_PASSPHRASE: &str = "incorrect donkey battery clip";
const KEY_BYTES: &[u8] = b"thirty-two bytes of credential k";
const SOURCE_MEDIA: &[u8] = b"scheduled media";
const AGE_SCRYPT_HEADER: &[u8] = b"age-encryption.org/v1\n-> scrypt ";
const AGE_SECRET_KEY_PREFIX: &str = "AGE-SECRET-KEY-";

fn passphrase(text: &str) -> Passphrase {
    Passphrase::new(text.to_owned())
}

struct Source {
    dir: TempDir,
    provider: Arc<dyn DataProvider>,
}

impl Source {
    async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        std::fs::create_dir_all(&data).unwrap();
        let url = common::database_url(&data);
        let backend = common::open_backend(&data).await;
        let pool = forge_storage_sqlite::connect(&url).await.unwrap();
        SqliteCredentialsRepo::new_with_key(pool.clone(), SQLITE_KEY)
            .store(&CredentialId::new("twitch:broadcaster"), "token")
            .await
            .unwrap();
        pool.close().await;
        write_file(&data.join(CREDENTIALS_KEY_FILE_NAME), KEY_BYTES);
        write_file(&data.join(MEDIA_DIR_NAME).join("clip.ogg"), SOURCE_MEDIA);
        write_file(
            &data.join(OVERLAYS_DIR_NAME).join("o.html"),
            b"source overlay",
        );
        write_file(&data.join(TTS_DIR_NAME).join("v.onnx"), b"source voice");
        write_file(&data.join(ASSETS_DIR_NAME).join("a.png"), b"source asset");
        std::fs::create_dir_all(dir.path().join("out")).unwrap();
        Self {
            dir,
            provider: Arc::new(backend),
        }
    }

    fn data_dir(&self) -> PathBuf {
        self.dir.path().join("data")
    }

    fn folder(&self) -> PathBuf {
        self.dir.path().join("out")
    }

    async fn write_to(
        &self,
        folder: PathBuf,
        key: &ScheduleKey,
        categories: Vec<BackupCategory>,
    ) -> Result<PathBuf, TransferError> {
        write_scheduled_backup(ScheduledBackupRequest {
            provider: self.provider.clone(),
            data_dir: self.data_dir(),
            folder,
            key: key.clone(),
            categories,
        })
        .await
    }

    async fn write(&self, key: &ScheduleKey, categories: Vec<BackupCategory>) -> PathBuf {
        self.write_to(self.folder(), key, categories).await.unwrap()
    }
}

async fn stage(
    bundle: &Path,
    data_dir: &Path,
    secret: &str,
) -> Result<StagedRestore, TransferError> {
    stage_full_restore(StageRequest {
        bundle: bundle.to_path_buf(),
        data_dir: data_dir.to_path_buf(),
        passphrase: Some(passphrase(secret)),
        database_schema: Arc::new(SqliteDatabaseFileSchema),
    })
    .await
}

async fn public_manifest(bundle: &Path) -> PublicManifest {
    match read_bundle_summary(bundle.to_path_buf()).await.unwrap() {
        BundleSummary::Encrypted(manifest) => *manifest,
        other => panic!("expected an encrypted bundle, got {other:?}"),
    }
}

fn media_only() -> Vec<BackupCategory> {
    vec![BackupCategory::Media]
}

#[tokio::test]
async fn scheduled_bundle_stages_only_its_categories_with_the_passphrase() {
    let source = Source::new().await;
    let key = create_schedule_key(&passphrase(PASSPHRASE)).unwrap();
    let bundle = source.write(&key, media_only()).await;
    let target = tempfile::tempdir().unwrap();

    let staged = stage(&bundle, target.path(), PASSPHRASE).await.unwrap();

    let mut staged_files = files_under(&staged.payload_dir());
    staged_files.remove(DATABASE_FILE_NAME);
    let expected: Tree = [
        (CREDENTIALS_KEY_FILE_NAME.to_owned(), KEY_BYTES.to_vec()),
        (format!("{MEDIA_DIR_NAME}/clip.ogg"), SOURCE_MEDIA.to_vec()),
    ]
    .into();
    assert_eq!(staged_files, expected);
    source.provider.shutdown().await;
}

#[tokio::test]
async fn scheduled_bundle_is_named_and_labelled_as_scheduled() {
    let source = Source::new().await;
    let key = create_schedule_key(&passphrase(PASSPHRASE)).unwrap();

    let bundle = source.write(&key, media_only()).await;

    let name = bundle.file_name().unwrap().to_string_lossy().into_owned();
    let manifest = public_manifest(&bundle).await;
    assert!(name.starts_with("forge-scheduled-"), "{name}");
    assert!(name.ends_with(&format!(".{BUNDLE_EXTENSION}")), "{name}");
    assert_eq!(bundle.parent().unwrap(), source.folder());
    assert_eq!(manifest.origin, BundleOrigin::Scheduled);
    assert!(manifest.encryption.is_wrapped_recipient());
    assert_eq!(manifest.key_fingerprint, Some(key.fingerprint()));
    assert_eq!(
        manifest.wrapped_identity.as_deref(),
        Some(key.wrapped_identity.as_str())
    );
    source.provider.shutdown().await;
}

#[tokio::test]
async fn scheduled_bundle_refuses_a_wrong_passphrase_and_leaves_no_staging() {
    let source = Source::new().await;
    let key = create_schedule_key(&passphrase(PASSPHRASE)).unwrap();
    let bundle = source.write(&key, media_only()).await;
    let target = tempfile::tempdir().unwrap();

    let error = stage(&bundle, target.path(), NEW_PASSPHRASE)
        .await
        .unwrap_err();

    assert!(matches!(error, TransferError::WrongPassphrase), "{error:?}");
    assert_eq!(
        files_under(&restore_staging_root(target.path())),
        Tree::new()
    );
    source.provider.shutdown().await;
}

#[test]
fn schedule_key_holds_the_private_key_only_sealed_by_the_passphrase() {
    let key = create_schedule_key(&passphrase(PASSPHRASE)).unwrap();

    let sealed = STANDARD.decode(&key.wrapped_identity).unwrap();

    assert!(key.recipient.starts_with("age1"), "{}", key.recipient);
    assert!(sealed.starts_with(AGE_SCRYPT_HEADER));
    assert!(!key.recipient.contains(AGE_SECRET_KEY_PREFIX));
    assert!(!String::from_utf8_lossy(&sealed).contains(AGE_SECRET_KEY_PREFIX));
}

#[tokio::test]
async fn the_recipient_alone_does_not_open_a_scheduled_bundle() {
    let source = Source::new().await;
    let key = create_schedule_key(&passphrase(PASSPHRASE)).unwrap();
    let bundle = source.write(&key, media_only()).await;
    let target = tempfile::tempdir().unwrap();

    let error = stage(&bundle, target.path(), &key.recipient)
        .await
        .unwrap_err();

    assert!(matches!(error, TransferError::WrongPassphrase), "{error:?}");
    source.provider.shutdown().await;
}

#[test]
fn creating_a_schedule_key_refuses_a_short_passphrase() {
    let error = create_schedule_key(&passphrase("eleven char")).unwrap_err();

    assert!(matches!(error, TransferError::PassphraseTooShort { .. }));
}

#[tokio::test]
async fn passphrase_change_seals_new_bundles_to_the_new_passphrase_and_old_ones_keep_the_old() {
    let source = Source::new().await;
    let old_key = create_schedule_key(&passphrase(PASSPHRASE)).unwrap();
    let old_bundle = source.write(&old_key, media_only()).await;
    let new_key = change_schedule_passphrase(
        &old_key.wrapped_identity,
        &passphrase(PASSPHRASE),
        &passphrase(NEW_PASSPHRASE),
    )
    .unwrap();
    let new_bundle = source.write(&new_key, media_only()).await;
    let cases = [
        (
            "new bundle, new passphrase",
            &new_bundle,
            NEW_PASSPHRASE,
            true,
        ),
        ("new bundle, old passphrase", &new_bundle, PASSPHRASE, false),
        ("old bundle, old passphrase", &old_bundle, PASSPHRASE, true),
        (
            "old bundle, new passphrase",
            &old_bundle,
            NEW_PASSPHRASE,
            false,
        ),
    ];

    assert_ne!(new_key.recipient, old_key.recipient);
    for (case, bundle, secret, opens) in cases {
        let target = tempfile::tempdir().unwrap();
        let staged = stage(bundle, target.path(), secret).await;
        match staged {
            Ok(_) => assert!(opens, "{case}: opened"),
            Err(error) => assert!(
                !opens && matches!(error, TransferError::WrongPassphrase),
                "{case}: {error:?}"
            ),
        }
    }
    source.provider.shutdown().await;
}

#[test]
fn passphrase_change_refuses_a_wrong_current_a_short_replacement_or_an_unreadable_key() {
    let key = create_schedule_key(&passphrase(PASSPHRASE)).unwrap();
    type Expected = fn(&TransferError) -> bool;
    let cases: [(&str, &str, &str, &str, Expected); 3] = [
        (
            "wrong current passphrase",
            key.wrapped_identity.as_str(),
            NEW_PASSPHRASE,
            NEW_PASSPHRASE,
            |e| matches!(e, TransferError::WrongPassphrase),
        ),
        (
            "short replacement",
            key.wrapped_identity.as_str(),
            PASSPHRASE,
            "eleven char",
            |e| matches!(e, TransferError::PassphraseTooShort { .. }),
        ),
        (
            "unreadable stored key",
            "not base64 !",
            PASSPHRASE,
            NEW_PASSPHRASE,
            |e| matches!(e, TransferError::Damaged { .. }),
        ),
    ];
    for (case, wrapped, current, replacement, expected) in cases {
        let error =
            change_schedule_passphrase(wrapped, &passphrase(current), &passphrase(replacement))
                .unwrap_err();
        assert!(expected(&error), "{case}: {error:?}");
    }
}

#[tokio::test]
async fn database_is_always_part_of_a_scheduled_bundle() {
    let source = Source::new().await;
    let key = create_schedule_key(&passphrase(PASSPHRASE)).unwrap();
    let cases = [
        (vec![], vec![BackupCategory::Database]),
        (
            vec![BackupCategory::Assets, BackupCategory::Media],
            vec![
                BackupCategory::Database,
                BackupCategory::Media,
                BackupCategory::Assets,
            ],
        ),
    ];
    for (requested, recorded) in cases {
        let bundle = source.write(&key, requested.clone()).await;
        let manifest = public_manifest(&bundle).await;
        assert_eq!(manifest.categories, recorded, "{requested:?}");
    }
    source.provider.shutdown().await;
}

#[tokio::test]
async fn missing_destination_folder_fails_without_being_created() {
    let source = Source::new().await;
    let key = create_schedule_key(&passphrase(PASSPHRASE)).unwrap();
    let missing = source.dir.path().join("unplugged-drive").join("backups");

    let result = source.write_to(missing.clone(), &key, media_only()).await;

    assert!(matches!(result, Err(TransferError::Io(_))), "{result:?}");
    assert!(!missing.exists());
    assert!(!missing.parent().unwrap().exists());
    source.provider.shutdown().await;
}

fn plant_target(root: &Path) {
    write_file(&root.join(DATABASE_FILE_NAME), b"target database");
    write_file(&root.join(CREDENTIALS_KEY_FILE_NAME), b"target key");
    write_file(&root.join(MEDIA_DIR_NAME).join("old.ogg"), b"target media");
    write_file(
        &root.join(OVERLAYS_DIR_NAME).join("t.html"),
        b"target overlay",
    );
    write_file(&root.join(TTS_DIR_NAME).join("t.onnx"), b"target voice");
    write_file(&root.join(ASSETS_DIR_NAME).join("t.png"), b"target asset");
}

fn excluded_folders(tree: &Tree) -> Tree {
    tree.iter()
        .filter(|(path, _)| {
            [OVERLAYS_DIR_NAME, TTS_DIR_NAME, ASSETS_DIR_NAME]
                .iter()
                .any(|dir| path.starts_with(&format!("{dir}/")))
        })
        .map(|(path, bytes)| (path.clone(), bytes.clone()))
        .collect()
}

#[tokio::test]
async fn applying_a_scheduled_restore_leaves_excluded_categories_untouched() {
    let source = Source::new().await;
    let key = create_schedule_key(&passphrase(PASSPHRASE)).unwrap();
    let bundle = source.write(&key, media_only()).await;
    source.provider.shutdown().await;
    let target = tempfile::tempdir().unwrap();
    plant_target(target.path());
    let before = excluded_folders(&files_under(target.path()));
    let staged = stage(&bundle, target.path(), PASSPHRASE).await.unwrap();
    record_pending_restore(target.path(), &staged, UpdateActivity::Idle)
        .await
        .unwrap();

    let outcome = apply_pending_restore(target.path()).unwrap();

    let RestoreOutcome::Applied {
        pre_restore_snapshot,
    } = outcome
    else {
        panic!("expected Applied, got {outcome:?}");
    };
    let after = files_under(target.path());
    assert_eq!(excluded_folders(&after), before);
    assert_eq!(
        after.get(&format!("{MEDIA_DIR_NAME}/clip.ogg")),
        Some(&SOURCE_MEDIA.to_vec())
    );
    assert_eq!(
        excluded_folders(&files_under(&pre_restore_snapshot)),
        Tree::new()
    );
}

impl Source {
    async fn write_unless(
        &self,
        folder: PathBuf,
        key: &ScheduleKey,
        categories: Vec<BackupCategory>,
        unchanged_since: Option<String>,
    ) -> ScheduledWrite {
        write_scheduled_backup_unless_unchanged(
            ScheduledBackupRequest {
                provider: self.provider.clone(),
                data_dir: self.data_dir(),
                folder,
                key: key.clone(),
                categories,
            },
            unchanged_since,
        )
        .await
        .unwrap()
    }
}

fn fingerprint_of(written: &ScheduledWrite) -> String {
    match written {
        ScheduledWrite::Written { fingerprint, .. } | ScheduledWrite::Unchanged { fingerprint } => {
            fingerprint.clone()
        }
    }
}

fn bundles_in(folder: &Path) -> usize {
    std::fs::read_dir(folder)
        .unwrap()
        .filter(|entry| {
            entry
                .as_ref()
                .unwrap()
                .file_name()
                .to_string_lossy()
                .ends_with(&format!(".{BUNDLE_EXTENSION}"))
        })
        .count()
}

#[tokio::test]
async fn unchanged_content_since_the_last_fingerprint_writes_no_bundle() {
    let source = Source::new().await;
    let key = create_schedule_key(&passphrase(PASSPHRASE)).unwrap();
    let first = source
        .write_unless(source.folder(), &key, media_only(), None)
        .await;

    let second = source
        .write_unless(
            source.folder(),
            &key,
            media_only(),
            Some(fingerprint_of(&first)),
        )
        .await;

    assert_eq!(
        (second, bundles_in(&source.folder())),
        (
            ScheduledWrite::Unchanged {
                fingerprint: fingerprint_of(&first)
            },
            1
        )
    );
    source.provider.shutdown().await;
}

#[tokio::test]
async fn any_change_to_included_content_key_categories_or_folder_writes_a_bundle() {
    let source = Source::new().await;
    let key = create_schedule_key(&passphrase(PASSPHRASE)).unwrap();
    let other_key = create_schedule_key(&passphrase(PASSPHRASE)).unwrap();
    let other_folder = source.dir.path().join("other-out");
    std::fs::create_dir_all(&other_folder).unwrap();
    let baseline = fingerprint_of(
        &source
            .write_unless(source.folder(), &key, media_only(), None)
            .await,
    );
    let with_overlays = vec![BackupCategory::Media, BackupCategory::Overlays];
    for (label, folder, key, categories) in [
        ("other key", source.folder(), &other_key, media_only()),
        ("other folder", other_folder.clone(), &key, media_only()),
        ("more categories", source.folder(), &key, with_overlays),
    ] {
        let written = source
            .write_unless(folder, key, categories, Some(baseline.clone()))
            .await;
        assert!(
            matches!(written, ScheduledWrite::Written { .. }),
            "{label}: {written:?}"
        );
    }
    source.provider.shutdown().await;
}

#[tokio::test]
async fn an_edited_included_file_writes_a_bundle() {
    let source = Source::new().await;
    let key = create_schedule_key(&passphrase(PASSPHRASE)).unwrap();
    let baseline = fingerprint_of(
        &source
            .write_unless(source.folder(), &key, media_only(), None)
            .await,
    );
    write_file(
        &source.data_dir().join(MEDIA_DIR_NAME).join("clip.ogg"),
        b"edited media",
    );

    let written = source
        .write_unless(source.folder(), &key, media_only(), Some(baseline))
        .await;

    assert!(
        matches!(written, ScheduledWrite::Written { .. }),
        "{written:?}"
    );
    source.provider.shutdown().await;
}

#[tokio::test]
async fn an_edit_in_an_excluded_category_does_not_count_as_a_change() {
    let source = Source::new().await;
    let key = create_schedule_key(&passphrase(PASSPHRASE)).unwrap();
    let baseline = fingerprint_of(
        &source
            .write_unless(source.folder(), &key, media_only(), None)
            .await,
    );
    write_file(
        &source.data_dir().join(OVERLAYS_DIR_NAME).join("o.html"),
        b"edited overlay",
    );

    let written = source
        .write_unless(source.folder(), &key, media_only(), Some(baseline))
        .await;

    assert!(
        matches!(written, ScheduledWrite::Unchanged { .. }),
        "{written:?}"
    );
    source.provider.shutdown().await;
}

#[tokio::test]
async fn a_cancelled_scheduled_write_is_interrupted_and_leaves_no_file_behind() {
    let source = Source::new().await;
    let key = create_schedule_key(&passphrase(PASSPHRASE)).unwrap();
    let cancel = CancelFlag::new();
    cancel.raise();

    let result = write_scheduled_backup_cancellable(
        ScheduledBackupRequest {
            provider: source.provider.clone(),
            data_dir: source.data_dir(),
            folder: source.folder(),
            key,
            categories: media_only(),
        },
        None,
        cancel,
    )
    .await;

    let leftovers = (
        files_under(&source.folder()),
        files_under(&source.data_dir().join(forge_paths::BACKUPS_DIR_NAME)),
    );
    assert!(
        matches!(result, Err(TransferError::Interrupted { .. })),
        "{result:?}"
    );
    assert_eq!(leftovers, (Tree::new(), Tree::new()));
    source.provider.shutdown().await;
}
