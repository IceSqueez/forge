#![allow(clippy::unwrap_used)]
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use forge_registry::{SubActionRegistry, TriggerRegistry};
use forge_storage::paths::{MEDIA_DIR_NAME, OVERLAYS_DIR_NAME};
use forge_storage::settings::reserved_keys as key;
use forge_storage::{
    AssignmentStrategy, DATABASE_FILE_NAME, DataProvider, EXPECTED_SCHEMA_VERSION, IgnoreProfile,
    MediaBlobId, MediaFormat, SettingsRepo, TtsPipelineSettings,
};
use forge_storage_sqlite::{SqliteBackend, SqliteDatabaseFileSchema};
use forge_transfer::{
    BUNDLE_FORMAT_VERSION, BundleKind, ClipDocument, ClipSourceDocument, ClosureRegistries,
    ContentEntry, MANIFEST_ENTRY_NAME, MediaDocument, OverlayDocument, OverlayFileDocument,
    PACK_DOCUMENTS_VERSION, PackDocuments, SETUP_DOCUMENTS_VERSION, SetupDocuments,
    SetupImportOutcome, SetupImportRequest, SetupManifest, StageRequest, TransferError,
    TtsDocument, VoicesDocument, apply_setup_import, setup_staging_root, stage_full_restore,
    stage_pack, stage_setup,
};
use forge_types::ClipId;
use sha2::{Digest, Sha256};
use tempfile::TempDir;
use time::OffsetDateTime;

const SQLITE_KEY: [u8; 32] = [0x42; 32];
const DOCUMENTS_ENTRY: &str = "setup/documents.json";
const MEDIA_FILE: &str = "setup/media/0";
const OVERLAY_FILE: &str = "setup/overlays/0/payload.css";
const ESCAPE_NAME: &str = "escape.css";
const UNCLASSIFIED_KEY: &str = "ui.last_note";
const PORTABLE_THEME: &str = "setup-theme";

type Expected = fn(&TransferError) -> bool;
type Settings = BTreeMap<String, String>;

struct Store {
    dir: TempDir,
    provider: Arc<dyn DataProvider>,
}

impl Store {
    async fn open() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let url = format!("sqlite://{}", dir.path().join(DATABASE_FILE_NAME).display());
        let backend =
            SqliteBackend::open_for_test(&url, SQLITE_KEY, dir.path().join(MEDIA_DIR_NAME), None)
                .await
                .unwrap();
        Self {
            dir,
            provider: Arc::new(backend),
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
        let staged = stage_setup(bundle, self.data_dir().to_path_buf()).await?;
        apply_setup_import(SetupImportRequest {
            provider: Arc::clone(&self.provider),
            registries: ClosureRegistries {
                sub_actions: &sub_actions,
                triggers: &triggers,
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
async fn a_crafted_setup_carrying_machine_local_keys_writes_none_of_them() {
    let store = Store::open().await;
    let local = [
        (key::WINDOW_PLACEMENT, "local-window"),
        (key::AUDIO_OUTPUT_DEVICE_ID, "local-speakers"),
        (key::SERVER_OVERLAY_ROOT, "/home/local/overlays"),
        (key::STAY_AWAKE, "false"),
        (key::BACKUP_SCHEDULE_KEY, "local-schedule-key"),
    ];
    store.set(&local).await;
    let path = bundle(
        store.data_dir(),
        &setup_carrying(&[
            (key::WINDOW_PLACEMENT, "foreign-window"),
            (key::AUDIO_OUTPUT_DEVICE_ID, "foreign-speakers"),
            (key::AUDIO_VOICE_GATE_INPUT_DEVICE_ID, "foreign-mic"),
            (key::SERVER_OVERLAY_ROOT, "/etc"),
            (key::UPDATES_ROLLED_BACK_VERSION, "9.9.9"),
            (key::STAY_AWAKE, "true"),
            (key::BACKUP_SCHEDULE_KEY, "foreign-schedule-key"),
            (key::VOICE_ASSIGNMENT_STRATEGY, "\"Random\""),
            (UNCLASSIFIED_KEY, "foreign-note"),
            (key::THEME, PORTABLE_THEME),
        ]),
        &[],
    );

    store.import(path).await.unwrap();

    let mut expected: Settings = local
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect();
    expected.insert(key::THEME.to_owned(), PORTABLE_THEME.to_owned());
    let mut settings = store.settings().await;
    settings.remove(key::VOICE_ASSIGNMENT_STRATEGY);
    assert_eq!(settings, expected);
}

#[tokio::test]
async fn a_crafted_setup_cannot_loosen_network_exposure_or_http_trust() {
    let store = Store::open().await;
    let local = [
        (key::SERVER_BIND_ADDRESS, "127.0.0.1"),
        (key::SERVER_LAN_BIND_ENABLED, "false"),
        (key::SERVER_AUTH_REQUIRED_FOR_READS, "true"),
        (key::SERVER_HTTP_OVERLAY_REQUIRE_TOKEN, "true"),
        (key::SERVER_OVERLAY_CORS_ANY_ORIGIN, "false"),
        (key::SERVER_ADDITIONAL_ORIGINS, "[]"),
        (key::SCRIPT_HTTP_ALLOWED_DOMAINS, "[\"api.example.com\"]"),
        (key::SCRIPT_HTTP_ALLOW_LOCAL, "false"),
        (key::CORE_HTTP_ALLOW_LOCAL, "false"),
    ];
    store.set(&local).await;
    let path = bundle(
        store.data_dir(),
        &setup_carrying(&[
            (key::SERVER_BIND_ADDRESS, "0.0.0.0"),
            (key::SERVER_LAN_BIND_ENABLED, "true"),
            (key::SERVER_AUTH_REQUIRED_FOR_READS, "false"),
            (key::SERVER_HTTP_OVERLAY_REQUIRE_TOKEN, "false"),
            (key::SERVER_OVERLAY_CORS_ANY_ORIGIN, "true"),
            (
                key::SERVER_ADDITIONAL_ORIGINS,
                "[\"https://attacker.example\"]",
            ),
            (key::SCRIPT_HTTP_ALLOWED_DOMAINS, "[\"*\"]"),
            (key::SCRIPT_HTTP_ALLOW_LOCAL, "true"),
            (key::CORE_HTTP_ALLOW_LOCAL, "true"),
        ]),
        &[],
    );

    store.import(path).await.unwrap();

    let settings = store.settings().await;
    let loosened: Vec<_> = local
        .iter()
        .filter(|(k, v)| settings.get(*k).map(String::as_str) != Some(*v))
        .map(|(k, _)| (*k, settings.get(*k).cloned()))
        .collect();
    assert_eq!(loosened, Vec::new());
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
