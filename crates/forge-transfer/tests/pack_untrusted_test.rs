#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use forge_registry::{SubActionRegistry, TriggerRegistry};
use forge_storage::paths::{MEDIA_DIR_NAME, OVERLAYS_DIR_NAME};
use forge_storage::{
    DATABASE_FILE_NAME, DataProvider, EXPECTED_SCHEMA_VERSION, MediaBlobId, MediaFormat,
    clip_source_referrer,
};
use forge_storage_sqlite::SqliteBackend;
use forge_transfer::{
    BUNDLE_FORMAT_VERSION, BundleKind, ClipDocument, ClipSourceDocument, ClosureRegistries,
    ContentEntry, MANIFEST_ENTRY_NAME, MediaDocument, OverlayDocument, OverlayFileDocument,
    PACK_DOCUMENTS_VERSION, PackDocuments, PackImportOutcome, PackImportRequest, PackManifest,
    TransferError, apply_pack_import, load_pack_inventory, pack_staging_root, plan_pack_import,
    stage_pack,
};
use forge_types::ClipId;
use sha2::{Digest, Sha256};
use tempfile::TempDir;
use time::OffsetDateTime;

const SQLITE_KEY: [u8; 32] = [0x42; 32];
const DOCUMENTS_ENTRY: &str = "pack/documents.json";
const MEDIA_FILE: &str = "pack/media/0";
const OVERLAY_FILE: &str = "pack/overlays/0/payload.css";
const ESCAPE_NAME: &str = "escape.css";
const LOCAL_OGG: &[u8] = b"OggS\x00\x02\x00\x00local sound";
const PACK_OGG: &[u8] = b"OggS\x00\x02\x00\x00pack sound";

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

    async fn import(&self, bundle: PathBuf) -> Result<PackImportOutcome, TransferError> {
        let sub_actions = SubActionRegistry::new();
        let triggers = TriggerRegistry::new();
        let registries = ClosureRegistries {
            sub_actions: &sub_actions,
            triggers: &triggers,
            schema: None,
        };
        let staged = stage_pack(bundle, self.data_dir().to_path_buf()).await?;
        let inventory = load_pack_inventory(self.provider.as_ref()).await?;
        let plan = plan_pack_import(&staged, &inventory, registries);
        apply_pack_import(PackImportRequest {
            provider: Arc::clone(&self.provider),
            registries,
            staged: &staged,
            plan,
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
