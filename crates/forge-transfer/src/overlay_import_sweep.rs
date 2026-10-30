use std::io;
use std::path::Path;
use std::time::{Duration, SystemTime};

use forge_storage::{OverlayId, OverlayRepo};

use crate::documents::overlay_files::PENDING_IMPORT_MARKER;
use crate::error::TransferError;

const ABANDONED_IMPORT_AGE: Duration = Duration::from_hours(1);

fn is_abandoned(marker: &std::fs::Metadata, now: SystemTime) -> bool {
    marker
        .modified()
        .ok()
        .and_then(|written| now.duration_since(written).ok())
        .is_some_and(|age| age >= ABANDONED_IMPORT_AGE)
}

pub async fn sweep_abandoned_overlay_imports(
    overlays: &dyn OverlayRepo,
    overlay_root: &Path,
) -> Result<usize, TransferError> {
    let mut entries = match tokio::fs::read_dir(overlay_root).await {
        Ok(entries) => entries,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(0),
        Err(e) => return Err(e.into()),
    };
    let now = SystemTime::now();
    let mut removed = 0;
    while let Some(entry) = entries.next_entry().await? {
        let marker = entry.path().join(PENDING_IMPORT_MARKER);
        let metadata = match tokio::fs::metadata(&marker).await {
            Ok(metadata) => metadata,
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::NotFound | io::ErrorKind::NotADirectory
                ) =>
            {
                continue;
            }
            Err(e) => return Err(e.into()),
        };
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if overlays.get(&OverlayId::new(name)).await?.is_some() {
            tokio::fs::remove_file(&marker).await?;
        } else if is_abandoned(&metadata, now) {
            tokio::fs::remove_dir_all(entry.path()).await?;
            removed += 1;
        }
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;

    use forge_storage::DataProvider;
    use forge_storage_sqlite::SqliteBackend;
    use tempfile::TempDir;

    use super::*;

    const KEY: [u8; 32] = [0x5c; 32];
    const LAPSED: Duration = Duration::from_secs(2 * 60 * 60);
    const RECENT: Duration = Duration::from_secs(59 * 60);

    struct Rig {
        backend: Arc<SqliteBackend>,
        root: PathBuf,
        _dir: TempDir,
    }

    impl Rig {
        async fn open() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let backend = SqliteBackend::open_for_test(
                "sqlite::memory:",
                KEY,
                dir.path().join("media"),
                None,
            )
            .await
            .unwrap();
            let root = dir.path().join("overlays");
            std::fs::create_dir(&root).unwrap();
            Self {
                backend: Arc::new(backend),
                root,
                _dir: dir,
            }
        }

        async fn sweep(&self) -> Result<usize, TransferError> {
            sweep_abandoned_overlay_imports(self.backend.overlay_repo().as_ref(), &self.root).await
        }

        async fn stored_overlay(&self) -> String {
            self.backend
                .overlay_repo()
                .create("Stored", "overlay.alert", 1)
                .await
                .unwrap()
                .id
                .as_str()
                .to_owned()
        }

        fn folder(&self, name: &str) -> PathBuf {
            let folder = self.root.join(name);
            std::fs::create_dir(&folder).unwrap();
            std::fs::write(folder.join("alert.css"), b"body {}").unwrap();
            folder
        }

        fn marked_folder(&self, name: &str, marker_age: Duration) -> PathBuf {
            let folder = self.folder(name);
            let marker = folder.join(PENDING_IMPORT_MARKER);
            std::fs::write(&marker, []).unwrap();
            let written = SystemTime::now().checked_sub(marker_age).unwrap();
            std::fs::OpenOptions::new()
                .write(true)
                .open(&marker)
                .unwrap()
                .set_modified(written)
                .unwrap();
            folder
        }
    }

    #[tokio::test]
    async fn a_folder_without_a_marker_and_a_loose_file_are_left_alone() {
        let rig = Rig::open().await;
        let foreign = rig.folder("hand-made");
        let loose = rig.root.join("readme.txt");
        std::fs::write(&loose, b"notes").unwrap();

        let removed = rig.sweep().await.unwrap();

        assert_eq!(removed, 0);
        assert!(foreign.join("alert.css").exists());
        assert!(loose.exists());
    }

    #[tokio::test]
    async fn a_marked_folder_whose_row_exists_loses_only_the_marker() {
        let rig = Rig::open().await;
        let id = rig.stored_overlay().await;
        let folder = rig.marked_folder(&id, LAPSED);

        let removed = rig.sweep().await.unwrap();

        assert_eq!(removed, 0);
        assert!(folder.join("alert.css").exists());
        assert!(!folder.join(PENDING_IMPORT_MARKER).exists());
    }

    #[tokio::test]
    async fn an_old_marked_folder_without_a_row_is_removed() {
        let rig = Rig::open().await;
        let folder = rig.marked_folder("abandoned", LAPSED);

        let removed = rig.sweep().await.unwrap();

        assert_eq!(removed, 1);
        assert!(!folder.exists());
    }

    #[tokio::test]
    async fn a_fresh_marked_folder_without_a_row_is_kept_marked() {
        let rig = Rig::open().await;
        let folder = rig.marked_folder("in-progress", RECENT);

        let removed = rig.sweep().await.unwrap();

        assert_eq!(removed, 0);
        assert!(folder.join("alert.css").exists());
        assert!(folder.join(PENDING_IMPORT_MARKER).exists());
    }

    #[tokio::test]
    async fn a_failed_row_lookup_fails_the_sweep_and_removes_nothing() {
        let rig = Rig::open().await;
        let folder = rig.marked_folder("unknown", LAPSED);
        rig.backend.shutdown().await;

        let swept = rig.sweep().await;

        assert!(swept.is_err());
        assert!(folder.join("alert.css").exists());
    }

    #[tokio::test]
    async fn a_root_that_does_not_exist_yet_removes_nothing() {
        let rig = Rig::open().await;
        std::fs::remove_dir(&rig.root).unwrap();

        assert_eq!(rig.sweep().await.unwrap(), 0);
    }
}
