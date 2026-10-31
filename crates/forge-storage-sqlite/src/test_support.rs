use std::ops::Deref;

use tempfile::TempDir;

use crate::SqliteBackend;

const DATABASE_FILE: &str = "forge.db";
const MEDIA_DIR: &str = "media";

pub(crate) struct Sandboxed<T> {
    inner: T,
    _temp_root: TempDir,
}

impl<T> Deref for Sandboxed<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.inner
    }
}

pub(crate) async fn sandboxed_backend(key: [u8; 32]) -> Sandboxed<SqliteBackend> {
    let root = tempfile::tempdir().expect("a temporary database directory");
    let url = format!("sqlite://{}", root.path().join(DATABASE_FILE).display());
    let backend = SqliteBackend::open_for_test(&url, key, root.path().join(MEDIA_DIR), None)
        .await
        .expect("a file-backed backend");
    Sandboxed {
        inner: backend,
        _temp_root: root,
    }
}
