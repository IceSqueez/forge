#![allow(clippy::unwrap_used)]
mod common;
use common::{empty_documents, entry, file_named_anywhere, sha256_hex};

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use forge_overlay::{OverlayKindRegistry, register_builtin_kinds};
use forge_paths::OVERLAYS_DIR_NAME;
use forge_registry::{SubActionRegistry, TriggerRegistry};
use forge_speech_types::{AssignmentStrategy, IgnoreProfile};
use forge_storage::settings::core_settings;
use forge_storage::{
    DATABASE_FILE_NAME, DataProvider, EXPECTED_SCHEMA_VERSION, MediaBlobId, MediaFormat,
    OVERLAY_ALERT_QUEUE_NAME, QueueRole, RowClasses, SettingDisclosure, SettingEncoding,
    SettingPortability, SettingSkip, SettingSpec, SettingsCatalog, StoreChange,
    TtsPipelineSettings,
};
use forge_storage_sqlite::{SqliteBackend, SqliteDatabaseFileSchema};
use forge_transfer::{
    ActionDocument, BUNDLE_FORMAT_VERSION, BundleKind, ClipDocument, ClipSourceDocument,
    ClosureRegistries, ContentEntry, MANIFEST_ENTRY_NAME, MediaDocument, OverlayDocument,
    PACK_DOCUMENTS_VERSION, PackDocuments, QueueDocument, SETUP_DOCUMENTS_VERSION, SetupDocuments,
    SetupExportRequest, SetupImportOutcome, SetupImportRequest, SetupManifest, SkippedSetting,
    StageRequest, TransferError, TriggerDocument, TtsDocument, VoicesDocument, apply_setup_import,
    backups_dir, setup_staging_root, stage_full_restore, stage_pack, stage_setup, write_setup,
};
use forge_types::{
    Action, ActionId, ClipId, DEFAULT_QUEUE_ID, ExecutionMode, PermissionRung, Queue, QueueId,
    TriggerConfig, TriggerInstance, TriggerInstanceId, Variant,
};
use sqlx::Connection;
use tempfile::TempDir;
use time::OffsetDateTime;

const DOCUMENTS_ENTRY: &str = "setup/documents.json";
const MEDIA_FILE: &str = "setup/media/0";
const OVERLAY_FILE: &str = "setup/overlays/0/payload.css";
const ESCAPE_NAME: &str = "escape.css";
const UNCLASSIFIED_KEY: &str = "ui.last_note";
const PORTABLE_THEME: &str = "setup-theme";
const PORTABLE_VERBATIM: RowClasses = RowClasses {
    portability: SettingPortability::Portable,
    disclosure: SettingDisclosure::Verbatim,
};
const PORTABLE_PRESENCE: RowClasses = RowClasses {
    portability: SettingPortability::Portable,
    disclosure: SettingDisclosure::Presence,
};
const MACHINE_LOCAL_WITHHELD: RowClasses = RowClasses {
    portability: SettingPortability::MachineLocal,
    disclosure: SettingDisclosure::Withheld,
};
const ENGINE_PARAMS_PIPER: &str = "tts.engine_params.piper";
const OWNER_PORTABLE: SettingSpec = SettingSpec::exact(
    "owner.portable_flag",
    SettingEncoding::Flag,
    SettingPortability::Portable,
    SettingDisclosure::Verbatim,
);
const OWNER_MACHINE_LOCAL: SettingSpec = SettingSpec::exact(
    "owner.machine_local_path",
    SettingEncoding::Text,
    SettingPortability::MachineLocal,
    SettingDisclosure::Presence,
);

type Expected = fn(&TransferError) -> bool;
type Settings = BTreeMap<String, String>;

struct Store {
    dir: TempDir,
    provider: Arc<dyn DataProvider>,
    sqlite: Arc<SqliteBackend>,
    settings_catalog: SettingsCatalog,
}

fn owner_registered_catalog() -> SettingsCatalog {
    let mut catalog = SettingsCatalog::core();
    catalog
        .register(&[OWNER_PORTABLE, OWNER_MACHINE_LOCAL])
        .unwrap();
    catalog
}

impl Store {
    async fn open() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let backend = common::open_backend(dir.path()).await;
        let sqlite = Arc::new(backend);
        Self {
            dir,
            provider: Arc::clone(&sqlite) as Arc<dyn DataProvider>,
            sqlite,
            settings_catalog: owner_registered_catalog(),
        }
    }

    fn data_dir(&self) -> &Path {
        self.dir.path()
    }

    async fn set(&self, pairs: &[(&str, &str)]) {
        for (k, v) in pairs {
            self.seed(k, v, None).await;
        }
    }

    async fn seed(&self, key: &str, value: &str, classes: Option<RowClasses>) {
        self.sqlite
            .seed_foreign_setting(key, value, classes)
            .await
            .unwrap();
    }

    async fn stored_rows(&self) -> BTreeMap<String, (String, RowClasses)> {
        self.sqlite
            .stored_setting_rows()
            .await
            .unwrap()
            .into_iter()
            .map(|row| (row.key, (row.value, row.classes)))
            .collect()
    }

    async fn settings(&self) -> Settings {
        self.stored_rows()
            .await
            .into_iter()
            .map(|(key, (value, _))| (key, value))
            .collect()
    }

    async fn side_connection(&self) -> sqlx::SqliteConnection {
        sqlx::SqliteConnection::connect(&format!(
            "sqlite://{}",
            self.data_dir().join(DATABASE_FILE_NAME).display()
        ))
        .await
        .unwrap()
    }

    async fn refuse(&self, statement: &str, table: &str) {
        let mut connection = self.side_connection().await;
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "CREATE TRIGGER refuse_{table} BEFORE {statement} ON {table} \
             BEGIN SELECT RAISE(ABORT, 'refused'); END"
        )))
        .execute(&mut connection)
        .await
        .unwrap();
    }

    async fn import(&self, bundle: PathBuf) -> Result<SetupImportOutcome, TransferError> {
        let sub_actions = SubActionRegistry::new();
        let triggers = TriggerRegistry::new();
        let mut overlays = OverlayKindRegistry::new();
        register_builtin_kinds(&mut overlays).unwrap();
        let staged = stage_setup(bundle, self.data_dir().to_path_buf()).await?;
        apply_setup_import(SetupImportRequest {
            provider: Arc::clone(&self.provider),
            settings_catalog: &self.settings_catalog,
            hotkey_settings: &[],
            registries: ClosureRegistries {
                sub_actions: &sub_actions,
                triggers: &triggers,
                overlays: &overlays,
                schema: None,
            },
            staged: &staged,
            data_dir: self.data_dir().to_path_buf(),
            overlay_root: self.data_dir().join(OVERLAYS_DIR_NAME),
        })
        .await
    }
}

