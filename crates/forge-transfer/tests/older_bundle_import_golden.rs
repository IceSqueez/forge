#![allow(clippy::unwrap_used, clippy::expect_used)]
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use age::secrecy::SecretString;
use forge_overlay::OverlayKindRegistry;
use forge_paths::{MEDIA_DIR_NAME, OVERLAYS_DIR_NAME};
use forge_registry::{SubActionRegistry, TriggerRegistry};
use forge_storage::{
    ActionRepo, DATABASE_FILE_NAME, DataProvider, GlobalsRepo, QueueRepo, load_setting_rows,
};
use forge_storage_sqlite::{
    MIGRATIONS, SqliteActionRepo, SqliteBackend, SqliteDatabaseFileSchema, SqliteGlobalsRepo,
    SqliteQueueRepo, connect,
};
use forge_transfer::{
    BUNDLE_FORMAT_VERSION, BackupCategory, BundleKind, BundleOrigin, ClosureRegistries,
    ContentEntry, ENCRYPTED_PAYLOAD_ENTRY_NAME, EncryptionScheme, FullManifest,
    MANIFEST_ENTRY_NAME, PackImportRequest, Passphrase, PublicManifest, RestoreOutcome,
    StageRequest, UpdateActivity, apply_pack_import, apply_pending_restore, load_pack_inventory,
    plan_pack_import, record_pending_restore, stage_full_restore, stage_pack,
};
use forge_types::{Action, Queue, SubActionStep, TriggerInstance, Variant};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tempfile::TempDir;
use time::OffsetDateTime;

const SQLITE_KEY: [u8; 32] = [0x42; 32];
const OLDER_SCHEMA_VERSION: i64 = 54;
const OLDER_FORGE_VERSION: &str = "0.6.3";
const PASSPHRASE: &str = "correct horse battery staple";
const CHEAP_WORK_FACTOR: u8 = 2;
const PACK_MANIFEST: &[u8] = include_bytes!("fixtures/older_pack/manifest.json");
const PACK_DOCUMENTS: &[u8] = include_bytes!("fixtures/older_pack/documents.json");
const PACK_DOCUMENTS_ENTRY: &str = "pack/documents.json";
const PACK_IMPORTED_GOLDEN: &str = include_str!("fixtures/older_pack/imported.json");
const BACKUP_SEED: &str = include_str!("fixtures/older_backup/seed.json");
const BACKUP_DATABASE_ENTRY: &str = "forge/forge.db";
const FAVORITE_STEPS_KEY: &str = "picker.favorites.sub_actions";
const FAVORITE_STEPS_AFTER_RENAME: &str = r#"["core.logic.wait","obs.scene.switch_current"]"#;

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

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
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    gz.write_all(&tar).unwrap();
    gz.finish().unwrap()
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

fn without_volatile_global_fields(globals: Vec<forge_storage::GlobalEntry>) -> Vec<Value> {
    let mut rows: Vec<Value> = globals
        .into_iter()
        .map(|g| json!({ "name": g.name, "value": g.value, "persisted": g.persisted }))
        .collect();
    rows.sort_by_key(|row| row["name"].as_str().unwrap_or_default().to_owned());
    rows
}

fn with_keys_sorted(value: Value) -> Value {
    match value {
        Value::Object(fields) => {
            let sorted: BTreeMap<String, Value> = fields
                .into_iter()
                .map(|(key, value)| (key, with_keys_sorted(value)))
                .collect();
            Value::Object(sorted.into_iter().collect())
        }
        Value::Array(items) => Value::Array(items.into_iter().map(with_keys_sorted).collect()),
        other => other,
    }
}

async fn imported_state(provider: &dyn DataProvider) -> String {
    let mut actions: Vec<Action> = provider.action_repo().list().await.unwrap();
    actions.sort_by(|a, b| a.name.cmp(&b.name));
    let mut queues: Vec<Queue> = provider.queue_repo().list().await.unwrap();
    queues.sort_by(|a, b| a.name.cmp(&b.name));
    let mut triggers: Vec<TriggerInstance> =
        provider.trigger_instance_repo().list_all().await.unwrap();
    triggers.sort_by(|a, b| a.name.cmp(&b.name));
    let mut links = BTreeMap::new();
    for action in &actions {
        let linked: Vec<String> = provider
            .trigger_instance_repo()
            .list_for_action(action.id)
            .await
            .unwrap()
            .into_iter()
            .map(|trigger| trigger.name)
            .collect();
        links.insert(action.name.clone(), linked);
    }
    let globals = without_volatile_global_fields(GlobalsRepo::list(provider).await.unwrap());
    let state = json!({
        "actions": actions,
        "queues": queues,
        "triggers": triggers,
        "trigger_links": links,
        "globals": globals,
    });
    format!(
        "{}\n",
        serde_json::to_string_pretty(&with_keys_sorted(state)).unwrap()
    )
}

#[tokio::test]
async fn a_pack_written_by_an_older_build_imports_into_the_golden_state() {
    let dir = tempfile::tempdir().unwrap();
    let data_dir = dir.path().join("data");
    std::fs::create_dir_all(&data_dir).unwrap();
    let provider = open_current(&data_dir).await;
    let bundle = dir.path().join("older.forgebundle");
    std::fs::write(
        &bundle,
        tar_gz(&[
            (MANIFEST_ENTRY_NAME, PACK_MANIFEST),
            (PACK_DOCUMENTS_ENTRY, PACK_DOCUMENTS),
        ]),
    )
    .unwrap();
    let sub_actions = SubActionRegistry::new();
    let triggers = TriggerRegistry::new();
    let overlays = OverlayKindRegistry::new();
    let registries = ClosureRegistries {
        sub_actions: &sub_actions,
        triggers: &triggers,
        overlays: &overlays,
        schema: None,
    };

    let staged = stage_pack(bundle, data_dir.clone()).await.unwrap();
    let inventory = load_pack_inventory(provider.as_ref()).await.unwrap();
    let plan = plan_pack_import(&staged, &inventory, registries).unwrap();
    apply_pack_import(PackImportRequest {
        provider: Arc::clone(&provider),
        registries,
        staged: &staged,
        plan,
        data_dir: data_dir.clone(),
        overlay_root: data_dir.join(OVERLAYS_DIR_NAME),
    })
    .await
    .unwrap();

    assert_eq!(
        imported_state(provider.as_ref()).await,
        PACK_IMPORTED_GOLDEN
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
    let pool = connect(&database_url(path)).await.unwrap();
    MIGRATIONS
        .run_to(OLDER_SCHEMA_VERSION, &pool)
        .await
        .expect("the older schema applies");
    SqliteQueueRepo::new(pool.clone())
        .save(&seed.queue)
        .await
        .unwrap();
    SqliteActionRepo::new(pool.clone())
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
        .execute(&pool)
        .await
        .unwrap();
    }
    let globals = SqliteGlobalsRepo::new(pool.clone());
    for (name, value) in &seed.globals {
        globals.set(name, value.clone(), true).await.unwrap();
    }
    pool.close().await;
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
