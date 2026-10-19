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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::fs::File;
    use std::io::Read;

    use forge_storage::paths::CREDENTIALS_KEY_FILE_NAME;

    use super::*;
    use crate::layout::credentials_key_entry;

    const CREDENTIALS_KEY_FILE_ENV: &str = "FORGE_CREDENTIAL_KEY_FILE";

    static CREDENTIALS_KEY_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[allow(unsafe_code)]
    fn with_credentials_key_override<R>(key_file: &Path, body: impl FnOnce() -> R) -> R {
        let _guard = CREDENTIALS_KEY_ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let previous = std::env::var_os(CREDENTIALS_KEY_FILE_ENV);
        // SAFETY: CREDENTIALS_KEY_ENV_LOCK serialises every test in this binary that touches this variable, and no other test in it reads it.
        unsafe {
            std::env::set_var(CREDENTIALS_KEY_FILE_ENV, key_file);
        }
        let result = body();
        // SAFETY: still under CREDENTIALS_KEY_ENV_LOCK, restoring the value seen on entry.
        unsafe {
            match previous {
                Some(v) => std::env::set_var(CREDENTIALS_KEY_FILE_ENV, v),
                None => std::env::remove_var(CREDENTIALS_KEY_FILE_ENV),
            }
        }
        result
    }

    fn archived_file(archive: &Path, name: &str) -> Option<Vec<u8>> {
        let decoder = flate2::read::GzDecoder::new(File::open(archive).unwrap());
        let mut entries = tar::Archive::new(decoder);
        for entry in entries.entries().unwrap() {
            let mut entry = entry.unwrap();
            if entry.path().unwrap() == Path::new(name) {
                let mut bytes = Vec::new();
                entry.read_to_end(&mut bytes).unwrap();
                return Some(bytes);
            }
        }
        None
    }

    #[test]
    fn write_archive_packs_the_overridden_credentials_key_instead_of_the_data_dir_one() {
        let data_dir = tempfile::tempdir().unwrap();
        std::fs::write(data_dir.path().join(CREDENTIALS_KEY_FILE_NAME), b"stale").unwrap();
        let key_dir = tempfile::tempdir().unwrap();
        let key_file = key_dir.path().join("custom.key");
        std::fs::write(&key_file, b"live key bytes").unwrap();
        let snapshot = data_dir.path().join("snapshot.db");
        std::fs::write(&snapshot, b"db").unwrap();
        let archive = data_dir.path().join("backup.tar.gz");

        with_credentials_key_override(&key_file, || {
            write_archive(data_dir.path(), &snapshot, &archive)
        })
        .unwrap();

        let packed = archived_file(&archive, &credentials_key_entry());
        assert_eq!(packed.as_deref(), Some(&b"live key bytes"[..]));
    }
}
