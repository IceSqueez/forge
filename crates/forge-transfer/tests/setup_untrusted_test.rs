#![allow(clippy::unwrap_used)]
use std::collections::{BTreeMap, HashMap};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use forge_overlay::{OverlayKindRegistry, register_builtin_kinds};
use forge_paths::{MEDIA_DIR_NAME, OVERLAYS_DIR_NAME};
use forge_registry::{SubActionRegistry, TriggerRegistry};
use forge_speech_types::{AssignmentStrategy, IgnoreProfile};
use forge_storage::settings::reserved_keys as key;
use forge_storage::{
    DATABASE_FILE_NAME, DataProvider, EXPECTED_SCHEMA_VERSION, MediaBlobId, MediaFormat,
    RowClasses, SettingDisclosure, SettingEncoding, SettingPortability, SettingSkip, SettingSpec,
    SettingsCatalog, SettingsRepo, TtsPipelineSettings,
};
use forge_storage_sqlite::{SqliteBackend, SqliteDatabaseFileSchema};
use forge_transfer::{
    BUNDLE_FORMAT_VERSION, BundleKind, ClipDocument, ClipSourceDocument, ClosureRegistries,
    ContentEntry, MANIFEST_ENTRY_NAME, MediaDocument, OverlayDocument, OverlayFileDocument,
    PACK_DOCUMENTS_VERSION, PackDocuments, SETUP_DOCUMENTS_VERSION, SetupDocuments,
    SetupExportRequest, SetupImportOutcome, SetupImportRequest, SetupManifest, SkippedSetting,
    StageRequest, TransferError, TtsDocument, VoicesDocument, apply_setup_import, backups_dir,
    setup_staging_root, stage_full_restore, stage_pack, stage_setup, write_setup,
};
use forge_types::{ClipId, Variant};
use sha2::{Digest, Sha256};
use sqlx::Connection;
use tempfile::TempDir;
use time::OffsetDateTime;

const SQLITE_KEY: [u8; 32] = [0x42; 32];
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
        let url = format!("sqlite://{}", dir.path().join(DATABASE_FILE_NAME).display());
        let backend =
            SqliteBackend::open_for_test(&url, SQLITE_KEY, dir.path().join(MEDIA_DIR_NAME), None)
                .await
                .unwrap();
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
            SettingsRepo::set_string(self.provider.as_ref(), k, v)
                .await
                .unwrap();
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
        SettingsRepo::load_all(self.provider.as_ref())
            .await
            .unwrap()
            .into_iter()
            .collect()
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

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn entry(path: &str, bytes: &[u8]) -> ContentEntry {
    ContentEntry {
        path: path.to_owned(),
        size: bytes.len() as u64,
        sha256_hex: sha256_hex(bytes),
    }
}

fn empty_configuration() -> PackDocuments {
    PackDocuments {
        version: PACK_DOCUMENTS_VERSION,
        actions: Vec::new(),
        triggers: Vec::new(),
        queues: Vec::new(),
        scripts: Vec::new(),
        globals: Vec::new(),
        clips: Vec::new(),
        overlays: Vec::new(),
        media: Vec::new(),
    }
}

fn empty_setup() -> SetupDocuments {
    SetupDocuments {
        version: SETUP_DOCUMENTS_VERSION,
        configuration: empty_configuration(),
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

fn header(name: &str, size: usize) -> tar::Header {
    let mut header = tar::Header::new_gnu();
    header.as_gnu_mut().unwrap().name[..name.len()].copy_from_slice(name.as_bytes());
    header.set_entry_type(tar::EntryType::Regular);
    header.set_size(size as u64);
    header.set_mode(0o600);
    header.set_cksum();
    header
}

fn bundle(dir: &Path, documents: &SetupDocuments, files: &[(&str, &[u8])]) -> PathBuf {
    let documents_json = serde_json::to_vec(documents).unwrap();
    let mut contents = vec![entry(DOCUMENTS_ENTRY, &documents_json)];
    contents.extend(files.iter().map(|(path, bytes)| entry(path, bytes)));
    let manifest = manifest(contents);
    let mut builder = tar::Builder::new(Vec::new());
    for (name, bytes) in [
        (MANIFEST_ENTRY_NAME, manifest.as_slice()),
        (DOCUMENTS_ENTRY, documents_json.as_slice()),
    ]
    .into_iter()
    .chain(files.iter().copied())
    {
        builder.append(&header(name, bytes.len()), bytes).unwrap();
    }
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    gz.write_all(&builder.into_inner().unwrap()).unwrap();
    let path = dir.join(format!("{}.forgebundle", ulid::Ulid::generate()));
    std::fs::write(&path, gz.finish().unwrap()).unwrap();
    path
}

fn overlay_carrying(name: &str) -> OverlayDocument {
    OverlayDocument {
        id: "setup-overlay".to_owned(),
        display_name: "Escaper".to_owned(),
        kind_id: "overlay.alert".to_owned(),
        enabled: true,
        position: 0,
        config: Default::default(),
        config_schema_version: 1,
        source_overrides: vec![name.to_owned()],
        files: vec![OverlayFileDocument {
            name: name.to_owned(),
            file: OVERLAY_FILE.to_owned(),
        }],
    }
}

fn file_named_anywhere(root: &Path, name: &str) -> bool {
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir).into_iter().flatten() {
            let path = entry.unwrap().path();
            if path.file_name().is_some_and(|n| n == name) {
                return true;
            }
            if path.is_dir() {
                pending.push(path);
            }
        }
    }
    false
}

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
                ..empty_configuration()
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
                    ..empty_configuration()
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
                    ..empty_configuration()
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
                    ..empty_configuration()
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
        (key::AUDIO_OUTPUT_DEVICE_ID, "local-speakers"),
        (OWNER_MACHINE_LOCAL.key.name(), "/home/local/owner"),
        (key::BACKUP_SCHEDULE_KEY, "local-schedule-key"),
    ];
    store.set(&local).await;
    let path = bundle(
        store.data_dir(),
        &setup_carrying(&[
            (key::AUDIO_OUTPUT_DEVICE_ID, "foreign-speakers"),
            (key::AUDIO_VOICE_GATE_INPUT_DEVICE_ID, "foreign-mic"),
            (OWNER_MACHINE_LOCAL.key.name(), "/etc"),
            (key::BACKUP_SCHEDULE_KEY, "foreign-schedule-key"),
            (key::VOICE_ASSIGNMENT_STRATEGY, "\"Random\""),
            (UNCLASSIFIED_KEY, "foreign-note"),
            (key::THEME, PORTABLE_THEME),
            (OWNER_PORTABLE.key.name(), "true"),
        ]),
        &[],
    );

    store.import(path).await.unwrap();

    let mut expected: Settings = local
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect();
    expected.insert(key::THEME.to_owned(), PORTABLE_THEME.to_owned());
    expected.insert(OWNER_PORTABLE.key.name().to_owned(), "true".to_owned());
    let mut settings = store.settings().await;
    settings.remove(key::VOICE_ASSIGNMENT_STRATEGY);
    assert_eq!(settings, expected);
}

