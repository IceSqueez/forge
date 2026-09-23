#![allow(clippy::expect_used)]

// Why: the plain `SqliteBackend` constructors hand the media repo the real user media
// directory, so a test that reaches media would write into the maintainer's own data.
// Every test backend in this crate is opened through here instead, with a media root
// that lives and dies with the test.

use std::ops::Deref;

use forge_storage_sqlite::SqliteBackend;
use tempfile::TempDir;

/// Keeps a temporary media root alive for as long as the backend built on it.
pub(crate) struct Sandboxed<T> {
    inner: T,
    _media_root: TempDir,
}

impl<T> Sandboxed<T> {
    pub(crate) fn map<U>(self, wrap: impl FnOnce(T) -> U) -> Sandboxed<U> {
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

pub(crate) async fn sandboxed_backend(key: [u8; 32]) -> Sandboxed<SqliteBackend> {
    let media_root = tempfile::tempdir().expect("a temporary media root");
    let backend =
        SqliteBackend::open_with_key_and_media_root(":memory:", key, media_root.path().to_owned())
            .await
            .expect("an in-memory backend");
    Sandboxed {
        inner: backend,
        _media_root: media_root,
    }
}
