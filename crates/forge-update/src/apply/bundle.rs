use std::ffi::OsStr;
use std::fs::File;
use std::io::BufReader;
use std::path::{Component, Path, PathBuf};

use flate2::read::GzDecoder;

use super::files::{FileRetry, remove_path, swap_in};
use crate::UpdateError;

const EXTRACT_DIR_PREFIX: &str = ".forge-update-extract-";
const BUNDLE_EXTENSION: &str = "app";

pub(super) fn apply(
    archive: &Path,
    target: &Path,
    previous: &Path,
    retry: FileRetry<'_>,
) -> Result<(), UpdateError> {
    let parent = target.parent().ok_or(UpdateError::PathWithoutFileName)?;
    let extract_dir = parent.join(format!("{EXTRACT_DIR_PREFIX}{}", std::process::id()));
    retry.run(|| remove_path(&extract_dir))?;
    let swapped = extract_archive(archive, &extract_dir)
        .and_then(|()| single_bundle(&extract_dir))
        .and_then(|bundle| swap_in(target, &bundle, previous, retry));
    #[expect(
        clippy::let_underscore_must_use,
        reason = "best-effort cleanup; the swap result is what gets reported"
    )]
    let _ = remove_path(&extract_dir);
    swapped?;
    #[expect(
        clippy::let_underscore_must_use,
        reason = "a leftover archive does not undo a finished swap"
    )]
    let _ = remove_path(archive);
    Ok(())
}

fn extract_archive(archive: &Path, into: &Path) -> Result<(), UpdateError> {
    ensure_entries_stay_inside(archive)?;
    std::fs::create_dir_all(into)?;
    let mut entries = open_archive(archive)?;
    entries.set_preserve_permissions(true);
    entries.set_preserve_mtime(true);
    entries.set_overwrite(false);
    entries.unpack(into)?;
    Ok(())
}

fn open_archive(archive: &Path) -> Result<tar::Archive<GzDecoder<BufReader<File>>>, UpdateError> {
    Ok(tar::Archive::new(GzDecoder::new(BufReader::new(
        File::open(archive)?,
    ))))
}

fn ensure_entries_stay_inside(archive: &Path) -> Result<(), UpdateError> {
    let mut entries = open_archive(archive)?;
    for entry in entries.entries()? {
        let entry = entry?;
        let escapes = entry.path()?.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        });
        if escapes {
            return Err(UpdateError::MalformedPayload);
        }
    }
    Ok(())
}

fn single_bundle(dir: &Path) -> Result<PathBuf, UpdateError> {
    let mut bundles = std::fs::read_dir(dir)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir() && path.extension() == Some(OsStr::new(BUNDLE_EXTENSION)));
    match (bundles.next(), bundles.next()) {
        (Some(bundle), None) => Ok(bundle),
        _ => Err(UpdateError::MalformedPayload),
    }
}
