use std::io;
use std::path::{Path, PathBuf};

use crate::bundle::blocking::run_blocking;
use crate::bundle::document_layout::PACK_LAYOUT;
use crate::bundle::layout::ArchiveScope;
use crate::bundle::manifest::ContentEntry;
use crate::bundle::naming::fresh_stem;
use crate::documents::archive::{
    DocumentManifest, carried_file, check_bundled_files, documents_version, stage_document_bundle,
};
use crate::documents::model::{PACK_DOCUMENTS_VERSION, PackDocuments};
use crate::error::TransferError;
use crate::pack::manifest::{PackManifest, parse_pack};

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
        carried_file(
            &self.staging_dir,
            &self.manifest.contents,
            PACK_LAYOUT,
            pack_path,
        )
    }
}

pub fn pack_staging_root(data_dir: &Path) -> PathBuf {
    data_dir.join(PACK_STAGING_DIR_NAME)
}

pub async fn stage_pack(bundle: PathBuf, data_dir: PathBuf) -> Result<StagedPack, TransferError> {
    let staging_dir = pack_staging_root(&data_dir).join(fresh_stem());
    let target = staging_dir.clone();
    let staged = run_blocking(bundle, move |bundle| stage(&bundle, &target)).await;
    if staged.is_err()
        && let Err(error) = tokio::fs::remove_dir_all(&staging_dir).await
    {
        tracing::warn!(%error, path = %staging_dir.display(), "pack staging could not be removed");
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
    let staged = stage_document_bundle(
        bundle,
        staging_dir,
        PACK_LAYOUT,
        ArchiveScope::Pack,
        parse_pack,
    )?;
    let documents = parse_documents(&staged.documents)?;
    check_bundled_files(&staged.manifest.contents, PACK_LAYOUT, &documents)?;
    Ok((staged.manifest, documents))
}

impl DocumentManifest for PackManifest {
    fn contents(&self) -> &[ContentEntry] {
        &self.contents
    }
}

fn parse_documents(bytes: &[u8]) -> Result<PackDocuments, TransferError> {
    let version = documents_version(bytes)?;
    if version > PACK_DOCUMENTS_VERSION {
        return Err(TransferError::NewerPackDocuments {
            found: version,
            supported: PACK_DOCUMENTS_VERSION,
        });
    }
    serde_json::from_slice(bytes).map_err(TransferError::damaged)
}
