use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use flate2::read::GzDecoder;
use forge_paths::CREDENTIALS_KEY_FILE_NAME;
use forge_storage::{
    DATABASE_FILE_NAME, DatabaseFileSchema, LAST_PRE_BASELINE_RELEASE, StoredSchema,
};

use crate::bundle::blocking::run_blocking;
use crate::bundle::category::BackupCategory;
use crate::bundle::durable_fs::{read_record, write_record};
use crate::bundle::encryption::{decrypt_from, decrypt_with_identity};
use crate::bundle::extract::Extractor;
use crate::bundle::layout::{ARCHIVE_ROOT, ArchiveScope};
use crate::bundle::limits::MAX_FULL_MANIFEST_BYTES;
use crate::bundle::manifest::{
    ENCRYPTED_PAYLOAD_ENTRY_NAME, FullManifest, MANIFEST_ENTRY_NAME, PublicManifest, check_schema,
    parse_full,
};
use crate::bundle::naming::fresh_stem;
use crate::bundle::outer::{open_outer, read_bounded};
use crate::bundle::passphrase::Passphrase;
use crate::bundle::reader::{BundleSummary, summarize_first_entry};
use crate::bundle::schedule_key::unwrap_identity;
use crate::error::TransferError;

pub(crate) const RESTORE_STAGING_DIR_NAME: &str = "restore-staging";
const STAGED_CATEGORIES_FILE: &str = "staged-categories.json";

pub struct StageRequest {
    pub bundle: PathBuf,
    pub data_dir: PathBuf,
    pub passphrase: Option<Passphrase>,
    pub database_schema: Arc<dyn DatabaseFileSchema>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagedRestore {
    staging_dir: PathBuf,
    summary: BundleSummary,
}

impl StagedRestore {
    pub fn staging_dir(&self) -> &Path {
        &self.staging_dir
    }

    pub fn summary(&self) -> &BundleSummary {
        &self.summary
    }

    pub fn payload_dir(&self) -> PathBuf {
        self.staging_dir.join(ARCHIVE_ROOT)
    }

    pub fn database_file(&self) -> PathBuf {
        self.payload_dir().join(DATABASE_FILE_NAME)
    }

    pub fn credentials_key_file(&self) -> Option<PathBuf> {
        Some(self.payload_dir().join(CREDENTIALS_KEY_FILE_NAME)).filter(|key| key.is_file())
    }
}

pub fn restore_staging_root(data_dir: &Path) -> PathBuf {
    data_dir.join(RESTORE_STAGING_DIR_NAME)
}

pub async fn stage_full_restore(request: StageRequest) -> Result<StagedRestore, TransferError> {
    let staging_dir = restore_staging_root(&request.data_dir).join(fresh_stem());
    let target = staging_dir.clone();
    let staged = run_blocking(request, move |request| stage(&request, &target)).await;
    if staged.is_err()
        && let Err(error) = tokio::fs::remove_dir_all(&staging_dir).await
    {
        tracing::warn!(%error, path = %staging_dir.display(), "restore staging could not be removed");
    }
    staged.map(|summary| StagedRestore {
        staging_dir,
        summary,
    })
}

pub async fn discard_staged_restore(staged: StagedRestore) -> Result<(), TransferError> {
    match tokio::fs::remove_dir_all(&staged.staging_dir).await {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e.into()),
        _ => Ok(()),
    }
}

fn stage(request: &StageRequest, staging_dir: &Path) -> Result<BundleSummary, TransferError> {
    std::fs::create_dir_all(staging_dir)?;
    let mut archive = open_outer(&request.bundle)?;
    let mut entries = archive.entries().map_err(|_| TransferError::NotABundle)?;
    let mut first = entries
        .next()
        .ok_or(TransferError::NotABundle)?
        .map_err(|_| TransferError::NotABundle)?;
    let summary = summarize_first_entry(&mut first)?;
    match &summary {
        BundleSummary::Encrypted(public) => {
            drop(first);
            let passphrase = request
                .passphrase
                .as_ref()
                .ok_or(TransferError::PassphraseRequired)?;
            let mut payload = entries
                .next()
                .ok_or_else(|| TransferError::damaged("the encrypted payload is missing"))?
                .map_err(TransferError::damaged)?;
            let is_payload = payload.header().entry_type().is_file()
                && payload.path().map_err(TransferError::damaged)?.as_ref()
                    == Path::new(ENCRYPTED_PAYLOAD_ENTRY_NAME);
            if !is_payload {
                return Err(TransferError::damaged("the encrypted payload is missing"));
            }
            let categories = extract_encrypted(&mut payload, public, passphrase, staging_dir)?;
            write_record(&staging_dir.join(STAGED_CATEGORIES_FILE), &categories)?;
        }
        BundleSummary::LegacyUnencrypted => {
            let mut extractor =
                Extractor::new(staging_dir.to_path_buf(), ArchiveScope::Backup, None);
            extractor.extract(&mut first)?;
            drop(first);
            for entry in entries.by_ref() {
                extractor.extract(&mut entry.map_err(TransferError::damaged)?)?;
            }
            extractor.finish()?;
        }
    }
    if entries.next().is_some() {
        return Err(TransferError::damaged(
            "the file carries data after the backup",
        ));
    }
    verify_staged_database(staging_dir, request.database_schema.as_ref())?;
    Ok(summary)
}

