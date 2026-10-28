use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use flate2::Compression;
use flate2::write::GzEncoder;
use forge_paths::credentials_key_path_in;
use forge_storage::DataProvider;

use crate::category::BackupCategory;
use crate::error::TransferError;
use crate::naming::{LEGACY_ARCHIVE_EXTENSION, backups_dir, partial_path};
use crate::payload::{append_verified, collect_payload};
use crate::private_file::create_private;
use crate::safety_copies::prune_after_safety_copy;
use crate::snapshot::with_database_snapshot;

pub(crate) async fn write_safety_snapshot(
    provider: &Arc<dyn DataProvider>,
    data_dir: &Path,
    stem: String,
) -> Result<PathBuf, TransferError> {
    let destination = backups_dir(data_dir).join(format!("{stem}.{LEGACY_ARCHIVE_EXTENSION}"));
    let work_dir = data_dir.to_path_buf();
    let output = destination.clone();
    let written = with_database_snapshot(provider, data_dir, &stem, move |snapshot| {
        write_plain_archive(&work_dir, &snapshot, &output)
    })
    .await;
    if written.is_err()
        && let Err(error) = tokio::fs::remove_file(partial_path(&destination)).await
    {
        tracing::debug!(%error, "partial safety snapshot could not be removed");
    }
    if written.is_ok() {
        prune_after_safety_copy(provider.as_ref(), data_dir, &destination).await;
    }
    written.map(|()| destination)
}

fn write_plain_archive(
    data_dir: &Path,
    database_snapshot: &Path,
    destination: &Path,
) -> Result<(), TransferError> {
    let key = credentials_key_path_in(data_dir);
    let files = collect_payload(data_dir, database_snapshot, &key, &BackupCategory::ALL)?;
    let partial = partial_path(destination);
    let sink = BufWriter::new(create_private(&partial)?);
    let mut builder = tar::Builder::new(GzEncoder::new(sink, Compression::default()));
    for file in &files {
        append_verified(&mut builder, file)?;
    }
    let mut sink = builder.into_inner()?.finish()?;
    sink.flush()?;
    sink.get_ref().sync_all()?;
    drop(sink);
    std::fs::rename(&partial, destination)?;
    Ok(())
}
