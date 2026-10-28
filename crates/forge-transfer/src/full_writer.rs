use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use flate2::Compression;
use flate2::write::GzEncoder;
use forge_paths::credentials_key_path_in;
use forge_storage::DataProvider;
use sha2::{Digest, Sha256};
use time::OffsetDateTime;

use crate::cancel::CancelFlag;
use crate::category::BackupCategory;
use crate::digest::to_hex;
use crate::error::TransferError;
use crate::manifest::{
    BUNDLE_FORMAT_VERSION, BundleKind, ENCRYPTED_PAYLOAD_ENTRY_NAME, FullManifest,
    MANIFEST_ENTRY_NAME, PublicManifest,
};
use crate::naming::{
    BUNDLE_EXTENSION, backups_dir, ciphertext_path, fresh_scheduled_stem, fresh_stem, partial_path,
};
use crate::passphrase::Passphrase;
use crate::payload::{PayloadFile, append_bytes, append_file, append_verified, collect_payload};
use crate::private_file::create_private;
use crate::protection::Protection;
use crate::schedule_key::ScheduleKey;
use crate::snapshot::with_database_snapshot;

pub(crate) const FORGE_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackupDestination {
    BackupsDir,
    File(PathBuf),
}

pub struct FullBackupRequest {
    pub provider: Arc<dyn DataProvider>,
    pub data_dir: PathBuf,
    pub destination: BackupDestination,
    pub passphrase: Passphrase,
}

pub struct ScheduledBackupRequest {
    pub provider: Arc<dyn DataProvider>,
    pub data_dir: PathBuf,
    pub folder: PathBuf,
    pub key: ScheduleKey,
    pub categories: Vec<BackupCategory>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScheduledWrite {
    Written { path: PathBuf, fingerprint: String },
    Unchanged { fingerprint: String },
}

pub async fn write_full_backup(request: FullBackupRequest) -> Result<PathBuf, TransferError> {
    let FullBackupRequest {
        provider,
        data_dir,
        destination,
        passphrase,
    } = request;
    passphrase.ensure_strong_enough()?;
    let stem = fresh_stem();
    let destination = match destination {
        BackupDestination::BackupsDir => {
            backups_dir(&data_dir).join(format!("{stem}.{BUNDLE_EXTENSION}"))
        }
        BackupDestination::File(path) => path,
    };
    let written = write_protected(WriteJob {
        provider,
        data_dir,
        stem,
        destination,
        protection: Protection::Passphrase(passphrase),
        categories: BackupCategory::all(),
        unchanged_since: None,
        cancel: CancelFlag::new(),
    })
    .await?;
    match written {
        ScheduledWrite::Written { path, .. } => Ok(path),
        ScheduledWrite::Unchanged { .. } => Err(TransferError::Interrupted {
            reason: "the backup was skipped as unchanged".to_owned(),
        }),
    }
}

pub async fn write_scheduled_backup(
    request: ScheduledBackupRequest,
) -> Result<PathBuf, TransferError> {
    match write_scheduled_backup_unless_unchanged(request, None).await? {
        ScheduledWrite::Written { path, .. } => Ok(path),
        ScheduledWrite::Unchanged { .. } => Err(TransferError::Interrupted {
            reason: "the backup was skipped as unchanged".to_owned(),
        }),
    }
}

pub async fn write_scheduled_backup_unless_unchanged(
    request: ScheduledBackupRequest,
    unchanged_since: Option<String>,
) -> Result<ScheduledWrite, TransferError> {
    write_scheduled_backup_cancellable(request, unchanged_since, CancelFlag::new()).await
}

pub async fn write_scheduled_backup_cancellable(
    request: ScheduledBackupRequest,
    unchanged_since: Option<String>,
    cancel: CancelFlag,
) -> Result<ScheduledWrite, TransferError> {
    let ScheduledBackupRequest {
        provider,
        data_dir,
        folder,
        key,
        categories,
    } = request;
    let protection = Protection::scheduled(key)?;
    let stem = fresh_scheduled_stem();
    let destination = folder.join(format!("{stem}.{BUNDLE_EXTENSION}"));
    write_protected(WriteJob {
        provider,
        data_dir,
        stem,
        destination,
        protection,
        categories: with_database(categories),
        unchanged_since,
        cancel,
    })
    .await
}

fn with_database(categories: Vec<BackupCategory>) -> Vec<BackupCategory> {
    BackupCategory::ALL
        .into_iter()
        .filter(|category| *category == BackupCategory::Database || categories.contains(category))
        .collect()
}

struct WriteJob {
    provider: Arc<dyn DataProvider>,
    data_dir: PathBuf,
    stem: String,
    destination: PathBuf,
    protection: Protection,
    categories: Vec<BackupCategory>,
    unchanged_since: Option<String>,
    cancel: CancelFlag,
}

async fn write_protected(job: WriteJob) -> Result<ScheduledWrite, TransferError> {
    let WriteJob {
        provider,
        data_dir,
        stem,
        destination,
        protection,
        categories,
        unchanged_since,
        cancel,
    } = job;
    let schema_version = provider.schema_version().await?;
    let work_dir = data_dir.clone();
    let output = destination.clone();
    let written = with_database_snapshot(&provider, &data_dir, &stem, move |snapshot| {
        write_bundle(BundleInput {
            data_dir: &work_dir,
            snapshot: &snapshot,
            destination: &output,
            protection: &protection,
            categories: &categories,
            schema_version,
            unchanged_since: unchanged_since.as_deref(),
            cancel: &cancel,
        })
    })
    .await;
    if written.is_err() {
        if let Err(error) = tokio::fs::remove_file(ciphertext_path(&destination)).await {
            tracing::debug!(%error, "partial ciphertext could not be removed");
        }
        if let Err(error) = tokio::fs::remove_file(partial_path(&destination)).await {
            tracing::debug!(%error, "partial export could not be removed");
        }
    }
    written.map(|outcome| {
        if outcome.written {
            ScheduledWrite::Written {
                path: destination,
                fingerprint: outcome.fingerprint,
            }
        } else {
            ScheduledWrite::Unchanged {
                fingerprint: outcome.fingerprint,
            }
        }
    })
}

struct BundleOutcome {
    fingerprint: String,
    written: bool,
}

struct BundleInput<'a> {
    data_dir: &'a Path,
    snapshot: &'a Path,
    destination: &'a Path,
    protection: &'a Protection,
    categories: &'a [BackupCategory],
    schema_version: u32,
    unchanged_since: Option<&'a str>,
    cancel: &'a CancelFlag,
}