pub(crate) fn read_staged_categories(
    staging_dir: &Path,
) -> Result<Vec<BackupCategory>, TransferError> {
    Ok(read_record(&staging_dir.join(STAGED_CATEGORIES_FILE))?.unwrap_or_else(BackupCategory::all))
}

fn extract_encrypted(
    payload: impl Read,
    public: &PublicManifest,
    passphrase: &Passphrase,
    staging_dir: &Path,
) -> Result<Vec<BackupCategory>, TransferError> {
    let decrypted = if public.encryption.is_wrapped_recipient() {
        let wrapped = public
            .wrapped_identity
            .as_deref()
            .ok_or_else(|| TransferError::damaged("the backup carries no restore key"))?;
        let identity = unwrap_identity(wrapped, passphrase)?;
        decrypt_with_identity(&identity, payload)?
    } else {
        decrypt_from(passphrase, payload)?
    };
    let mut inner = tar::Archive::new(GzDecoder::new(decrypted));
    let manifest = {
        let mut entries = inner.entries().map_err(TransferError::damaged)?;
        let mut first = entries
            .next()
            .ok_or_else(|| TransferError::damaged("the backup manifest is missing"))?
            .map_err(TransferError::damaged)?;
        let manifest = read_full_manifest(&mut first)?;
        drop(first);
        let mut extractor = Extractor::new(
            staging_dir.to_path_buf(),
            ArchiveScope::Backup,
            Some(manifest.contents.clone()),
        );
        for entry in entries {
            extractor.extract(&mut entry.map_err(TransferError::damaged)?)?;
        }
        extractor.finish()?;
        manifest
    };
    io::copy(&mut inner.into_inner(), &mut io::sink()).map_err(TransferError::damaged)?;
    check_schema(manifest.schema_version)?;
    if manifest.categories != public.categories {
        return Err(TransferError::damaged(
            "the backup contents do not match its label",
        ));
    }
    Ok(manifest.categories)
}

fn read_full_manifest<R: Read>(
    entry: &mut tar::Entry<'_, R>,
) -> Result<FullManifest, TransferError> {
    let is_manifest = entry.header().entry_type().is_file()
        && entry.path().map_err(TransferError::damaged)?.as_ref() == Path::new(MANIFEST_ENTRY_NAME);
    if !is_manifest {
        return Err(TransferError::damaged("the backup manifest is missing"));
    }
    let size = entry.size();
    let bytes = read_bounded(entry, size, MAX_FULL_MANIFEST_BYTES)?;
    parse_full(&bytes)
}

fn verify_staged_database(
    staging_dir: &Path,
    database_schema: &dyn DatabaseFileSchema,
) -> Result<(), TransferError> {
    let payload_dir = staging_dir.join(ARCHIVE_ROOT);
    let stored = database_schema
        .inspect(&payload_dir.join(DATABASE_FILE_NAME))?
        .ok_or(TransferError::MissingDatabase)?;
    match stored.schema {
        StoredSchema::Current { version } => check_schema(version)?,
        StoredSchema::AdoptableLegacy { .. } => {}
        StoredSchema::PreBaseline { found } => {
            return Err(TransferError::SchemaNotAdoptable {
                found,
                release: LAST_PRE_BASELINE_RELEASE,
            });
        }
        StoredSchema::Unrecognized => return Err(TransferError::UnrecognizedDatabase),
    }
    if stored.holds_credentials && !payload_dir.join(CREDENTIALS_KEY_FILE_NAME).is_file() {
        return Err(TransferError::MissingCredentialsKey);
    }
    Ok(())
}