fn empty_setup() -> SetupDocuments {
    SetupDocuments {
        version: SETUP_DOCUMENTS_VERSION,
        configuration: empty_documents(),
        global_values: BTreeMap::new(),
        clip_hotkeys: Vec::new(),
        tts: TtsDocument {
            pipeline: TtsPipelineSettings::default(),
            filter_rules: Vec::new(),
        },
        voices: VoicesDocument {
            aliases: Vec::new(),
            strategy: AssignmentStrategy::default(),
            ignore_profile: IgnoreProfile::default(),
        },
        settings: BTreeMap::new(),
    }
}

fn setup_carrying(settings: &[(&str, &str)]) -> SetupDocuments {
    SetupDocuments {
        settings: settings
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect(),
        ..empty_setup()
    }
}

fn manifest(contents: Vec<ContentEntry>) -> Vec<u8> {
    serde_json::to_vec(&SetupManifest {
        format_version: BUNDLE_FORMAT_VERSION,
        kind: BundleKind::Setup,
        forge_version: "0.0.0".to_owned(),
        schema_version: EXPECTED_SCHEMA_VERSION,
        created_at: OffsetDateTime::UNIX_EPOCH,
        contents,
    })
    .unwrap()
}

fn bundle(dir: &Path, documents: &SetupDocuments, files: &[(&str, &[u8])]) -> PathBuf {
    let documents_json = serde_json::to_vec(documents).unwrap();
    let mut contents = vec![entry(DOCUMENTS_ENTRY, &documents_json)];
    contents.extend(files.iter().map(|(path, bytes)| entry(path, bytes)));
    let manifest = manifest(contents);
    let mut entries = vec![
        (MANIFEST_ENTRY_NAME, manifest.as_slice()),
        (DOCUMENTS_ENTRY, documents_json.as_slice()),
    ];
    entries.extend_from_slice(files);
    common::write_bundle_file(dir, &entries)
}

fn overlay_carrying(name: &str) -> OverlayDocument {
    common::overlay_carrying("setup-overlay", OVERLAY_FILE, name)
}

const PENDING_IMPORT_MARKER: &str = ".forge-import-pending";

fn staging_leftovers(data_dir: &Path) -> usize {
    std::fs::read_dir(setup_staging_root(data_dir))
        .map(|entries| entries.count())
        .unwrap_or(0)
}

#[tokio::test]
async fn a_setup_overlay_file_name_that_leaves_the_overlay_folder_is_refused_and_nothing_lands() {
    let outside = tempfile::tempdir().unwrap();
    let absolute = outside.path().join(ESCAPE_NAME).display().to_string();
    for name in [
        format!("../{ESCAPE_NAME}"),
        format!("nested/../../{ESCAPE_NAME}"),
        format!("..\\{ESCAPE_NAME}"),
        absolute,
    ] {
        let store = Store::open().await;
        let documents = SetupDocuments {
            configuration: PackDocuments {
                overlays: vec![overlay_carrying(&name)],
                ..empty_documents()
            },
            ..empty_setup()
        };
        let path = bundle(store.data_dir(), &documents, &[(OVERLAY_FILE, b"x")]);

        let refused = store.import(path).await;

        assert!(
            matches!(refused, Err(TransferError::UnsafeEntry { .. })),
            "{name:?}: {refused:?}"
        );
        assert!(
            store
                .provider
                .overlay_repo()
                .list()
                .await
                .unwrap()
                .is_empty(),
            "{name:?}: overlay row survived"
        );
        assert!(
            !file_named_anywhere(store.data_dir(), ESCAPE_NAME)
                && !file_named_anywhere(outside.path(), ESCAPE_NAME),
            "{name:?}: file escaped"
        );
    }
}

#[tokio::test]
async fn a_setup_archive_entry_that_climbs_out_of_the_setup_is_refused_at_staging() {
    let store = Store::open().await;
    let climbing = format!("setup/overlays/0/../../../{ESCAPE_NAME}");
    let path = bundle(store.data_dir(), &empty_setup(), &[(&climbing, b"x")]);

    let refused = stage_setup(path, store.data_dir().to_path_buf()).await;

    assert!(
        matches!(refused, Err(TransferError::UnsafeEntry { .. })),
        "{refused:?}"
    );
    assert!(!file_named_anywhere(store.data_dir(), ESCAPE_NAME));
    assert_eq!(staging_leftovers(store.data_dir()), 0);
}