fn content_fingerprint(input: &BundleInput<'_>, files: &[PayloadFile]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.protection.key_fingerprint().unwrap_or_default());
    hasher.update(
        input
            .destination
            .parent()
            .map(|folder| folder.to_string_lossy().into_owned())
            .unwrap_or_default(),
    );
    for category in input.categories {
        hasher.update(format!("{category:?}"));
    }
    for file in files {
        hasher.update(&file.entry.path);
        hasher.update(file.entry.size.to_be_bytes());
        hasher.update(&file.entry.sha256_hex);
    }
    to_hex(&hasher.finalize())
}

fn write_bundle(input: BundleInput<'_>) -> Result<BundleOutcome, TransferError> {
    let key = credentials_key_path_in(input.data_dir);
    let files = collect_payload(input.data_dir, input.snapshot, &key, input.categories)?;
    input.cancel.checkpoint()?;
    let fingerprint = content_fingerprint(&input, &files);
    if input.unchanged_since == Some(fingerprint.as_str()) {
        return Ok(BundleOutcome {
            fingerprint,
            written: false,
        });
    }
    let created_at = OffsetDateTime::now_utc();
    let full = FullManifest {
        format_version: BUNDLE_FORMAT_VERSION,
        kind: BundleKind::Full,
        forge_version: FORGE_VERSION.to_owned(),
        schema_version: input.schema_version,
        created_at,
        categories: input.categories.to_vec(),
        contents: files.iter().map(|file| file.entry.clone()).collect(),
    };
    let public = PublicManifest {
        format_version: BUNDLE_FORMAT_VERSION,
        kind: BundleKind::Full,
        encryption: input.protection.scheme(),
        forge_version: FORGE_VERSION.to_owned(),
        schema_version: input.schema_version,
        created_at,
        origin: input.protection.origin(),
        categories: input.categories.to_vec(),
        key_fingerprint: input.protection.key_fingerprint(),
        wrapped_identity: input.protection.wrapped_identity(),
    };

    let ciphertext = ciphertext_path(input.destination);
    write_encrypted_payload(&ciphertext, input.protection, &full, &files, input.cancel)?;
    input.cancel.checkpoint()?;
    let partial = partial_path(input.destination);
    write_outer(&partial, &public, &ciphertext)?;
    std::fs::remove_file(&ciphertext)?;
    std::fs::rename(&partial, input.destination)?;
    Ok(BundleOutcome {
        fingerprint,
        written: true,
    })
}

fn write_encrypted_payload(
    path: &Path,
    protection: &Protection,
    manifest: &FullManifest,
    files: &[PayloadFile],
    cancel: &CancelFlag,
) -> Result<(), TransferError> {
    let sink = BufWriter::new(create_private(path)?);
    let encrypted = protection.seal(sink)?;
    let mut builder = tar::Builder::new(GzEncoder::new(encrypted, Compression::default()));
    append_bytes(
        &mut builder,
        MANIFEST_ENTRY_NAME,
        &serde_json::to_vec(manifest).map_err(std::io::Error::from)?,
    )?;
    for file in files {
        cancel.checkpoint()?;
        append_verified(&mut builder, file)?;
    }
    let encrypted = builder.into_inner()?.finish()?;
    let mut sink = encrypted.finish()?;
    sink.flush()?;
    sink.get_ref().sync_all()?;
    Ok(())
}

fn write_outer(
    path: &Path,
    manifest: &PublicManifest,
    ciphertext: &Path,
) -> Result<(), TransferError> {
    let sink = BufWriter::new(create_private(path)?);
    let mut builder = tar::Builder::new(GzEncoder::new(sink, Compression::none()));
    append_bytes(
        &mut builder,
        MANIFEST_ENTRY_NAME,
        &serde_json::to_vec(manifest).map_err(std::io::Error::from)?,
    )?;
    append_file(&mut builder, ENCRYPTED_PAYLOAD_ENTRY_NAME, ciphertext)?;
    let mut sink = builder.into_inner()?.finish()?;
    sink.flush()?;
    sink.get_ref().sync_all()?;
    Ok(())
}
