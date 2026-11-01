use std::collections::BTreeSet;
use std::io;
use std::path::Path;
use std::time::{Duration, SystemTime};

use forge_storage::{OverlayId, OverlayRepo};

use crate::documents::overlay_files::{
    PENDING_IMPORT_MARKER, SET_ASIDE_EXTENSION, SET_ASIDE_SESSION_SEPARATOR,
    UNCOMMITTED_IMPORT_PREFIX,
};
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SetAsideSweep {
    pub restored: usize,
    pub removed: usize,
}

struct SetAside<'a> {
    replaced: &'a str,
    session: Option<&'a str>,
}

fn set_aside_of(name: &str) -> Option<SetAside<'_>> {
    let (replaced, suffix) = name.rsplit_once(&format!(".{SET_ASIDE_EXTENSION}-"))?;
    if replaced.is_empty() || suffix.is_empty() {
        return None;
    }
    let session = suffix
        .split_once(SET_ASIDE_SESSION_SEPARATOR)
        .map(|(session, _)| session);
    Some(SetAside { replaced, session })
}

async fn uncommitted_sessions(overlay_root: &Path) -> Result<BTreeSet<String>, TransferError> {
    let mut sessions = BTreeSet::new();
    let mut entries = match tokio::fs::read_dir(overlay_root).await {
        Ok(entries) => entries,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(sessions),
        Err(e) => return Err(e.into()),
    };
    while let Some(entry) = entries.next_entry().await? {
        let file_name = entry.file_name();
        if let Some(session) = file_name
            .to_str()
            .and_then(|name| name.strip_prefix(UNCOMMITTED_IMPORT_PREFIX))
        {
            sessions.insert(session.to_owned());
        }
    }
    Ok(sessions)
}

pub async fn sweep_set_aside_overlay_files(
    overlay_root: &Path,
) -> Result<SetAsideSweep, TransferError> {
    let uncommitted = uncommitted_sessions(overlay_root).await?;
    let mut sweep = SetAsideSweep::default();
    let mut pending = vec![overlay_root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let mut entries = match tokio::fs::read_dir(&directory).await {
            Ok(entries) => entries,
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e.into()),
        };
        while let Some(entry) = entries.next_entry().await? {
            if entry.file_type().await?.is_dir() {
                pending.push(entry.path());
                continue;
            }
            let file_name = entry.file_name();
            let Some(aside) = file_name.to_str().and_then(set_aside_of) else {
                continue;
            };
            let replaced = directory.join(aside.replaced);
            let interrupted = aside
                .session
                .is_some_and(|session| uncommitted.contains(session));
            let keep_replacement = !interrupted && tokio::fs::try_exists(&replaced).await?;
            if keep_replacement {
                tokio::fs::remove_file(entry.path()).await?;
                sweep.removed += 1;
            } else {
                tokio::fs::rename(entry.path(), &replaced).await?;
                sweep.restored += 1;
            }
        }
    }
    for session in &uncommitted {
        let marker = overlay_root.join(format!("{UNCOMMITTED_IMPORT_PREFIX}{session}"));
        tokio::fs::remove_file(&marker).await?;
    }
    Ok(sweep)
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

    fn aside_name(file: &str, session: &str) -> String {
        format!("{file}.{SET_ASIDE_EXTENSION}-{session}{SET_ASIDE_SESSION_SEPARATOR}x1y2z3")
    }

    fn uncommitted_marker(rig: &Rig, session: &str) -> PathBuf {
        let marker = rig
            .root
            .join(format!("{UNCOMMITTED_IMPORT_PREFIX}{session}"));
        std::fs::write(&marker, []).unwrap();
        marker
    }

    #[tokio::test]
    async fn a_committed_import_drops_the_old_file_and_keeps_the_new_one() {
        let rig = Rig::open().await;
        let folder = rig.folder("overlay");
        let aside = folder.join(aside_name("alert.css", "abc123"));
        std::fs::write(&aside, b"old").unwrap();

        let swept = sweep_set_aside_overlay_files(&rig.root).await.unwrap();

        assert_eq!(
            swept,
            SetAsideSweep {
                restored: 0,
                removed: 1
            }
        );
        assert_eq!(std::fs::read(folder.join("alert.css")).unwrap(), b"body {}");
        assert!(!aside.exists());
    }

    #[tokio::test]
    async fn an_interrupted_import_puts_the_old_file_over_the_partial_one() {
        let rig = Rig::open().await;
        let folder = rig.folder("overlay");
        let aside = folder.join(aside_name("alert.css", "abc123"));
        std::fs::write(&aside, b"old").unwrap();
        let marker = uncommitted_marker(&rig, "abc123");

        let swept = sweep_set_aside_overlay_files(&rig.root).await.unwrap();

        assert_eq!(
            swept,
            SetAsideSweep {
                restored: 1,
                removed: 0
            }
        );
        assert_eq!(std::fs::read(folder.join("alert.css")).unwrap(), b"old");
        assert!(!aside.exists());
        assert!(!marker.exists());
    }

    #[tokio::test]
    async fn an_interrupted_import_restores_a_file_whose_replacement_never_landed() {
        let rig = Rig::open().await;
        let folder = rig.folder("overlay");
        let aside = folder.join(aside_name("theme.css", "abc123"));
        std::fs::write(&aside, b"old").unwrap();
        uncommitted_marker(&rig, "abc123");

        let swept = sweep_set_aside_overlay_files(&rig.root).await.unwrap();

        assert_eq!(swept.restored, 1);
        assert_eq!(std::fs::read(folder.join("theme.css")).unwrap(), b"old");
    }

    #[tokio::test]
    async fn another_sessions_marker_does_not_roll_back_a_committed_import() {
        let rig = Rig::open().await;
        let folder = rig.folder("overlay");
        std::fs::write(folder.join(aside_name("alert.css", "abc123")), b"old").unwrap();
        let other = uncommitted_marker(&rig, "ffffff");

        let swept = sweep_set_aside_overlay_files(&rig.root).await.unwrap();

        assert_eq!(
            swept,
            SetAsideSweep {
                restored: 0,
                removed: 1
            }
        );
        assert_eq!(std::fs::read(folder.join("alert.css")).unwrap(), b"body {}");
        assert!(!other.exists());
    }

    #[tokio::test]
    async fn nested_set_aside_files_are_found_and_unrelated_files_are_left() {
        let rig = Rig::open().await;
        let folder = rig.folder("overlay");
        let nested = folder.join("img");
        std::fs::create_dir(&nested).unwrap();
        std::fs::write(nested.join(aside_name("logo.png", "abc123")), b"old").unwrap();
        let unrelated = folder.join("notes.txt");
        std::fs::write(&unrelated, b"keep").unwrap();
        uncommitted_marker(&rig, "abc123");

        let swept = sweep_set_aside_overlay_files(&rig.root).await.unwrap();

        assert_eq!(swept.restored, 1);
        assert_eq!(std::fs::read(nested.join("logo.png")).unwrap(), b"old");
        assert!(unrelated.exists());
    }

    #[tokio::test]
    async fn a_missing_root_sweeps_nothing() {
        let rig = Rig::open().await;
        std::fs::remove_dir(&rig.root).unwrap();

        let swept = sweep_set_aside_overlay_files(&rig.root).await.unwrap();

        assert_eq!(swept, SetAsideSweep::default());
    }
}
