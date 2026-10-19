use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use flate2::Compression;
use flate2::write::GzEncoder;
use forge_storage::DataProvider;
use forge_storage::paths::credentials_key_path_in;
use time::OffsetDateTime;

use crate::encryption::encrypt_into;
use crate::error::TransferError;
use crate::manifest::{
    BUNDLE_FORMAT_VERSION, BundleKind, ENCRYPTED_PAYLOAD_ENTRY_NAME, EncryptionScheme,
    FullManifest, MANIFEST_ENTRY_NAME, PublicManifest,
};
use crate::naming::{BUNDLE_EXTENSION, backups_dir, ciphertext_path, fresh_stem, partial_path};
use crate::passphrase::Passphrase;
use crate::payload::{PayloadFile, append_bytes, append_file, append_verified, collect_payload};
use crate::private_file::create_private;
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
    let schema_version = provider.schema_version().await?;
    let work_dir = data_dir.clone();
    let output = destination.clone();
    let written = with_database_snapshot(&provider, &data_dir, &stem, move |snapshot| {
        write_bundle(BundleInput {
            data_dir: &work_dir,
            snapshot: &snapshot,
            destination: &output,
            passphrase: &passphrase,
            schema_version,
        })
    })
    .await;
    if written.is_err() {
        let _ = tokio::fs::remove_file(ciphertext_path(&destination)).await;
        let _ = tokio::fs::remove_file(partial_path(&destination)).await;
    }
    written.map(|()| destination)
}

struct BundleInput<'a> {
    data_dir: &'a Path,
    snapshot: &'a Path,
    destination: &'a Path,
    passphrase: &'a Passphrase,
    schema_version: u32,
}

fn write_bundle(input: BundleInput<'_>) -> Result<(), TransferError> {
    let key = credentials_key_path_in(input.data_dir);
    let files = collect_payload(input.data_dir, input.snapshot, &key)?;
    let created_at = OffsetDateTime::now_utc();
    let full = FullManifest {
        format_version: BUNDLE_FORMAT_VERSION,
        kind: BundleKind::Full,
        forge_version: FORGE_VERSION.to_owned(),
        schema_version: input.schema_version,
        created_at,
        contents: files.iter().map(|file| file.entry.clone()).collect(),
    };
    let public = PublicManifest {
        format_version: BUNDLE_FORMAT_VERSION,
        kind: BundleKind::Full,
        encryption: EncryptionScheme::age_scrypt(),
        forge_version: FORGE_VERSION.to_owned(),
        schema_version: input.schema_version,
        created_at,
    };

    let ciphertext = ciphertext_path(input.destination);
    write_encrypted_payload(&ciphertext, input.passphrase, &full, &files)?;
    let partial = partial_path(input.destination);
    write_outer(&partial, &public, &ciphertext)?;
    std::fs::remove_file(&ciphertext)?;
    std::fs::rename(&partial, input.destination)?;
    Ok(())
}

fn write_encrypted_payload(
    path: &Path,
    passphrase: &Passphrase,
    manifest: &FullManifest,
    files: &[PayloadFile],
) -> Result<(), TransferError> {
    let sink = BufWriter::new(create_private(path)?);
    let encrypted = encrypt_into(passphrase, sink)?;
    let mut builder = tar::Builder::new(GzEncoder::new(encrypted, Compression::default()));
    append_bytes(
        &mut builder,
        MANIFEST_ENTRY_NAME,
        &serde_json::to_vec(manifest).map_err(std::io::Error::from)?,
    )?;
    for file in files {
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
