use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use crate::layout::INCLUDED_DIRS;
use crate::naming::backups_dir;
use crate::scheduled::status::FailureCategory;

const CASE_INSENSITIVE_FILESYSTEM: bool = cfg!(any(windows, target_os = "macos"));

pub(crate) async fn resolve_folder(
    data_dir: &Path,
    configured: Option<&Path>,
) -> Result<PathBuf, FailureCategory> {
    let requested = match configured {
        Some(folder) => folder.to_path_buf(),
        None => {
            let default = backups_dir(data_dir);
            tokio::fs::create_dir_all(&default)
                .await
                .map_err(|_| FailureCategory::DestinationNotWritable)?;
            default
        }
    };
    let folder = tokio::fs::canonicalize(&requested)
        .await
        .map_err(|error| match error.kind() {
            ErrorKind::NotFound => FailureCategory::DestinationMissing,
            _ => FailureCategory::DestinationNotWritable,
        })?;
    let is_directory = tokio::fs::metadata(&folder)
        .await
        .map(|metadata| metadata.is_dir())
        .unwrap_or(false);
    if !is_directory {
        return Err(FailureCategory::DestinationMissing);
    }
    let data_root = tokio::fs::canonicalize(data_dir)
        .await
        .unwrap_or_else(|_| data_dir.to_path_buf());
    let scope_folder = folder.clone();
    let inside =
        tokio::task::spawn_blocking(move || inside_backup_scope(&scope_folder, &data_root))
            .await
            .unwrap_or(true);
    if inside {
        return Err(FailureCategory::DestinationInsideBackupScope);
    }
    Ok(folder)
}

fn inside_backup_scope(folder: &Path, data_root: &Path) -> bool {
    segments(folder) == segments(data_root)
        || INCLUDED_DIRS
            .iter()
            .any(|dir| is_within(folder, &data_root.join(dir)))
}

pub(crate) fn is_within(path: &Path, ancestor: &Path) -> bool {
    let canonical =
        |path: &Path| std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    segments(&canonical(path)).starts_with(&segments(&canonical(ancestor)))
}

fn segments(path: &Path) -> Vec<String> {
    path.components()
        .map(|component| {
            let text = component.as_os_str().to_string_lossy().into_owned();
            if CASE_INSENSITIVE_FILESYSTEM {
                text.to_lowercase()
            } else {
                text
            }
        })
        .collect()
}