#[tokio::test]
async fn staging_refuses_setup_documents_that_do_not_hold_together() {
    let dir = tempfile::tempdir().unwrap();
    let carried = MediaBlobId::from_digest(&[0xcd; 32]);
    let cases: [(&str, SetupDocuments, Expected); 4] = [
        (
            "newer setup documents format",
            SetupDocuments {
                version: SETUP_DOCUMENTS_VERSION + 1,
                ..empty_setup()
            },
            |e| {
                matches!(
                    e,
                    TransferError::NewerSetupDocuments { found, supported }
                        if *found == SETUP_DOCUMENTS_VERSION + 1 && *supported == SETUP_DOCUMENTS_VERSION
                )
            },
        ),
        (
            "newer nested configuration format",
            SetupDocuments {
                configuration: PackDocuments {
                    version: PACK_DOCUMENTS_VERSION + 1,
                    ..empty_documents()
                },
                ..empty_setup()
            },
            |e| {
                matches!(
                    e,
                    TransferError::NewerSetupDocuments { found, supported }
                        if *found == PACK_DOCUMENTS_VERSION + 1 && *supported == PACK_DOCUMENTS_VERSION
                )
            },
        ),
        (
            "media file not in the archive",
            SetupDocuments {
                configuration: PackDocuments {
                    media: vec![MediaDocument {
                        id: carried.as_str().to_owned(),
                        label: "ding.ogg".to_owned(),
                        format: MediaFormat::Ogg,
                        file: MEDIA_FILE.to_owned(),
                    }],
                    ..empty_documents()
                },
                ..empty_setup()
            },
            |e| matches!(e, TransferError::ContentsMismatch { path } if path == MEDIA_FILE),
        ),
        (
            "clip on media the setup does not carry",
            SetupDocuments {
                configuration: PackDocuments {
                    clips: vec![ClipDocument {
                        id: ClipId::new(),
                        name: "Dangling".to_owned(),
                        volume: 1.0,
                        category: String::new(),
                        loop_playback: false,
                        duration_secs: None,
                        builtin_id: None,
                        source: ClipSourceDocument::Media {
                            media_id: carried.as_str().to_owned(),
                        },
                    }],
                    ..empty_documents()
                },
                ..empty_setup()
            },
            |e| matches!(e, TransferError::Damaged { .. }),
        ),
    ];

    for (label, documents, expected) in cases {
        let path = bundle(dir.path(), &documents, &[]);
        let refused = stage_setup(path, dir.path().to_path_buf()).await;
        assert!(
            refused.as_ref().is_err_and(expected),
            "{label}: {refused:?}"
        );
        assert_eq!(staging_leftovers(dir.path()), 0, "{label}");
    }
}

#[tokio::test]
async fn a_setup_bundle_is_refused_by_restore_and_by_pack_import_as_the_wrong_kind() {
    let dir = tempfile::tempdir().unwrap();
    let path = bundle(dir.path(), &empty_setup(), &[]);

    let restore = stage_full_restore(StageRequest {
        bundle: path.clone(),
        data_dir: dir.path().to_path_buf(),
        passphrase: None,
        database_schema: Arc::new(SqliteDatabaseFileSchema),
    })
    .await;
    let pack = stage_pack(path, dir.path().to_path_buf()).await;

    assert!(
        matches!(
            restore,
            Err(TransferError::KindMismatch {
                expected: BundleKind::Full,
                found: BundleKind::Setup
            })
        ),
        "{restore:?}"
    );
    assert!(
        matches!(
            pack,
            Err(TransferError::KindMismatch {
                expected: BundleKind::Pack,
                found: BundleKind::Setup
            })
        ),
        "{pack:?}"
    );
}

#[tokio::test]
async fn a_crafted_setup_writes_only_the_settings_the_catalog_declares_portable() {
    let store = Store::open().await;
    let local = [
        (
            core_settings::AUDIO_OUTPUT_DEVICE_ID.name(),
            "local-speakers",
        ),
        (OWNER_MACHINE_LOCAL.key.name(), "/home/local/owner"),
        (
            core_settings::BACKUP_SCHEDULE_KEY.name(),
            "local-schedule-key",
        ),
    ];
    store.set(&local).await;
    let path = bundle(
        store.data_dir(),
        &setup_carrying(&[
            (
                core_settings::AUDIO_OUTPUT_DEVICE_ID.name(),
                "foreign-speakers",
            ),
            (
                core_settings::AUDIO_VOICE_GATE_INPUT_DEVICE_ID.name(),
                "foreign-mic",
            ),
            (OWNER_MACHINE_LOCAL.key.name(), "/etc"),
            (
                core_settings::BACKUP_SCHEDULE_KEY.name(),
                "foreign-schedule-key",
            ),
            (
                core_settings::VOICE_ASSIGNMENT_STRATEGY.name(),
                "\"Random\"",
            ),
            (UNCLASSIFIED_KEY, "foreign-note"),
            (core_settings::THEME.name(), PORTABLE_THEME),
            (OWNER_PORTABLE.key.name(), "true"),
        ]),
        &[],
    );

    store.import(path).await.unwrap();

    let mut expected: Settings = local
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect();
    expected.insert(
        core_settings::THEME.name().to_owned(),
        PORTABLE_THEME.to_owned(),
    );
    expected.insert(OWNER_PORTABLE.key.name().to_owned(), "true".to_owned());
    let mut settings = store.settings().await;
    settings.remove(core_settings::VOICE_ASSIGNMENT_STRATEGY.name());
    assert_eq!(settings, expected);
}

