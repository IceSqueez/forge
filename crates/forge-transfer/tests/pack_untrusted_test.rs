#![allow(clippy::unwrap_used)]
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use forge_overlay::{OverlayError, OverlayKindRegistry, register_builtin_kinds};
use forge_paths::{MEDIA_DIR_NAME, OVERLAYS_DIR_NAME};
use forge_registry::{SubActionRegistry, TriggerRegistry};
use forge_storage::{
    DATABASE_FILE_NAME, DataProvider, EXPECTED_SCHEMA_VERSION, MediaBlobId, MediaFormat,
    clip_source_referrer,
};
use forge_storage::{OVERLAY_ALERT_QUEUE_NAME, StoreChange};
use forge_storage::{OverlayDefinition, OverlayId};
use forge_storage_sqlite::SqliteBackend;
use forge_transfer::{
    ActionDocument, BUNDLE_FORMAT_VERSION, BundleKind, ClipDocument, ClipSourceDocument,
    ClosureRegistries, ContentEntry, MANIFEST_ENTRY_NAME, MediaDocument, OverlayDocument,
    OverlayFileDocument, PACK_DOCUMENTS_VERSION, PackDocuments, PackExportRequest,
    PackImportOutcome, PackImportPlan, PackImportRequest, PackItemKey, PackItemKind, PackManifest,
    PackMetadata, PackResolution, QueueDocument, StagedPack, TransferError, TriggerDocument,
    apply_pack_import, backups_dir, load_pack_inventory, pack_staging_root, plan_pack_import,
    resolve_pack_closure, stage_pack, write_pack,
};
use forge_types::{
    Action, ActionId, ClipId, ExecutionMode, PermissionRung, Queue, QueueId, TriggerInstanceId,
    Variant,
};
use sha2::{Digest, Sha256};
use sqlx::Connection;
use tempfile::TempDir;
use time::OffsetDateTime;

const SQLITE_KEY: [u8; 32] = [0x42; 32];
const DOCUMENTS_ENTRY: &str = "pack/documents.json";
const MEDIA_FILE: &str = "pack/media/0";
const OVERLAY_FILE: &str = "pack/overlays/0/payload.css";
const ESCAPE_NAME: &str = "escape.css";
const LOCAL_OGG: &[u8] = b"OggS\x00\x02\x00\x00local sound";
const PACK_OGG: &[u8] = b"OggS\x00\x02\x00\x00pack sound";
const CHAT_KIND: &str = "overlay.chat";
const UNSHIPPED_KIND: &str = "overlay.vendor_unshipped";
const POSITION_KEY: &str = "position";
const KNOWN_POSITION: &str = "top";
const UNKNOWN_POSITION: &str = "middle";
const OLDEST_SCHEMA: u32 = 1;
const PACKED_OVERLAY_NAME: &str = "Packed chat";

type Expected = fn(&TransferError) -> bool;

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

    async fn stage(&self, bundle: PathBuf) -> StagedPack {
        stage_pack(bundle, self.data_dir().to_path_buf())
            .await
            .unwrap()
    }

    async fn plan(&self, staged: &StagedPack) -> Result<PackImportPlan, TransferError> {
        let registries = Registries::builtin();
        let inventory = load_pack_inventory(self.provider.as_ref()).await?;
        plan_pack_import(staged, &inventory, registries.closure())
    }

    async fn apply(
        &self,
        staged: &StagedPack,
        plan: PackImportPlan,
    ) -> Result<PackImportOutcome, TransferError> {
        let registries = Registries::builtin();
        apply_pack_import(PackImportRequest {
            provider: Arc::clone(&self.provider),
            registries: registries.closure(),
            staged,
            plan,
            data_dir: self.data_dir().to_path_buf(),
            overlay_root: self.data_dir().join(OVERLAYS_DIR_NAME),
        })
        .await
    }

    async fn import(&self, bundle: PathBuf) -> Result<PackImportOutcome, TransferError> {
        let staged = stage_pack(bundle, self.data_dir().to_path_buf()).await?;
        let plan = self.plan(&staged).await?;
        self.apply(&staged, plan).await
    }

    async fn overlay(&self, id: &OverlayId) -> Option<OverlayDefinition> {
        self.provider.overlay_repo().get(id).await.unwrap()
    }

    async fn refuse_overlay_inserts(&self) {
        let mut connection = sqlx::SqliteConnection::connect(&format!(
            "sqlite://{}",
            self.data_dir().join(DATABASE_FILE_NAME).display()
        ))
        .await
        .unwrap();
        sqlx::query(
            "CREATE TRIGGER refuse_overlays BEFORE INSERT ON overlays \
             BEGIN SELECT RAISE(ABORT, 'refused'); END",
        )
        .execute(&mut connection)
        .await
        .unwrap();
    }

    fn safety_snapshots(&self) -> usize {
        std::fs::read_dir(backups_dir(self.data_dir()))
            .map(|entries| entries.count())
            .unwrap_or(0)
    }
}