#[tokio::test]
async fn a_setup_import_trims_older_pre_setup_copies_to_the_kept_count() {
    let store = Store::open().await;
    store
        .set(&[
            (key::SAFETY_COPIES_MODE, "keep_newest"),
            (key::SAFETY_COPIES_KEEP_NEWEST, "1"),
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
        concurrency: 1,
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
    store.set(&[(key::THEME, "local-theme")]).await;
    let before = store.settings().await;
    let mut documents = setup_carrying(&[(key::THEME, PORTABLE_THEME)]);
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
        (key::AUDIO_OUTPUT_DEVICE_ID, "foreign-speakers"),
        (key::THEME, PORTABLE_THEME),
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
        (key::AUDIO_OUTPUT_DEVICE_ID, SettingSkip::NotPortable),
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
        (key::THEME.to_owned(), PORTABLE_THEME.to_owned()),
        (ENGINE_PARAMS_PIPER.to_owned(), "{\"speed\":1.2}".to_owned()),
    ]
    .into();
    let mut settings = store.settings().await;
    settings.remove(key::VOICE_ASSIGNMENT_STRATEGY);
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
        .seed(key::ACCENT, "local-accent", Some(PORTABLE_VERBATIM))
        .await;
    store
        .seed(
            key::AUDIO_OUTPUT_DEVICE_ID,
            "local-speakers",
            Some(MACHINE_LOCAL_WITHHELD),
        )
        .await;
    let path = bundle(store.data_dir(), &empty_setup(), &[]);

    store.import(path).await.unwrap();

    let mut left: Vec<String> = store.stored_rows().await.into_keys().collect();
    left.retain(|name| name != key::VOICE_ASSIGNMENT_STRATEGY);
    assert_eq!(left, vec![key::AUDIO_OUTPUT_DEVICE_ID.to_owned()]);
}

#[tokio::test]
async fn a_classless_legacy_row_for_a_portable_declared_key_is_not_removed_by_a_replace_import() {
    let store = Store::open().await;
    store.seed(key::ACCENT, "legacy-accent", None).await;
    let path = bundle(store.data_dir(), &empty_setup(), &[]);

    store.import(path).await.unwrap();

    assert_eq!(
        store.stored_rows().await.get(key::ACCENT),
        Some(&("legacy-accent".to_owned(), MACHINE_LOCAL_WITHHELD))
    );
}

#[tokio::test]
async fn an_imported_setting_is_stored_with_its_declared_classes_over_a_legacy_row() {
    let store = Store::open().await;
    store.seed(key::THEME, "legacy-theme", None).await;
    let path = bundle(
        store.data_dir(),
        &setup_carrying(&[
            (key::THEME, PORTABLE_THEME),
            (OWNER_PORTABLE.key.name(), "true"),
        ]),
        &[],
    );

    store.import(path).await.unwrap();

    let rows = store.stored_rows().await;
    assert_eq!(
        (
            rows.get(key::THEME).cloned(),
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
        .seed(key::THEME, "stored-portable", Some(PORTABLE_VERBATIM))
        .await;
    store
        .seed(
            UNCLASSIFIED_KEY,
            "stored-portable-note",
            Some(PORTABLE_PRESENCE),
        )
        .await;
    store.seed(key::ACCENT, "legacy-classless", None).await;
    store
        .seed(
            key::AUDIO_OUTPUT_DEVICE_ID,
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
        key::THEME,
        UNCLASSIFIED_KEY,
        key::ACCENT,
        key::AUDIO_OUTPUT_DEVICE_ID,
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
            (key::THEME.to_owned(), "stored-portable".to_owned()),
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
        .seed(key::THEME, "local-theme", Some(PORTABLE_PRESENCE))
        .await;
    store
        .seed(key::ACCENT, "local-accent", Some(PORTABLE_VERBATIM))
        .await;
    let blocked = forge_types::Queue {
        id: forge_types::QueueId::new(),
        name: "Blocked".to_owned(),
        description: String::new(),
        concurrency: 1,
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
            (key::THEME, PORTABLE_THEME),
            (OWNER_PORTABLE.key.name(), "true"),
        ]),
        &[],
    );

    let failed = store.import(path).await;

    let mut after = store.stored_rows().await;
    after.remove(key::VOICE_ASSIGNMENT_STRATEGY);
    assert!(failed.is_err(), "{failed:?}");
    assert_eq!(after, before);
}
