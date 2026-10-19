use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use flate2::Compression;
use flate2::write::GzEncoder;
use forge_storage::DataProvider;
use forge_storage::paths::credentials_key_path_in;

use crate::error::TransferError;
use crate::naming::{LEGACY_ARCHIVE_EXTENSION, backups_dir, fresh_stem, partial_path};
use crate::payload::{append_verified, collect_payload};
use crate::private_file::create_private;
use crate::snapshot::with_database_snapshot;

pub async fn write_unencrypted_backup(
    provider: Arc<dyn DataProvider>,
    data_dir: PathBuf,
) -> Result<PathBuf, TransferError> {
    let stem = fresh_stem();
    let archive = backups_dir(&data_dir).join(format!("{stem}.{LEGACY_ARCHIVE_EXTENSION}"));
    let partial = partial_path(&archive);
    let work_dir = data_dir.clone();
    let (partial_out, archive_out) = (partial.clone(), archive.clone());
    let written = with_database_snapshot(&provider, &data_dir, &stem, move |snapshot| {
        write_archive(&work_dir, &snapshot, &partial_out)?;
        std::fs::rename(&partial_out, &archive_out)?;
        Ok(())
    })
    .await;
    if written.is_err() {
        let _ = tokio::fs::remove_file(&partial).await;
    }
    written.map(|()| archive)
}

fn write_archive(data_dir: &Path, snapshot: &Path, dest: &Path) -> Result<(), TransferError> {
    let files = collect_payload(data_dir, snapshot, &credentials_key_path_in(data_dir))?;
    let sink = BufWriter::new(create_private(dest)?);
    let mut builder = tar::Builder::new(GzEncoder::new(sink, Compression::default()));
    for file in &files {
        append_verified(&mut builder, file)?;
    }
    let mut sink = builder.into_inner()?.finish()?;
    sink.flush()?;
    sink.get_ref().sync_all()?;
    Ok(())
}