#[tokio::test]
async fn a_setup_import_trims_older_pre_setup_copies_to_the_kept_count() {
    let store = Store::open().await;
    store
        .set(&[
            (core_settings::SAFETY_COPIES_MODE.name(), "keep_newest"),
            (core_settings::SAFETY_COPIES_KEEP_NEWEST.name(), "1"),
        ])
        .await;
    let backups = forge_transfer::backups_dir(store.data_dir());
    std::fs::create_dir_all(&backups).unwrap();
    let older = backups.join("forge-pre-setup-20200101-000000Z-old001.tar.gz");
    std::fs::write(&older, b"older safety copy").unwrap();
    let path = bundle(store.data_dir(), &empty_setup(), &[]);

    store.import(path).await.unwrap();

    let pre_setup: Vec<String> = std::fs::read_dir(&backups)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with("forge-pre-setup-"))
        .collect();
    assert_eq!(
        (pre_setup.len(), older.exists()),
        (1, false),
        "{pre_setup:?}"
    );
}

#[tokio::test]
async fn a_setup_carrying_no_queues_removes_the_local_ones_but_keeps_the_default_queue() {
    let store = Store::open().await;
    let queues = store.provider.queue_repo();
    let local = forge_types::Queue {
        id: forge_types::QueueId::new(),
        name: "Alerts".to_owned(),
        description: String::new(),
        concurrency: std::num::NonZeroU32::MIN,
    };
    queues.save(&local).await.unwrap();
    let path = bundle(store.data_dir(), &empty_setup(), &[]);

    store.import(path).await.unwrap();

    let left: Vec<_> = queues
        .list()
        .await
        .unwrap()
        .into_iter()
        .map(|queue| queue.id)
        .collect();
    assert_eq!(left, vec![forge_types::DEFAULT_QUEUE_ID]);
}

#[tokio::test]
async fn a_setup_carrying_an_overlay_with_invalid_settings_writes_nothing_and_takes_no_snapshot() {
    let store = Store::open().await;
    store
        .set(&[(core_settings::THEME.name(), "local-theme")])
        .await;
    let before = store.settings().await;
    let mut documents = setup_carrying(&[(core_settings::THEME.name(), PORTABLE_THEME)]);
    documents.configuration.overlays = vec![OverlayDocument {
        config: [("margin_top".to_owned(), Variant::String("wide".to_owned()))].into(),
        files: Vec::new(),
        source_overrides: Vec::new(),
        ..overlay_carrying("unused.css")
    }];
    let path = bundle(store.data_dir(), &documents, &[]);

    let refused = store.import(path).await;

    assert!(
        matches!(refused, Err(TransferError::InvalidOverlayConfig { .. })),
        "{refused:?}"
    );
    let snapshots = std::fs::read_dir(backups_dir(store.data_dir()))
        .map(|entries| entries.count())
        .unwrap_or(0);
    let overlays = store.provider.overlay_repo().list().await.unwrap();
    assert_eq!(
        (store.settings().await, overlays.len(), snapshots),
        (before, 0, 0)
    );
}

fn mixed_admission_documents() -> SetupDocuments {
    setup_carrying(&[
        (
            core_settings::AUDIO_OUTPUT_DEVICE_ID.name(),
            "foreign-speakers",
        ),
        (core_settings::THEME.name(), PORTABLE_THEME),
        (UNCLASSIFIED_KEY, "foreign-note"),
        (OWNER_PORTABLE.key.name(), "maybe"),
        ("tts.engine_params.has space", "{}"),
        ("tts.engine_params.", "{}"),
        (ENGINE_PARAMS_PIPER, "{\"speed\":1.2}"),
        ("zz.undeclared", "tail"),
    ])
}

#[tokio::test]
async fn an_import_reports_every_refused_setting_with_the_reason_admission_gave() {
    let store = Store::open().await;
    let path = bundle(store.data_dir(), &mixed_admission_documents(), &[]);

    let outcome = store.import(path).await.unwrap();

    let skipped: HashMap<String, SettingSkip> = outcome
        .skipped_settings
        .into_iter()
        .map(|SkippedSetting { key, reason }| (key, reason))
        .collect();
    let expected: HashMap<String, SettingSkip> = [
        ("tts.engine_params.", SettingSkip::Undeclared),
        ("tts.engine_params.has space", SettingSkip::Undeclared),
        (UNCLASSIFIED_KEY, SettingSkip::Undeclared),
        (
            core_settings::AUDIO_OUTPUT_DEVICE_ID.name(),
            SettingSkip::NotPortable,
        ),
        (OWNER_PORTABLE.key.name(), SettingSkip::Undecodable),
        ("zz.undeclared", SettingSkip::Undeclared),
    ]
    .into_iter()
    .map(|(k, reason)| (k.to_owned(), reason))
    .collect();
    assert_eq!(skipped, expected);
}

#[tokio::test]
async fn an_import_keeps_writing_the_admitted_settings_around_the_refused_ones() {
    let store = Store::open().await;
    let path = bundle(store.data_dir(), &mixed_admission_documents(), &[]);

    store.import(path).await.unwrap();

    let expected: Settings = [
        (
            core_settings::THEME.name().to_owned(),
            PORTABLE_THEME.to_owned(),
        ),
        (ENGINE_PARAMS_PIPER.to_owned(), "{\"speed\":1.2}".to_owned()),
    ]
    .into();
    let mut settings = store.settings().await;
    settings.remove(core_settings::VOICE_ASSIGNMENT_STRATEGY.name());
    assert_eq!(settings, expected);
}

