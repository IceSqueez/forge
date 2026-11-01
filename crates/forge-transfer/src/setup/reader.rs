use std::path::{Path, PathBuf};

use crate::bundle::blocking::{discard_staging, stage_into};
use crate::bundle::document_layout::SETUP_LAYOUT;
use crate::bundle::layout::ArchiveScope;
use crate::bundle::naming::fresh_stem;
use crate::documents::archive::{
    carried_file, check_bundled_files, documents_version, stage_document_bundle,
};
use crate::documents::model::PACK_DOCUMENTS_VERSION;
use crate::error::TransferError;
use crate::setup::documents::{SETUP_DOCUMENTS_VERSION, SetupDocuments};
use crate::setup::manifest::{SetupManifest, parse_setup};

pub(crate) const SETUP_STAGING_DIR_NAME: &str = "setup-staging";

#[derive(Debug, Clone)]
pub struct StagedSetup {
    staging_dir: PathBuf,
    manifest: SetupManifest,
    documents: SetupDocuments,
}

impl StagedSetup {
    pub fn staging_dir(&self) -> &Path {
        &self.staging_dir
    }

    pub fn manifest(&self) -> &SetupManifest {
        &self.manifest
    }

    pub fn documents(&self) -> &SetupDocuments {
        &self.documents
    }

    pub fn file(&self, setup_path: &str) -> Option<PathBuf> {
        carried_file(
            &self.staging_dir,
            &self.manifest.contents,
            SETUP_LAYOUT,
            setup_path,
        )
    }
}

pub fn setup_staging_root(data_dir: &Path) -> PathBuf {
    data_dir.join(SETUP_STAGING_DIR_NAME)
}

pub async fn stage_setup(bundle: PathBuf, data_dir: PathBuf) -> Result<StagedSetup, TransferError> {
    let staging_dir = setup_staging_root(&data_dir).join(fresh_stem());
    let staged = stage_into(&staging_dir, bundle, |bundle, target| {
        stage(&bundle, target)
    })
    .await;
    staged.map(|(manifest, documents)| StagedSetup {
        staging_dir,
        manifest,
        documents,
    })
}

pub async fn discard_staged_setup(staged: StagedSetup) -> Result<(), TransferError> {
    discard_staging(staged.staging_dir).await
}

fn stage(
    bundle: &Path,
    staging_dir: &Path,
) -> Result<(SetupManifest, SetupDocuments), TransferError> {
    let staged = stage_document_bundle(
        bundle,
        staging_dir,
        SETUP_LAYOUT,
        ArchiveScope::Setup,
        parse_setup,
    )?;
    let documents = parse_documents(&staged.documents)?;
    check_bundled_files(
        &staged.manifest.contents,
        SETUP_LAYOUT,
        &documents.configuration,
    )?;
    Ok((staged.manifest, documents))
}

fn parse_documents(bytes: &[u8]) -> Result<SetupDocuments, TransferError> {
    let version = documents_version(bytes)?;
    if version > SETUP_DOCUMENTS_VERSION {
        return Err(TransferError::NewerSetupDocuments {
            found: version,
            supported: SETUP_DOCUMENTS_VERSION,
        });
    }
    let documents: SetupDocuments =
        serde_json::from_slice(bytes).map_err(TransferError::damaged)?;
    if documents.configuration.version > PACK_DOCUMENTS_VERSION {
        return Err(TransferError::NewerSetupDocuments {
            found: documents.configuration.version,
            supported: PACK_DOCUMENTS_VERSION,
        });
    }
    if documents.configuration.version < PACK_DOCUMENTS_VERSION {
        return Err(TransferError::RetiredDocuments {
            found: documents.configuration.version,
            supported: PACK_DOCUMENTS_VERSION,
        });
    }
    Ok(documents)
}
