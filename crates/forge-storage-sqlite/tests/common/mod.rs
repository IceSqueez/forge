#![allow(dead_code, clippy::expect_used)]

use std::ops::Deref;

use forge_storage_sqlite::SqliteBackend;
use tempfile::TempDir;

pub const TEST_KEY: [u8; 32] = [0xab; 32];

pub struct Sandboxed<T> {
    inner: T,
    _media_root: TempDir,
}

impl<T> Sandboxed<T> {
    pub fn map<U>(self, wrap: impl FnOnce(T) -> U) -> Sandboxed<U> {
        Sandboxed {
            inner: wrap(self.inner),
            _media_root: self._media_root,
        }
    }
}

impl<T> Deref for Sandboxed<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.inner
    }
}

pub async fn sandboxed_backend(url: &str, key: [u8; 32]) -> Sandboxed<SqliteBackend> {
    let media_root = tempfile::tempdir().expect("a temporary media root");
    let backend = SqliteBackend::open_for_test(url, key, media_root.path().to_owned(), None)
        .await
        .expect("a backend");
    Sandboxed {
        inner: backend,
        _media_root: media_root,
    }
}
