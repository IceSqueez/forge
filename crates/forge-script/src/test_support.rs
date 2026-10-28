use std::ops::Deref;

use forge_storage_sqlite::SqliteBackend;
use tempfile::TempDir;

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
    let backend = SqliteBackend::open_for_test(":memory:", key, media_root.path().to_owned(), None)
        .await
        .expect("an in-memory backend");
    Sandboxed {
        inner: backend,
        _media_root: media_root,
    }
}

pub(crate) struct RepoGlobals {
    repo: std::sync::Arc<dyn forge_storage::GlobalsRepo>,
    causes: std::sync::Mutex<Vec<forge_types::EventId>>,
}

impl RepoGlobals {
    pub(crate) fn new(repo: std::sync::Arc<dyn forge_storage::GlobalsRepo>) -> Self {
        Self {
            repo,
            causes: std::sync::Mutex::new(Vec::new()),
        }
    }

    pub(crate) fn causes(&self) -> Vec<forge_types::EventId> {
        self.causes.lock().unwrap().clone()
    }

    fn record(&self, caused_by: forge_types::EventId) {
        self.causes.lock().unwrap().push(caused_by);
    }
}

pub(crate) fn repo_globals(
    repo: std::sync::Arc<impl forge_storage::GlobalsRepo + 'static>,
) -> std::sync::Arc<dyn crate::ScriptGlobals> {
    std::sync::Arc::new(RepoGlobals::new(repo))
}

fn port_error(error: forge_storage::StorageError) -> crate::ScriptGlobalsError {
    crate::ScriptGlobalsError(error.to_string())
}

#[async_trait::async_trait]
impl crate::ScriptGlobals for RepoGlobals {
    async fn get(
        &self,
        key: &str,
    ) -> Result<Option<forge_types::Variant>, crate::ScriptGlobalsError> {
        self.repo.get(key).await.map_err(port_error)
    }

    async fn set(
        &self,
        key: &str,
        value: forge_types::Variant,
        persisted: bool,
        caused_by: forge_types::EventId,
    ) -> Result<Option<forge_types::Variant>, crate::ScriptGlobalsError> {
        self.record(caused_by);
        let previous = self.repo.get(key).await.map_err(port_error)?;
        self.repo
            .set(key, value, persisted)
            .await
            .map_err(port_error)?;
        Ok(previous)
    }

    async fn increment(
        &self,
        key: &str,
        delta: i64,
        caused_by: forge_types::EventId,
    ) -> Result<crate::GlobalIncrement, crate::ScriptGlobalsError> {
        self.record(caused_by);
        let previous = self.repo.get(key).await.map_err(port_error)?;
        let current = self.repo.incr(key, delta).await.map_err(port_error)?;
        Ok(crate::GlobalIncrement { previous, current })
    }

    async fn delete(
        &self,
        key: &str,
        caused_by: forge_types::EventId,
    ) -> Result<crate::GlobalDeletion, crate::ScriptGlobalsError> {
        self.record(caused_by);
        let previous = self.repo.get(key).await.map_err(port_error)?;
        if self.repo.delete(key).await.map_err(port_error)? {
            Ok(crate::GlobalDeletion::Deleted { previous })
        } else {
            Ok(crate::GlobalDeletion::Missing)
        }
    }
}
