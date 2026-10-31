#![expect(clippy::expect_used, clippy::unwrap_used, reason = "test fixtures")]
mod common;
use common::{SQLITE_KEY, sha256_hex};

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use age::secrecy::SecretString;
use forge_paths::MEDIA_DIR_NAME;
use forge_storage::{
    ActionRepo, DATABASE_FILE_NAME, DataProvider, GlobalsRepo, QueueRepo, load_setting_rows,
};
use forge_storage_sqlite::{
    MIGRATIONS, SqliteActionRepo, SqliteBackend, SqliteDatabaseFileSchema, SqliteGlobalsRepo,
    SqliteQueueRepo, connect_pools,
};
use forge_transfer::{
    BUNDLE_FORMAT_VERSION, BackupCategory, BundleKind, BundleOrigin, ContentEntry,
    ENCRYPTED_PAYLOAD_ENTRY_NAME, EncryptionScheme, FullManifest, MANIFEST_ENTRY_NAME,
    PACK_DOCUMENTS_VERSION, Passphrase, PublicManifest, RestoreOutcome, StageRequest,
    TransferError, UpdateActivity, apply_pending_restore, record_pending_restore,
    stage_full_restore, stage_pack,
};
use forge_types::{Action, Queue, SubActionStep, Variant};
use serde_json::Value;
use tempfile::TempDir;
use time::OffsetDateTime;

const OLDER_SCHEMA_VERSION: i64 = 54;
const OLDER_FORGE_VERSION: &str = "0.6.3";
const PASSPHRASE: &str = "correct horse battery staple";
const CHEAP_WORK_FACTOR: u8 = 2;
const PACK_MANIFEST: &[u8] = include_bytes!("fixtures/older_pack/manifest.json");
const PACK_DOCUMENTS: &[u8] = include_bytes!("fixtures/older_pack/documents.json");
const PACK_DOCUMENTS_ENTRY: &str = "pack/documents.json";
const BACKUP_SEED: &str = include_str!("fixtures/older_backup/seed.json");
const BACKUP_DATABASE_ENTRY: &str = "forge/forge.db";
const FAVORITE_STEPS_KEY: &str = "picker.favorites.sub_actions";
const FAVORITE_STEPS_AFTER_RENAME: &str = r#"["core.logic.wait","obs.scene.switch_current"]"#;

fn add_file(builder: &mut tar::Builder<Vec<u8>>, path: &str, bytes: &[u8]) {
    let mut header = tar::Header::new_gnu();
    header.set_entry_type(tar::EntryType::Regular);
    header.set_size(bytes.len() as u64);
    header.set_mode(0o600);
    builder.append_data(&mut header, path, bytes).unwrap();
}

fn tar_gz(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut builder = tar::Builder::new(Vec::new());
    for (path, bytes) in files {
        add_file(&mut builder, path, bytes);
    }
    let tar = builder.into_inner().unwrap();
    common::gzip(&tar)
}

fn encrypt(plaintext: &[u8]) -> Vec<u8> {
    let mut recipient = age::scrypt::Recipient::new(SecretString::from(PASSPHRASE.to_owned()));
    recipient.set_work_factor(CHEAP_WORK_FACTOR);
    let encryptor =
        age::Encryptor::with_recipients(std::iter::once(&recipient as &dyn age::Recipient))
            .unwrap();
    let mut writer = encryptor.wrap_output(Vec::new()).unwrap();
    writer.write_all(plaintext).unwrap();
    writer.finish().unwrap()
}

fn older_created_at() -> OffsetDateTime {
    OffsetDateTime::from_unix_timestamp(1_761_328_800).unwrap()
}

fn older_full_backup(database: &[u8]) -> Vec<u8> {
    let schema_version = u32::try_from(OLDER_SCHEMA_VERSION).unwrap();
    let inner_manifest = serde_json::to_vec(&FullManifest {
        format_version: BUNDLE_FORMAT_VERSION,
        kind: BundleKind::Full,
        forge_version: OLDER_FORGE_VERSION.to_owned(),
        schema_version,
        created_at: older_created_at(),
        categories: BackupCategory::all(),
        contents: vec![ContentEntry {
            path: BACKUP_DATABASE_ENTRY.to_owned(),
            size: database.len() as u64,
            sha256_hex: sha256_hex(database),
        }],
    })
    .unwrap();
    let public_manifest = serde_json::to_vec(&PublicManifest {
        format_version: BUNDLE_FORMAT_VERSION,
        kind: BundleKind::Full,
        encryption: EncryptionScheme::age_scrypt(),
        forge_version: OLDER_FORGE_VERSION.to_owned(),
        schema_version,
        created_at: older_created_at(),
        origin: BundleOrigin::Manual,
        categories: BackupCategory::all(),
        key_fingerprint: None,
        wrapped_identity: None,
    })
    .unwrap();
    let payload = encrypt(&tar_gz(&[
        (MANIFEST_ENTRY_NAME, inner_manifest.as_slice()),
        (BACKUP_DATABASE_ENTRY, database),
    ]));
    tar_gz(&[
        (MANIFEST_ENTRY_NAME, public_manifest.as_slice()),
        (ENCRYPTED_PAYLOAD_ENTRY_NAME, payload.as_slice()),
    ])
}

fn database_url(path: &Path) -> String {
    format!("sqlite://{}", path.display())
}