struct Registries {
    sub_actions: SubActionRegistry,
    triggers: TriggerRegistry,
    overlays: OverlayKindRegistry,
}

impl Registries {
    fn builtin() -> Self {
        let mut overlays = OverlayKindRegistry::new();
        register_builtin_kinds(&mut overlays).unwrap();
        Self {
            sub_actions: SubActionRegistry::new(),
            triggers: TriggerRegistry::new(),
            overlays,
        }
    }

    fn closure(&self) -> ClosureRegistries<'_> {
        ClosureRegistries {
            sub_actions: &self.sub_actions,
            triggers: &self.triggers,
            overlays: &self.overlays,
            schema: None,
        }
    }
}

fn current_schema(kind_id: &str) -> u32 {
    Registries::builtin()
        .overlays
        .get(kind_id)
        .unwrap()
        .config_schema_version()
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

const PENDING_IMPORT_MARKER: &str = ".forge-import-pending";

fn empty_documents() -> PackDocuments {
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

fn manifest(contents: Vec<ContentEntry>) -> Vec<u8> {
    serde_json::to_vec(&PackManifest {
        format_version: BUNDLE_FORMAT_VERSION,
        kind: BundleKind::Pack,
        forge_version: "0.0.0".to_owned(),
        schema_version: EXPECTED_SCHEMA_VERSION,
        created_at: OffsetDateTime::UNIX_EPOCH,
        name: "Untrusted".to_owned(),
        author: String::new(),
        description: String::new(),
        required_integrations: Vec::new(),
        external_references: Vec::new(),
        unresolved_references: Vec::new(),
        unknown_kinds: Vec::new(),
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

fn bundle_at(dir: &Path, documents_json: &[u8], files: &[(&str, &[u8])]) -> PathBuf {
    let mut contents = vec![entry(DOCUMENTS_ENTRY, documents_json)];
    contents.extend(files.iter().map(|(path, bytes)| entry(path, bytes)));
    let manifest = manifest(contents);
    let mut builder = tar::Builder::new(Vec::new());
    for (name, bytes) in [
        (MANIFEST_ENTRY_NAME, manifest.as_slice()),
        (DOCUMENTS_ENTRY, documents_json),
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

fn bundle(dir: &Path, documents: &PackDocuments, files: &[(&str, &[u8])]) -> PathBuf {
    bundle_at(dir, &serde_json::to_vec(documents).unwrap(), files)
}

fn overlay_carrying(name: &str) -> OverlayDocument {
    OverlayDocument {
        id: "pack-overlay".to_owned(),
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

fn media_claiming(id: &str) -> MediaDocument {
    MediaDocument {
        id: id.to_owned(),
        label: "ding.ogg".to_owned(),
        format: MediaFormat::Ogg,
        file: MEDIA_FILE.to_owned(),
    }
}

fn clip_on_media(media_id: &str) -> ClipDocument {
    ClipDocument {
        id: ClipId::new(),
        name: "Packed clip".to_owned(),
        volume: 1.0,
        category: String::new(),
        loop_playback: false,
        duration_secs: None,
        builtin_id: None,
        source: ClipSourceDocument::Media {
            media_id: media_id.to_owned(),
        },
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

#[tokio::test]
async fn an_overlay_file_name_that_leaves_the_overlay_folder_is_refused_and_nothing_lands() {
    let outside = tempfile::tempdir().unwrap();
    let absolute = outside.path().join(ESCAPE_NAME).display().to_string();
    for name in [
        format!("../{ESCAPE_NAME}"),
        format!("../../{ESCAPE_NAME}"),
        format!("nested/../../{ESCAPE_NAME}"),
        format!("./{ESCAPE_NAME}"),
        format!("..\\{ESCAPE_NAME}"),
        format!("c:{ESCAPE_NAME}"),
        absolute,
        "..".to_owned(),
    ] {
        let store = Store::open().await;
        let documents = PackDocuments {
            overlays: vec![overlay_carrying(&name)],
            ..empty_documents()
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
async fn a_pack_archive_entry_that_climbs_out_of_the_pack_is_refused_at_staging() {
    let store = Store::open().await;
    let climbing = format!("pack/overlays/0/../../../{ESCAPE_NAME}");
    let path = bundle(store.data_dir(), &empty_documents(), &[(&climbing, b"x")]);

    let refused = stage_pack(path, store.data_dir().to_path_buf()).await;

    assert!(
        matches!(refused, Err(TransferError::UnsafeEntry { .. })),
        "{refused:?}"
    );
    assert!(!file_named_anywhere(store.data_dir(), ESCAPE_NAME));
    let leftovers = std::fs::read_dir(pack_staging_root(store.data_dir()))
        .map(|entries| entries.count())
        .unwrap_or(0);
    assert_eq!(leftovers, 0);
}

#[tokio::test]
async fn a_pack_claiming_a_local_media_id_for_other_bytes_gets_its_own_blob() {
    let store = Store::open().await;
    let media = store.provider.media_repo();
    let local = media
        .store("local.ogg", LOCAL_OGG.to_vec())
        .await
        .unwrap()
        .id;
    let clip = clip_on_media(local.as_str());
    let documents = PackDocuments {
        media: vec![media_claiming(local.as_str())],
        clips: vec![clip.clone()],
        ..empty_documents()
    };
    let path = bundle(store.data_dir(), &documents, &[(MEDIA_FILE, PACK_OGG)]);

    store.import(path).await.unwrap();
    let clip_blob = media
        .blob_of(&clip_source_referrer(clip.id))
        .await
        .unwrap()
        .unwrap();

    assert_ne!(clip_blob, local);
    assert_eq!(media.read(&clip_blob).await.unwrap(), PACK_OGG);
    assert_eq!(media.read(&local).await.unwrap(), LOCAL_OGG);
}

#[tokio::test]
async fn a_pack_media_id_that_does_not_match_its_bytes_is_never_stored_under_that_id() {
    let store = Store::open().await;
    let claimed = MediaBlobId::from_digest(&[0xab; 32]);
    let clip = clip_on_media(claimed.as_str());
    let documents = PackDocuments {
        media: vec![media_claiming(claimed.as_str())],
        clips: vec![clip.clone()],
        ..empty_documents()
    };
    let path = bundle(store.data_dir(), &documents, &[(MEDIA_FILE, PACK_OGG)]);

    store.import(path).await.unwrap();
    let media = store.provider.media_repo();
    let clip_blob = media.blob_of(&clip_source_referrer(clip.id)).await.unwrap();

    assert_eq!(media.get(&claimed).await.unwrap(), None);
    assert_eq!(
        clip_blob.map(|id| id.as_str() != claimed.as_str()),
        Some(true)
    );
}

#[tokio::test]
async fn staging_refuses_documents_that_do_not_hold_together() {
    let dir = tempfile::tempdir().unwrap();
    let carried = MediaBlobId::from_digest(&[0xcd; 32]);
    let cases: [(&str, PackDocuments, Expected); 4] = [
        (
            "newer documents format",
            PackDocuments {
                version: PACK_DOCUMENTS_VERSION + 1,
                ..empty_documents()
            },
            |e| {
                matches!(
                    e,
                    TransferError::NewerPackDocuments { found, supported }
                        if *found == PACK_DOCUMENTS_VERSION + 1 && *supported == PACK_DOCUMENTS_VERSION
                )
            },
        ),
        (
            "media file not in the archive",
            PackDocuments {
                media: vec![media_claiming(carried.as_str())],
                ..empty_documents()
            },
            |e| matches!(e, TransferError::ContentsMismatch { path } if path == MEDIA_FILE),
        ),
        (
            "bundled file aliases the documents entry",
            PackDocuments {
                overlays: vec![OverlayDocument {
                    files: vec![OverlayFileDocument {
                        name: "page.css".to_owned(),
                        file: DOCUMENTS_ENTRY.to_owned(),
                    }],
                    ..overlay_carrying("page.css")
                }],
                ..empty_documents()
            },
            |e| matches!(e, TransferError::ContentsMismatch { path } if path == DOCUMENTS_ENTRY),
        ),
        (
            "clip on media the pack does not carry",
            PackDocuments {
                clips: vec![clip_on_media(carried.as_str())],
                ..empty_documents()
            },
            |e| matches!(e, TransferError::Damaged { .. }),
        ),
    ];

    for (label, documents, expected) in cases {
        let path = bundle(dir.path(), &documents, &[]);
        let refused = stage_pack(path, dir.path().to_path_buf()).await;
        assert!(
            refused.as_ref().is_err_and(expected),
            "{label}: {refused:?}"
        );
    }
}

fn chat_overlay(id: &str, position: &str, schema: u32) -> OverlayDocument {
    OverlayDocument {
        id: id.to_owned(),
        display_name: PACKED_OVERLAY_NAME.to_owned(),
        kind_id: CHAT_KIND.to_owned(),
        enabled: true,
        position: 0,
        config: [(
            POSITION_KEY.to_owned(),
            Variant::String(position.to_owned()),
        )]
        .into(),
        config_schema_version: schema,
        source_overrides: Vec::new(),
        files: Vec::new(),
    }
}

fn carrying_overlay(overlay: OverlayDocument) -> PackDocuments {
    PackDocuments {
        overlays: vec![overlay],
        ..empty_documents()
    }
}

#[tokio::test]
async fn planning_judges_each_overlay_against_the_look_and_schema_the_pack_recorded() {
    type Verdict = fn(&Result<PackImportPlan, TransferError>) -> bool;
    let current = current_schema(CHAT_KIND);
    let cases: [(&str, OverlayDocument, Verdict); 4] = [
        (
            "valid settings",
            chat_overlay("valid", KNOWN_POSITION, current),
            |r| r.is_ok(),
        ),
        (
            "older schema the upgrade repairs",
            chat_overlay("older", UNKNOWN_POSITION, OLDEST_SCHEMA),
            |r| r.is_ok(),
        ),
        (
            "current schema with an unknown choice",
            chat_overlay("current", UNKNOWN_POSITION, current),
            |r| {
                matches!(
                    r,
                    Err(TransferError::InvalidOverlayConfig {
                        overlay,
                        source: OverlayError::UnknownChoice { key, value },
                    }) if overlay == PACKED_OVERLAY_NAME && key == POSITION_KEY && value == UNKNOWN_POSITION
                )
            },
        ),
        (
            "look this forge lacks",
            OverlayDocument {
                kind_id: UNSHIPPED_KIND.to_owned(),
                ..chat_overlay("unshipped", KNOWN_POSITION, current)
            },
            |r| {
                matches!(
                    r,
                    Err(TransferError::InvalidOverlayConfig {
                        source: OverlayError::UnknownKind(kind),
                        ..
                    }) if kind == UNSHIPPED_KIND
                )
            },
        ),
    ];

    for (label, overlay, verdict) in cases {
        let store = Store::open().await;
        let staged = store
            .stage(bundle(store.data_dir(), &carrying_overlay(overlay), &[]))
            .await;

        let planned = store.plan(&staged).await;

        assert!(verdict(&planned), "{label}: {planned:?}");
    }
}

#[tokio::test]
async fn applying_an_invalid_overlay_against_a_replacing_plan_writes_nothing_and_takes_no_snapshot()
{
    let store = Store::open().await;
    let current = current_schema(CHAT_KIND);
    let local = store
        .provider
        .overlay_repo()
        .create("Local chat", CHAT_KIND, current)
        .await
        .unwrap();
    let id = local.id.as_str();
    let valid = store
        .stage(bundle(
            store.data_dir(),
            &carrying_overlay(chat_overlay(id, KNOWN_POSITION, current)),
            &[],
        ))
        .await;
    let mut plan = store.plan(&valid).await.unwrap();
    plan.set_resolution(
        &PackItemKey {
            kind: PackItemKind::Overlay,
            id: id.to_owned(),
        },
        PackResolution::Replace,
    )
    .unwrap();
    let invalid = store
        .stage(bundle(
            store.data_dir(),
            &carrying_overlay(chat_overlay(id, UNKNOWN_POSITION, current)),
            &[],
        ))
        .await;

    let refused = store.apply(&invalid, plan).await;

    assert!(
        matches!(refused, Err(TransferError::InvalidOverlayConfig { .. })),
        "{refused:?}"
    );
    assert_eq!(
        (store.overlay(&local.id).await, store.safety_snapshots()),
        (Some(local), 0)
    );
}

const PAYLOAD_NAME: &str = "payload.css";

#[tokio::test]
async fn a_pack_import_reaches_the_catalog_as_a_single_revision() {
    let store = Store::open().await;
    let queue_id = QueueId::new();
    let trigger_id = TriggerInstanceId::new();
    let documents = PackDocuments {
        queues: vec![QueueDocument {
            id: queue_id,
            name: "Packed".to_owned(),
            description: String::new(),
            concurrency: 1,
            role: None,
        }],
        triggers: vec![TriggerDocument {
            id: trigger_id,
            kind_id: "twitch.follow".to_owned(),
            name: "follow".to_owned(),
            overrides: Default::default(),
            enabled: true,
            user_defined: true,
            platform_scope: Default::default(),
            cooldown_secs: 0,
            cooldown_global: true,
            permission_rung: PermissionRung::Everyone,
        }],
        actions: vec![ActionDocument {
            id: ActionId::new(),
            name: "alert".to_owned(),
            group: None,
            queue_id,
            enabled: true,
            concurrent: false,
            bypass_pause: false,
            execution_mode: ExecutionMode::Sequential,
            description: None,
            steps: Vec::new(),
            trigger_ids: vec![trigger_id],
        }],
        ..empty_documents()
    };
    let path = bundle(store.data_dir(), &documents, &[]);
    let revision = store.provider.catalog_revision();
    let before = revision.current();

    store.import(path).await.unwrap();

    assert_eq!(revision.current(), before + 1);
}

#[tokio::test]
async fn a_pack_import_whose_commit_fails_removes_the_files_and_media_it_placed() {
    let store = Store::open().await;
    store.refuse_overlay_inserts().await;
    let media_id = format!("sha256-{}", sha256_hex(PACK_OGG));
    let documents = PackDocuments {
        media: vec![media_claiming(&media_id)],
        clips: vec![clip_on_media(&media_id)],
        overlays: vec![overlay_carrying(PAYLOAD_NAME)],
        ..empty_documents()
    };
    let path = bundle(
        store.data_dir(),
        &documents,
        &[(MEDIA_FILE, PACK_OGG), (OVERLAY_FILE, b"body {}")],
    );

    let failed = store.import(path).await;

    assert!(
        matches!(&failed, Err(e) if !matches!(e, TransferError::PackRollbackIncomplete { .. })),
        "{failed:?}"
    );
    assert!(!file_named_anywhere(
        &store.data_dir().join(OVERLAYS_DIR_NAME),
        PAYLOAD_NAME
    ));
    assert_eq!(store.provider.media_repo().list().await.unwrap(), []);
    assert_eq!(
        store.provider.soundboard_clips_repo().list().await.unwrap(),
        []
    );
}

#[tokio::test]
async fn a_finished_pack_import_keeps_its_overlay_folder_and_clears_the_pending_marker() {
    let store = Store::open().await;
    let documents = PackDocuments {
        overlays: vec![overlay_carrying(PAYLOAD_NAME)],
        ..empty_documents()
    };
    let path = bundle(store.data_dir(), &documents, &[(OVERLAY_FILE, b"body {}")]);

    store.import(path).await.unwrap();

    let overlays = store.data_dir().join(OVERLAYS_DIR_NAME);
    assert!(file_named_anywhere(&overlays, PAYLOAD_NAME));
    assert!(!file_named_anywhere(&overlays, PENDING_IMPORT_MARKER));
}

#[tokio::test]
async fn a_rolled_back_pack_import_leaves_neither_its_overlay_folder_nor_a_pending_marker() {
    let store = Store::open().await;
    store.refuse_overlay_inserts().await;
    let documents = PackDocuments {
        overlays: vec![overlay_carrying(PAYLOAD_NAME)],
        ..empty_documents()
    };
    let path = bundle(store.data_dir(), &documents, &[(OVERLAY_FILE, b"body {}")]);

    let failed = store.import(path).await;

    assert!(failed.is_err(), "{failed:?}");
    let leftovers = std::fs::read_dir(store.data_dir().join(OVERLAYS_DIR_NAME))
        .map(Iterator::count)
        .unwrap_or(0);
    assert_eq!(leftovers, 0);
}

#[tokio::test]
async fn two_new_pack_overlays_with_one_name_land_under_distinct_ids() {
    let store = Store::open().await;
    let twin = |id: &str| OverlayDocument {
        id: id.to_owned(),
        display_name: "Twin".to_owned(),
        files: Vec::new(),
        source_overrides: Vec::new(),
        ..overlay_carrying(PAYLOAD_NAME)
    };
    let documents = PackDocuments {
        overlays: vec![twin("first"), twin("second")],
        ..empty_documents()
    };
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

const RENAMED_ALERT_QUEUE: &str = "Stream alerts";

fn named_queue(name: &str) -> Queue {
    Queue {
        id: QueueId::new(),
        name: name.to_owned(),
        description: String::new(),
        concurrency: std::num::NonZeroU32::MIN,
    }
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

    async fn pack_of_alert_queue(&self, queue: &Queue) -> PathBuf {
        let action = Action {
            id: ActionId::new(),
            name: "alert".to_owned(),
            group: None,
            queue_id: queue.id,
            enabled: true,
            concurrent: false,
            bypass_pause: false,
            execution_mode: ExecutionMode::Sequential,
            description: None,
            sub_actions: Vec::new(),
        };
        self.provider.action_repo().save(&action).await.unwrap();
        let registries = Registries::builtin();
        let inventory = load_pack_inventory(self.provider.as_ref()).await.unwrap();
        let selection =
            resolve_pack_closure(&inventory, registries.closure(), &[action.id]).unwrap();
        write_pack(PackExportRequest {
            provider: Arc::clone(&self.provider),
            inventory,
            selection,
            metadata: PackMetadata {
                name: "alerts".to_owned(),
                author: String::new(),
                description: String::new(),
            },
            overlay_root: self.data_dir().join(OVERLAYS_DIR_NAME),
            destination: self.data_dir().join("alerts.forgebundle"),
        })
        .await
        .unwrap()
    }
}

#[tokio::test]
async fn a_renamed_alert_queue_carried_by_a_pack_takes_the_free_role() {
    let source = Store::open().await;
    let queue = named_queue(RENAMED_ALERT_QUEUE);
    source.hold_alert_role(&queue).await;
    let pack = source.pack_of_alert_queue(&queue).await;
    let target = Store::open().await;

    target.import(pack).await.unwrap();

    let holder = target.alert_queue().await.map(|queue| queue.name);
    assert_eq!(holder.as_deref(), Some(RENAMED_ALERT_QUEUE));
}

#[tokio::test]
async fn a_pack_alert_queue_leaves_the_role_with_the_queue_holding_it_here() {
    let source = Store::open().await;
    let queue = named_queue(RENAMED_ALERT_QUEUE);
    source.hold_alert_role(&queue).await;
    let pack = source.pack_of_alert_queue(&queue).await;
    let target = Store::open().await;
    let local = named_queue("Local alerts");
    target.hold_alert_role(&local).await;

    target.import(pack).await.unwrap();

    assert_eq!(target.alert_queue().await, Some(local));
}

#[tokio::test]
async fn a_pack_written_before_queue_roles_imports_with_no_role_taken() {
    let store = Store::open().await;
    let queue_id = QueueId::new();
    let document: QueueDocument = serde_json::from_value(serde_json::json!({
        "id": queue_id,
        "name": RENAMED_ALERT_QUEUE,
        "description": "",
        "concurrency": 1,
    }))
    .unwrap();
    let documents = PackDocuments {
        queues: vec![document],
        actions: vec![ActionDocument {
            id: ActionId::new(),
            name: "alert".to_owned(),
            group: None,
            queue_id,
            enabled: true,
            concurrent: false,
            bypass_pause: false,
            execution_mode: ExecutionMode::Sequential,
            description: None,
            steps: Vec::new(),
            trigger_ids: Vec::new(),
        }],
        ..empty_documents()
    };

    store
        .import(bundle(store.data_dir(), &documents, &[]))
        .await
        .unwrap();

    assert_eq!(store.alert_queue().await, None);
}