#[tokio::test]
async fn a_bundle_value_the_declared_encoding_cannot_decode_does_not_delete_the_local_row() {
    let store = Store::open().await;
    store
        .seed(OWNER_PORTABLE.key.name(), "true", Some(PORTABLE_VERBATIM))
        .await;
    let path = bundle(
        store.data_dir(),
        &setup_carrying(&[(OWNER_PORTABLE.key.name(), "maybe")]),
        &[],
    );

    store.import(path).await.unwrap();

    assert_eq!(
        store.stored_rows().await.get(OWNER_PORTABLE.key.name()),
        Some(&("true".to_owned(), PORTABLE_VERBATIM))
    );
}

#[tokio::test]
async fn a_stored_portable_row_the_bundle_does_not_carry_is_removed_and_a_machine_local_one_stays()
{
    let store = Store::open().await;
    store
        .seed(
            core_settings::ACCENT.name(),
            "local-accent",
            Some(PORTABLE_VERBATIM),
        )
        .await;
    store
        .seed(
            core_settings::AUDIO_OUTPUT_DEVICE_ID.name(),
            "local-speakers",
            Some(MACHINE_LOCAL_WITHHELD),
        )
        .await;
    let path = bundle(store.data_dir(), &empty_setup(), &[]);

    store.import(path).await.unwrap();

    let mut left: Vec<String> = store.stored_rows().await.into_keys().collect();
    left.retain(|name| name != core_settings::VOICE_ASSIGNMENT_STRATEGY.name());
    assert_eq!(
        left,
        vec![core_settings::AUDIO_OUTPUT_DEVICE_ID.name().to_owned()]
    );
}

#[tokio::test]
async fn a_classless_legacy_row_for_a_portable_declared_key_is_not_removed_by_a_replace_import() {
    let store = Store::open().await;
    store
        .seed(core_settings::ACCENT.name(), "legacy-accent", None)
        .await;
    let path = bundle(store.data_dir(), &empty_setup(), &[]);

    store.import(path).await.unwrap();

    assert_eq!(
        store.stored_rows().await.get(core_settings::ACCENT.name()),
        Some(&("legacy-accent".to_owned(), MACHINE_LOCAL_WITHHELD))
    );
}

#[tokio::test]
async fn an_imported_setting_is_stored_with_its_declared_classes_over_a_legacy_row() {
    let store = Store::open().await;
    store
        .seed(core_settings::THEME.name(), "legacy-theme", None)
        .await;
    let path = bundle(
        store.data_dir(),
        &setup_carrying(&[
            (core_settings::THEME.name(), PORTABLE_THEME),
            (OWNER_PORTABLE.key.name(), "true"),
        ]),
        &[],
    );

    store.import(path).await.unwrap();

    let rows = store.stored_rows().await;
    assert_eq!(
        (
            rows.get(core_settings::THEME.name()).cloned(),
            rows.get(OWNER_PORTABLE.key.name()).cloned()
        ),
        (
            Some((PORTABLE_THEME.to_owned(), PORTABLE_VERBATIM)),
            Some(("true".to_owned(), PORTABLE_VERBATIM)),
        )
    );
}

#[tokio::test]
async fn an_exported_setup_carries_only_rows_stored_as_portable() {
    let store = Store::open().await;
    store
        .seed(
            core_settings::THEME.name(),
            "stored-portable",
            Some(PORTABLE_VERBATIM),
        )
        .await;
    store
        .seed(
            UNCLASSIFIED_KEY,
            "stored-portable-note",
            Some(PORTABLE_PRESENCE),
        )
        .await;
    store
        .seed(core_settings::ACCENT.name(), "legacy-classless", None)
        .await;
    store
        .seed(
            core_settings::AUDIO_OUTPUT_DEVICE_ID.name(),
            "local-speakers",
            Some(MACHINE_LOCAL_WITHHELD),
        )
        .await;
    let written = write_setup(SetupExportRequest {
        provider: Arc::clone(&store.provider),
        overlay_root: store.data_dir().join(OVERLAYS_DIR_NAME),
        destination: store.data_dir().join("export.forgebundle"),
    })
    .await
    .unwrap();

    let staged = stage_setup(written, store.data_dir().to_path_buf())
        .await
        .unwrap();

    let seeded = [
        core_settings::THEME.name(),
        UNCLASSIFIED_KEY,
        core_settings::ACCENT.name(),
        core_settings::AUDIO_OUTPUT_DEVICE_ID.name(),
    ];
    let carried: Settings = staged
        .documents()
        .settings
        .iter()
        .filter(|(name, _)| seeded.contains(&name.as_str()))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    assert_eq!(
        carried,
        [
            (
                core_settings::THEME.name().to_owned(),
                "stored-portable".to_owned()
            ),
            (
                UNCLASSIFIED_KEY.to_owned(),
                "stored-portable-note".to_owned()
            ),
        ]
        .into()
    );
}

