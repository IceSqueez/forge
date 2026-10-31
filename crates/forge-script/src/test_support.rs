pub(crate) use forge_storage_sqlite::test_support::{Sandboxed, sandboxed_backend};

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

#[async_trait::async_trait]
impl crate::ScriptGlobals for RepoGlobals {
    async fn get(
        &self,
        key: &str,
    ) -> Result<Option<forge_types::Variant>, crate::ScriptGlobalsError> {
        Ok(self.repo.get(key).await?)
    }

    async fn set(
        &self,
        key: &str,
        value: forge_types::Variant,
        persisted: bool,
        caused_by: forge_types::EventId,
    ) -> Result<Option<forge_types::Variant>, crate::ScriptGlobalsError> {
        self.record(caused_by);
        let previous = self.repo.get(key).await?;
        self.repo.set(key, value, persisted).await?;
        Ok(previous)
    }

    async fn increment(
        &self,
        key: &str,
        delta: i64,
        caused_by: forge_types::EventId,
    ) -> Result<crate::GlobalIncrement, crate::ScriptGlobalsError> {
        self.record(caused_by);
        let previous = self.repo.get(key).await?;
        let current = self.repo.incr(key, delta).await?;
        Ok(crate::GlobalIncrement { previous, current })
    }

    async fn delete(
        &self,
        key: &str,
        caused_by: forge_types::EventId,
    ) -> Result<crate::GlobalDeletion, crate::ScriptGlobalsError> {
        self.record(caused_by);
        let previous = self.repo.get(key).await?;
        if self.repo.delete(key).await? {
            Ok(crate::GlobalDeletion::Deleted { previous })
        } else {
            Ok(crate::GlobalDeletion::Missing)
        }
    }
}
