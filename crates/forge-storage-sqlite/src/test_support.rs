#![expect(clippy::expect_used, reason = "test fixtures")]
use std::ops::Deref;
use std::path::Path;

use tempfile::TempDir;

use crate::SqliteBackend;

pub const TEST_KEY: [u8; 32] = [0xab; 32];

const DATABASE_FILE: &str = "forge.db";
const MEDIA_DIR: &str = "media";
const IN_MEMORY_URL: &str = ":memory:";

pub struct Sandboxed<T> {
    inner: T,
    _temp_root: TempDir,
}

impl<T> Sandboxed<T> {
    pub fn new(inner: T, temp_root: TempDir) -> Self {
        Self {
            inner,
            _temp_root: temp_root,
        }
    }

    pub fn map<U>(self, wrap: impl FnOnce(T) -> U) -> Sandboxed<U> {
        Sandboxed {
            inner: wrap(self.inner),
            _temp_root: self._temp_root,
        }
    }
}

impl<T> Deref for Sandboxed<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.inner
    }
}

pub async fn open_existing(data_dir: &Path) -> SqliteBackend {
    let key_file = data_dir.join(forge_paths::CREDENTIALS_KEY_FILE_NAME);
    let key =
        crate::codec::crypto::read_key_file(&key_file).expect("a readable credential key file");
    let url = format!(
        "sqlite://{}?mode=rw",
        data_dir.join(forge_storage::DATABASE_FILE_NAME).display()
    );
    SqliteBackend::open_for_test(&url, key, data_dir.join(forge_paths::MEDIA_DIR_NAME), None)
        .await
        .expect("a backend over the existing data directory")
}

pub async fn sandboxed_backend_at(url: &str, key: [u8; 32]) -> Sandboxed<SqliteBackend> {
    let media_root = tempfile::tempdir().expect("a temporary media root");
    let backend = SqliteBackend::open_for_test(url, key, media_root.path().to_owned(), None)
        .await
        .expect("a backend");
    Sandboxed::new(backend, media_root)
}

pub async fn sandboxed_backend() -> Sandboxed<SqliteBackend> {
    sandboxed_backend_at(IN_MEMORY_URL, TEST_KEY).await
}

pub async fn sandboxed_file_backend(key: [u8; 32]) -> Sandboxed<SqliteBackend> {
    let root = tempfile::tempdir().expect("a temporary database directory");
    let url = format!("sqlite://{}", root.path().join(DATABASE_FILE).display());
    let backend = SqliteBackend::open_for_test(&url, key, root.path().join(MEDIA_DIR), None)
        .await
        .expect("a file-backed backend");
    Sandboxed::new(backend, root)
}