#[tokio::test]
async fn a_failed_setup_import_restores_replaced_rows_and_removes_added_ones_with_their_exact_classes()
 {
    let store = Store::open().await;
    store
        .seed(
            core_settings::THEME.name(),
            "local-theme",
            Some(PORTABLE_PRESENCE),
        )
        .await;
    store
        .seed(
            core_settings::ACCENT.name(),
            "local-accent",
            Some(PORTABLE_VERBATIM),
        )
        .await;
    let blocked = forge_types::Queue {
        id: forge_types::QueueId::new(),
        name: "Blocked".to_owned(),
        description: String::new(),
        concurrency: std::num::NonZeroU32::MIN,
    };
    store.provider.queue_repo().save(&blocked).await.unwrap();
    let mut connection = sqlx::SqliteConnection::connect(&format!(
        "sqlite://{}",
        store.data_dir().join(DATABASE_FILE_NAME).display()
    ))
    .await
    .unwrap();
    sqlx::query(
        "CREATE TRIGGER refuse_queue_delete BEFORE DELETE ON queues \
         BEGIN SELECT RAISE(ABORT, 'queue delete refused'); END",
    )
    .execute(&mut connection)
    .await
    .unwrap();
    let before = store.stored_rows().await;
    let path = bundle(
        store.data_dir(),
        &setup_carrying(&[
            (core_settings::THEME.name(), PORTABLE_THEME),
            (OWNER_PORTABLE.key.name(), "true"),
        ]),
        &[],
    );

    let failed = store.import(path).await;

    let mut after = store.stored_rows().await;
    after.remove(core_settings::VOICE_ASSIGNMENT_STRATEGY.name());
    assert!(failed.is_err(), "{failed:?}");
    assert_eq!(after, before);
}

#[tokio::test]
async fn a_failed_setup_import_restores_an_overlay_it_had_already_removed() {
    let store = Store::open().await;
    let removed_first = store
        .provider
        .overlay_repo()
        .create("Alpha overlay", "overlay.chat", 1)
        .await
        .unwrap();
    let refused = store
        .provider
        .overlay_repo()
        .create("Beta overlay", "overlay.chat", 1)
        .await
        .unwrap();
    assert!(removed_first.id.as_str() < refused.id.as_str());
    let mut connection = store.side_connection().await;
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "CREATE TRIGGER refuse_overlay_delete BEFORE DELETE ON overlays \
         WHEN OLD.id = '{}' BEGIN SELECT RAISE(ABORT, 'overlay delete refused'); END",
        refused.id.as_str()
    )))
    .execute(&mut connection)
    .await
    .unwrap();
    let path = bundle(store.data_dir(), &empty_setup(), &[]);

    let failed = store.import(path).await;

    let mut remaining: Vec<String> = store
        .provider
        .overlay_repo()
        .list()
        .await
        .unwrap()
        .into_iter()
        .map(|overlay| overlay.id.as_str().to_owned())
        .collect();
    remaining.sort();
    assert!(failed.is_err(), "{failed:?}");
    assert_eq!(
        remaining,
        vec![
            removed_first.id.as_str().to_owned(),
            refused.id.as_str().to_owned()
        ]
    );
}

const SETUP_OGG: &[u8] = b"OggS\x00\x02\x00\x00setup sound";
const OVERLAY_BYTES: &[u8] = b"body { color: red; }";
const PAYLOAD_NAME: &str = "payload.css";

fn setup_of(configuration: PackDocuments) -> SetupDocuments {
    SetupDocuments {
        configuration,
        ..empty_setup()
    }
}

fn default_queue_document() -> QueueDocument {
    QueueDocument {
        id: DEFAULT_QUEUE_ID,
        name: "Default".to_owned(),
        description: String::new(),
        concurrency: 1,
        role: None,
    }
}

fn trigger_document(id: TriggerInstanceId) -> TriggerDocument {
    TriggerDocument {
        id,
        kind_id: "twitch.follow".to_owned(),
        name: "follow".to_owned(),
        overrides: Default::default(),
        enabled: true,
        user_defined: true,
        platform_scope: Default::default(),
        cooldown_secs: 0,
        cooldown_global: true,
        permission_rung: PermissionRung::Everyone,
    }
}

fn action_document(
    id: ActionId,
    queue_id: QueueId,
    trigger_ids: Vec<TriggerInstanceId>,
) -> ActionDocument {
    ActionDocument {
        id,
        name: "alert".to_owned(),
        group: None,
        queue_id,
        enabled: true,
        concurrent: false,
        bypass_pause: false,
        execution_mode: ExecutionMode::Sequential,
        description: None,
        steps: Vec::new(),
        trigger_ids,
    }
}

fn overlay_named(id: &str, display_name: &str) -> OverlayDocument {
    OverlayDocument {
        id: id.to_owned(),
        display_name: display_name.to_owned(),
        files: Vec::new(),
        source_overrides: Vec::new(),
        ..overlay_carrying(PAYLOAD_NAME)
    }
}

fn media_document() -> MediaDocument {
    MediaDocument {
        id: format!("sha256-{}", sha256_hex(SETUP_OGG)),
        label: "ding.ogg".to_owned(),
        format: MediaFormat::Ogg,
        file: MEDIA_FILE.to_owned(),
    }
}

#[tokio::test]
async fn a_setup_import_reaches_the_catalog_as_a_single_revision() {
    let store = Store::open().await;
    let trigger = TriggerInstanceId::new();
    let documents = setup_of(PackDocuments {
        queues: vec![default_queue_document()],
        triggers: vec![trigger_document(trigger)],
        actions: vec![action_document(
            ActionId::new(),
            DEFAULT_QUEUE_ID,
            vec![trigger],
        )],
        ..empty_documents()
    });
    let path = bundle(store.data_dir(), &documents, &[]);
    let revision = store.provider.catalog_revision();
    let before = revision.current();

    store.import(path).await.unwrap();

    assert_eq!(revision.current(), before + 1);
}

