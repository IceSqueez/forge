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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::path::{Path, PathBuf};

    use forge_storage::paths::{MEDIA_DIR_NAME, OVERLAYS_DIR_NAME};
    use tempfile::TempDir;

    use super::resolve_folder;
    use crate::naming::backups_dir;
    use crate::scheduled::status::FailureCategory;

    struct Layout {
        root: TempDir,
    }

    impl Layout {
        fn new() -> Self {
            let root = tempfile::tempdir().unwrap();
            std::fs::create_dir_all(root.path().join("data").join(MEDIA_DIR_NAME)).unwrap();
            Self { root }
        }

        fn data(&self) -> PathBuf {
            self.root.path().join("data")
        }

        fn outside(&self, name: &str) -> PathBuf {
            let path = self.root.path().join(name);
            std::fs::create_dir_all(&path).unwrap();
            path
        }
    }

    async fn resolve(data: &Path, configured: &Path) -> Result<PathBuf, FailureCategory> {
        resolve_folder(data, Some(configured)).await
    }

    #[tokio::test]
    async fn default_destination_is_created_inside_the_data_dir() {
        let layout = Layout::new();

        let folder = resolve_folder(&layout.data(), None).await.unwrap();

        assert_eq!(
            folder,
            std::fs::canonicalize(backups_dir(&layout.data())).unwrap()
        );
    }

    #[tokio::test]
    async fn an_existing_custom_folder_resolves_to_its_canonical_path() {
        let layout = Layout::new();
        let custom = layout.outside("usb");

        let folder = resolve(&layout.data(), &custom).await.unwrap();

        assert_eq!(folder, std::fs::canonicalize(custom).unwrap());
    }

    #[tokio::test]
    async fn a_missing_custom_folder_fails_and_is_never_created() {
        let layout = Layout::new();
        let unplugged = layout.root.path().join("unplugged").join("backups");

        let result = resolve(&layout.data(), &unplugged).await;

        assert_eq!(
            (result, unplugged.parent().unwrap().exists()),
            (Err(FailureCategory::DestinationMissing), false)
        );
    }

    #[tokio::test]
    async fn a_custom_destination_that_is_a_file_is_reported_missing() {
        let layout = Layout::new();
        let file = layout.root.path().join("not-a-folder");
        std::fs::write(&file, b"x").unwrap();

        assert_eq!(
            resolve(&layout.data(), &file).await,
            Err(FailureCategory::DestinationMissing)
        );
    }

    #[tokio::test]
    async fn a_destination_inside_the_backed_up_scope_is_refused() {
        let layout = Layout::new();
        let data = layout.data();
        let nested = data.join(MEDIA_DIR_NAME).join("backups");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::create_dir_all(data.join(OVERLAYS_DIR_NAME)).unwrap();
        let traversal = data.join(OVERLAYS_DIR_NAME).join("..").join(MEDIA_DIR_NAME);
        for candidate in [data.clone(), data.join(MEDIA_DIR_NAME), nested, traversal] {
            assert_eq!(
                resolve(&data, &candidate).await,
                Err(FailureCategory::DestinationInsideBackupScope),
                "{candidate:?}"
            );
        }
    }

    #[tokio::test]
    async fn a_sibling_whose_name_only_starts_like_a_scope_directory_is_allowed() {
        let layout = Layout::new();
        let lookalike = layout.data().join(format!("{MEDIA_DIR_NAME}-copies"));
        std::fs::create_dir_all(&lookalike).unwrap();

        assert!(resolve(&layout.data(), &lookalike).await.is_ok());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_symlink_pointing_into_the_backed_up_scope_is_refused() {
        let layout = Layout::new();
        let link = layout.root.path().join("innocent-looking");
        std::os::unix::fs::symlink(layout.data().join(MEDIA_DIR_NAME), &link).unwrap();

        assert_eq!(
            resolve(&layout.data(), &link).await,
            Err(FailureCategory::DestinationInsideBackupScope)
        );
    }
}