async fn open_current(data_dir: &Path) -> Arc<dyn DataProvider> {
    let backend = SqliteBackend::open_for_test(
        &database_url(&data_dir.join(DATABASE_FILE_NAME)),
        SQLITE_KEY,
        data_dir.join(MEDIA_DIR_NAME),
        None,
    )
    .await
    .unwrap();
    Arc::new(backend)
}

#[tokio::test]
async fn a_pack_written_in_a_retired_document_format_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let data_dir = dir.path().join("data");
    std::fs::create_dir_all(&data_dir).unwrap();
    let bundle = dir.path().join("older.forgebundle");
    std::fs::write(
        &bundle,
        tar_gz(&[
            (MANIFEST_ENTRY_NAME, PACK_MANIFEST),
            (PACK_DOCUMENTS_ENTRY, PACK_DOCUMENTS),
        ]),
    )
    .unwrap();

    let refusal = stage_pack(bundle, data_dir).await.unwrap_err();

    assert!(
        matches!(
            refusal,
            TransferError::RetiredDocuments { found: 1, supported } if supported == PACK_DOCUMENTS_VERSION
        ),
        "unexpected refusal: {refusal:?}"
    );
}

struct BackupSeed {
    action: Action,
    queue: Queue,
    settings: BTreeMap<String, String>,
    globals: BTreeMap<String, Variant>,
}

fn backup_seed() -> BackupSeed {
    let seed: Value = serde_json::from_str(BACKUP_SEED).unwrap();
    BackupSeed {
        action: serde_json::from_value(seed["action"].clone()).unwrap(),
        queue: serde_json::from_value(seed["queue"].clone()).unwrap(),
        settings: serde_json::from_value(seed["settings"].clone()).unwrap(),
        globals: serde_json::from_value(seed["globals"].clone()).unwrap(),
    }
}

async fn older_database(path: &Path, seed: &BackupSeed) -> Vec<u8> {
    let pools = connect_pools(&database_url(path)).await.unwrap();
    MIGRATIONS
        .run_to(OLDER_SCHEMA_VERSION, pools.writer())
        .await
        .expect("the older schema applies");
    SqliteQueueRepo::new(pools.clone())
        .save(&seed.queue)
        .await
        .unwrap();
    SqliteActionRepo::new(pools.clone())
        .save(&seed.action)
        .await
        .unwrap();
    for (key, value) in &seed.settings {
        sqlx::query(
            "INSERT INTO settings (key, value) VALUES (?, ?) \
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        )
        .bind(key)
        .bind(value)
        .execute(pools.writer())
        .await
        .unwrap();
    }
    let globals = SqliteGlobalsRepo::new(pools.clone());
    for (name, value) in &seed.globals {
        globals.set(name, value.clone(), true).await.unwrap();
    }
    pools.close().await;
    std::fs::read(path).unwrap()
}

async fn restored_into(data_dir: &Path, bundle: PathBuf) {
    let staged = stage_full_restore(StageRequest {
        bundle,
        data_dir: data_dir.to_path_buf(),
        passphrase: Some(Passphrase::new(PASSPHRASE.to_owned())),
        database_schema: Arc::new(SqliteDatabaseFileSchema),
    })
    .await
    .expect("an older backup stages");
    record_pending_restore(data_dir, &staged, UpdateActivity::Idle)
        .await
        .unwrap();
    let outcome = apply_pending_restore(data_dir).unwrap();
    assert!(
        matches!(outcome, RestoreOutcome::Applied { .. }),
        "{outcome:?}"
    );
}

async fn restored_steps(provider: &dyn DataProvider, seed: &BackupSeed) -> Vec<SubActionStep> {
    provider
        .action_repo()
        .get(seed.action.id)
        .await
        .unwrap()
        .expect("the restored action")
        .sub_actions
}

#[tokio::test]
async fn an_older_full_backup_restores_with_its_data_intact_and_retired_ids_renamed() {
    let seed = backup_seed();
    let dir = TempDir::new().unwrap();
    let database = older_database(&dir.path().join("older.db"), &seed).await;
    let bundle = dir.path().join("older.forgebundle");
    std::fs::write(&bundle, older_full_backup(&database)).unwrap();
    let data_dir = dir.path().join("data");
    std::fs::create_dir_all(&data_dir).unwrap();

    restored_into(&data_dir, bundle).await;
    let provider = open_current(&data_dir).await;

    let stored: BTreeMap<String, String> = load_setting_rows(provider.as_ref())
        .await
        .unwrap()
        .into_iter()
        .map(|row| (row.key().to_owned(), row.value().to_owned()))
        .collect();
    let settings: BTreeMap<String, String> = seed
        .settings
        .keys()
        .map(|key| (key.clone(), stored.get(key).cloned().unwrap_or_default()))
        .collect();
    let mut expected_settings = seed.settings.clone();
    expected_settings.insert(
        FAVORITE_STEPS_KEY.to_owned(),
        FAVORITE_STEPS_AFTER_RENAME.to_owned(),
    );
    let mut globals = BTreeMap::new();
    for name in seed.globals.keys() {
        globals.insert(
            name.clone(),
            GlobalsRepo::get(provider.as_ref(), name)
                .await
                .unwrap()
                .unwrap(),
        );
    }
    assert_eq!(
        (
            restored_steps(provider.as_ref(), &seed).await,
            provider.queue_repo().get(seed.queue.id).await.unwrap(),
            settings,
            globals,
        ),
        (
            seed.action.sub_actions.clone(),
            Some(seed.queue.clone()),
            expected_settings,
            seed.globals.clone(),
        )
    );
}