#[tokio::test]
async fn a_setup_import_whose_commit_fails_removes_the_files_and_media_it_placed() {
    let store = Store::open().await;
    store.refuse("INSERT", "overlays").await;
    let documents = setup_of(PackDocuments {
        overlays: vec![overlay_carrying(PAYLOAD_NAME)],
        media: vec![media_document()],
        ..empty_documents()
    });
    let path = bundle(
        store.data_dir(),
        &documents,
        &[(MEDIA_FILE, SETUP_OGG), (OVERLAY_FILE, OVERLAY_BYTES)],
    );

    let failed = store.import(path).await;

    assert!(
        matches!(&failed, Err(e) if !matches!(e, TransferError::SetupRollbackIncomplete { .. })),
        "{failed:?}"
    );
    assert!(!file_named_anywhere(
        &store.data_dir().join(OVERLAYS_DIR_NAME),
        PAYLOAD_NAME
    ));
    assert_eq!(store.provider.media_repo().list().await.unwrap(), []);
}

#[tokio::test]
async fn a_setup_import_whose_commit_fails_puts_back_the_overlay_file_it_replaced() {
    let store = Store::open().await;
    let local = store
        .provider
        .overlay_repo()
        .create("Escaper", "overlay.alert", 1)
        .await
        .unwrap();
    let directory = store
        .data_dir()
        .join(OVERLAYS_DIR_NAME)
        .join(local.id.as_str());
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(directory.join(PAYLOAD_NAME), b"local bytes").unwrap();
    store.refuse("UPDATE", "overlays").await;
    let documents = setup_of(PackDocuments {
        overlays: vec![OverlayDocument {
            id: local.id.as_str().to_owned(),
            ..overlay_carrying(PAYLOAD_NAME)
        }],
        ..empty_documents()
    });
    let path = bundle(
        store.data_dir(),
        &documents,
        &[(OVERLAY_FILE, OVERLAY_BYTES)],
    );

    let failed = store.import(path).await;

    assert!(failed.is_err(), "{failed:?}");
    assert_eq!(
        std::fs::read(directory.join(PAYLOAD_NAME)).unwrap(),
        b"local bytes"
    );
}

#[tokio::test]
async fn a_finished_setup_import_keeps_its_overlay_folder_and_clears_the_pending_marker() {
    let store = Store::open().await;
    let documents = setup_of(PackDocuments {
        overlays: vec![overlay_carrying(PAYLOAD_NAME)],
        ..empty_documents()
    });
    let path = bundle(
        store.data_dir(),
        &documents,
        &[(OVERLAY_FILE, OVERLAY_BYTES)],
    );

    store.import(path).await.unwrap();

    let overlays = store.data_dir().join(OVERLAYS_DIR_NAME);
    assert!(file_named_anywhere(&overlays, PAYLOAD_NAME));
    assert!(!file_named_anywhere(&overlays, PENDING_IMPORT_MARKER));
}

#[tokio::test]
async fn a_rolled_back_setup_import_leaves_neither_its_overlay_folder_nor_a_pending_marker() {
    let store = Store::open().await;
    store.refuse("INSERT", "overlays").await;
    let documents = setup_of(PackDocuments {
        overlays: vec![overlay_carrying(PAYLOAD_NAME)],
        ..empty_documents()
    });
    let path = bundle(
        store.data_dir(),
        &documents,
        &[(OVERLAY_FILE, OVERLAY_BYTES)],
    );

    let failed = store.import(path).await;

    assert!(failed.is_err(), "{failed:?}");
    let leftovers = std::fs::read_dir(store.data_dir().join(OVERLAYS_DIR_NAME))
        .map(Iterator::count)
        .unwrap_or(0);
    assert_eq!(leftovers, 0);
}

#[tokio::test]
async fn importing_over_an_existing_overlay_folder_never_marks_it_as_pending() {
    let store = Store::open().await;
    let local = store
        .provider
        .overlay_repo()
        .create("Escaper", "overlay.alert", 1)
        .await
        .unwrap();
    let directory = store
        .data_dir()
        .join(OVERLAYS_DIR_NAME)
        .join(local.id.as_str());
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(directory.join("local.css"), b"local bytes").unwrap();
    store.refuse("UPDATE", "overlays").await;
    let documents = setup_of(PackDocuments {
        overlays: vec![OverlayDocument {
            id: local.id.as_str().to_owned(),
            ..overlay_carrying(PAYLOAD_NAME)
        }],
        ..empty_documents()
    });
    let path = bundle(
        store.data_dir(),
        &documents,
        &[(OVERLAY_FILE, OVERLAY_BYTES)],
    );

    let failed = store.import(path).await;

    assert!(failed.is_err(), "{failed:?}");
    assert!(!directory.join(PENDING_IMPORT_MARKER).exists());
    assert!(directory.join("local.css").exists());
}

#[tokio::test]
async fn two_new_setup_overlays_with_one_name_land_under_distinct_ids() {
    let store = Store::open().await;
    let documents = setup_of(PackDocuments {
        overlays: vec![
            overlay_named("first", "Twin"),
            overlay_named("second", "Twin"),
        ],
        ..empty_documents()
    });
    let path = bundle(store.data_dir(), &documents, &[]);

    store.import(path).await.unwrap();

    let mut ids: Vec<String> = store
        .provider
        .overlay_repo()
        .list()
        .await
        .unwrap()
        .iter()
        .map(|overlay| overlay.id.as_str().to_owned())
        .collect();
    ids.sort();
    assert_eq!(ids, ["twin", "twin-2"]);
}

