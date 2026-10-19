use std::io;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::TransferError;
use crate::extract::Extractor;
use crate::layout::{ARCHIVE_SEPARATOR, ArchiveScope};
use crate::limits::{MAX_PACK_DOCUMENTS_BYTES, MAX_PACK_MANIFEST_BYTES};
use crate::manifest::MANIFEST_ENTRY_NAME;
use crate::naming::fresh_stem;
use crate::outer::{open_outer, read_bounded};
use crate::pack::documents::{ClipSourceDocument, PACK_DOCUMENTS_VERSION, PackDocuments};
use crate::pack::layout::documents_entry;
use crate::pack::manifest::{PackManifest, parse_pack};
use crate::snapshot::run_blocking;

pub const PACK_STAGING_DIR_NAME: &str = "pack-staging";

#[derive(Debug, Clone, PartialEq)]
pub struct StagedPack {
    staging_dir: PathBuf,
    manifest: PackManifest,
    documents: PackDocuments,
}

impl StagedPack {
    pub fn staging_dir(&self) -> &Path {
        &self.staging_dir
    }

    pub fn manifest(&self) -> &PackManifest {
        &self.manifest
    }

    pub fn documents(&self) -> &PackDocuments {
        &self.documents
    }

    pub fn file(&self, pack_path: &str) -> Option<PathBuf> {
        let carried = pack_path != documents_entry()
            && self
                .manifest
                .contents
                .iter()
                .any(|entry| entry.path == pack_path);
        carried.then(|| staged_path(&self.staging_dir, pack_path))
    }
}

pub fn pack_staging_root(data_dir: &Path) -> PathBuf {
    data_dir.join(PACK_STAGING_DIR_NAME)
}

pub async fn stage_pack(bundle: PathBuf, data_dir: PathBuf) -> Result<StagedPack, TransferError> {
    let staging_dir = pack_staging_root(&data_dir).join(fresh_stem());
    let target = staging_dir.clone();
    let staged = run_blocking(bundle, move |bundle| stage(&bundle, &target)).await;
    if staged.is_err() {
        let _ = tokio::fs::remove_dir_all(&staging_dir).await;
    }
    staged.map(|(manifest, documents)| StagedPack {
        staging_dir,
        manifest,
        documents,
    })
}

pub async fn discard_staged_pack(staged: StagedPack) -> Result<(), TransferError> {
    match tokio::fs::remove_dir_all(&staged.staging_dir).await {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e.into()),
        _ => Ok(()),
    }
}

fn stage(
    bundle: &Path,
    staging_dir: &Path,
) -> Result<(PackManifest, PackDocuments), TransferError> {
    std::fs::create_dir_all(staging_dir)?;
    let mut archive = open_outer(bundle)?;
    let mut entries = archive.entries().map_err(|_| TransferError::NotABundle)?;
    let mut first = entries
        .next()
        .ok_or(TransferError::NotABundle)?
        .map_err(|_| TransferError::NotABundle)?;
    let is_manifest = first.header().entry_type().is_file()
        && first
            .path()
            .map_err(|_| TransferError::NotABundle)?
            .as_ref()
            == Path::new(MANIFEST_ENTRY_NAME);
    if !is_manifest {
        return Err(TransferError::NotABundle);
    }
    let size = first.size();
    let manifest = parse_pack(&read_bounded(&mut first, size, MAX_PACK_MANIFEST_BYTES)?)?;
    drop(first);

    let documents_path = documents_entry();
    let documents_size = manifest
        .contents
        .iter()
        .find(|entry| entry.path == documents_path)
        .map(|entry| entry.size)
        .ok_or_else(|| TransferError::ContentsMismatch {
            path: documents_path.clone(),
        })?;
    if documents_size > MAX_PACK_DOCUMENTS_BYTES {
        return Err(TransferError::EntryTooLarge {
            path: documents_path,
            size: documents_size,
            limit: MAX_PACK_DOCUMENTS_BYTES,
        });
    }

    let mut extractor = Extractor::new(
        staging_dir.to_path_buf(),
        ArchiveScope::Pack,
        Some(manifest.contents.clone()),
    );
    for entry in entries {
        extractor.extract(&mut entry.map_err(TransferError::damaged)?)?;
    }
    extractor.finish()?;

    let bytes = std::fs::read(staged_path(staging_dir, &documents_path))?;
    let documents = parse_documents(&bytes)?;
    check_bundled_files(&manifest, &documents)?;
    Ok((manifest, documents))
}

#[derive(Deserialize)]
struct DocumentsHead {
    version: u32,
}

fn parse_documents(bytes: &[u8]) -> Result<PackDocuments, TransferError> {
    let head: DocumentsHead = serde_json::from_slice(bytes).map_err(TransferError::damaged)?;
    if head.version > PACK_DOCUMENTS_VERSION {
        return Err(TransferError::NewerPackDocuments {
            found: head.version,
            supported: PACK_DOCUMENTS_VERSION,
        });
    }
    serde_json::from_slice(bytes).map_err(TransferError::damaged)
}

fn check_bundled_files(
    manifest: &PackManifest,
    documents: &PackDocuments,
) -> Result<(), TransferError> {
    let documents_path = documents_entry();
    let carried = |path: &str| {
        path != documents_path && manifest.contents.iter().any(|entry| entry.path == path)
    };
    if let Some(missing) = documents.bundled_files().find(|path| !carried(path)) {
        return Err(TransferError::ContentsMismatch {
            path: missing.to_owned(),
        });
    }
    let dangling_clip = documents.clips.iter().any(|clip| match &clip.source {
        ClipSourceDocument::Media { media_id } => {
            !documents.media.iter().any(|media| &media.id == media_id)
        }
        ClipSourceDocument::File { .. } | ClipSourceDocument::Missing => false,
    });
    if dangling_clip {
        return Err(TransferError::damaged(
            "a sound clip refers to media the pack does not carry",
        ));
    }
    Ok(())
}

fn staged_path(staging_dir: &Path, pack_path: &str) -> PathBuf {
    pack_path
        .split(ARCHIVE_SEPARATOR)
        .fold(staging_dir.to_path_buf(), |path, segment| {
            path.join(segment)
        })
}