#[tokio::test]
async fn a_setup_import_clears_the_links_a_local_action_holds_to_an_archived_trigger() {
    let store = Store::open().await;
    let action = Action {
        id: ActionId::new(),
        name: "alert".to_owned(),
        group: None,
        queue_id: DEFAULT_QUEUE_ID,
        enabled: true,
        concurrent: false,
        bypass_pause: false,
        execution_mode: ExecutionMode::Sequential,
        description: None,
        sub_actions: Vec::new(),
    };
    let trigger = TriggerInstance {
        id: TriggerInstanceId::new(),
        kind_id: "twitch.follow".to_owned(),
        name: "follow".to_owned(),
        overrides: TriggerConfig::new(),
        enabled: true,
        user_defined: true,
        platform_scope: Default::default(),
        cooldown_secs: 0,
        cooldown_global: true,
        permission_rung: PermissionRung::Everyone,
    };
    let triggers = store.provider.trigger_instance_repo();
    store.provider.action_repo().save(&action).await.unwrap();
    triggers.save(&trigger).await.unwrap();
    triggers.link_action(action.id, trigger.id).await.unwrap();
    assert!(triggers.archive(trigger.id).await.unwrap());
    let documents = setup_of(PackDocuments {
        queues: vec![default_queue_document()],
        triggers: vec![trigger_document(trigger.id)],
        actions: vec![action_document(action.id, DEFAULT_QUEUE_ID, Vec::new())],
        ..empty_documents()
    });
    let path = bundle(store.data_dir(), &documents, &[]);

    store.import(path).await.unwrap();

    assert_eq!(triggers.list_for_action(action.id).await.unwrap(), []);
    assert_eq!(triggers.actions_using(trigger.id).await.unwrap(), []);
}

const RENAMED_ALERT_QUEUE: &str = "Stream alerts";

fn named_queue(name: &str) -> Queue {
    Queue {
        id: QueueId::new(),
        name: name.to_owned(),
        description: String::new(),
        concurrency: std::num::NonZeroU32::MIN,
    }
}

fn queue_document(queue: &Queue, role: Option<QueueRole>) -> QueueDocument {
    QueueDocument {
        id: queue.id,
        name: queue.name.clone(),
        description: queue.description.clone(),
        concurrency: queue.concurrency.get(),
        role,
    }
}

fn setup_of_queues(queues: Vec<QueueDocument>) -> SetupDocuments {
    let mut documents = empty_setup();
    documents.configuration.queues = queues;
    documents
}

impl Store {
    async fn hold_alert_role(&self, queue: &Queue) {
        self.provider
            .commit_changes(&[
                StoreChange::SaveQueue(queue.clone()),
                StoreChange::AssignOverlayAlertQueue(queue.id),
            ])
            .await
            .unwrap();
    }

    async fn alert_queue(&self) -> Option<Queue> {
        self.provider
            .queue_repo()
            .overlay_alert_queue(OVERLAY_ALERT_QUEUE_NAME)
            .await
            .unwrap()
    }

    async fn export(&self) -> PathBuf {
        write_setup(SetupExportRequest {
            provider: Arc::clone(&self.provider),
            overlay_root: self.data_dir().join(OVERLAYS_DIR_NAME),
            destination: self.data_dir().join("export.forgebundle"),
        })
        .await
        .unwrap()
    }
}

#[tokio::test]
async fn an_exported_setup_marks_only_the_queue_holding_the_alert_role() {
    let store = Store::open().await;
    let queue = named_queue(RENAMED_ALERT_QUEUE);
    store.hold_alert_role(&queue).await;

    let staged = stage_setup(store.export().await, store.data_dir().to_path_buf())
        .await
        .unwrap();

    let marked: Vec<QueueId> = staged
        .documents()
        .configuration
        .queues
        .iter()
        .filter(|document| document.role == Some(QueueRole::OverlayAlerts))
        .map(|document| document.id)
        .collect();
    assert_eq!(marked, [queue.id]);
}

#[tokio::test]
async fn a_renamed_alert_queue_keeps_the_role_through_a_setup_export_and_import() {
    let source = Store::open().await;
    let queue = named_queue(RENAMED_ALERT_QUEUE);
    source.hold_alert_role(&queue).await;
    let exported = source.export().await;
    let target = Store::open().await;

    target.import(exported).await.unwrap();

    assert_eq!(target.alert_queue().await, Some(queue));
}

#[tokio::test]
async fn a_setup_moves_the_alert_role_off_a_local_queue_it_also_carries() {
    let store = Store::open().await;
    let holder = named_queue("Old alerts");
    store.hold_alert_role(&holder).await;
    let incoming = named_queue(RENAMED_ALERT_QUEUE);
    let documents = setup_of_queues(vec![
        queue_document(&holder, None),
        queue_document(&incoming, Some(QueueRole::OverlayAlerts)),
    ]);

    store
        .import(bundle(store.data_dir(), &documents, &[]))
        .await
        .unwrap();

    assert_eq!(store.alert_queue().await, Some(incoming));
}

#[tokio::test]
async fn a_setup_written_before_queue_roles_leaves_the_local_holder_in_place() {
    let store = Store::open().await;
    let holder = named_queue(RENAMED_ALERT_QUEUE);
    store.hold_alert_role(&holder).await;
    let document: QueueDocument = serde_json::from_value(serde_json::json!({
        "id": holder.id,
        "name": holder.name,
        "description": holder.description,
        "concurrency": holder.concurrency,
    }))
    .unwrap();

    store
        .import(bundle(
            store.data_dir(),
            &setup_of_queues(vec![document]),
            &[],
        ))
        .await
        .unwrap();

    assert_eq!(store.alert_queue().await, Some(holder));
}
