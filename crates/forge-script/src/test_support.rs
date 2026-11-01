pub(crate) use forge_storage_sqlite::test_support::{Sandboxed, sandboxed_backend};

pub(crate) struct ScriptedClock {
    now: std::sync::Mutex<std::time::Instant>,
    per_read: std::time::Duration,
}

impl ScriptedClock {
    pub(crate) fn frozen() -> std::sync::Arc<Self> {
        Self::ticking(std::time::Duration::ZERO)
    }

    pub(crate) fn ticking(per_read: std::time::Duration) -> std::sync::Arc<Self> {
        std::sync::Arc::new(Self {
            now: std::sync::Mutex::new(std::time::Instant::now()),
            per_read,
        })
    }

    pub(crate) fn advance(&self, by: std::time::Duration) {
        *self.now.lock().unwrap() += by;
    }

    pub(crate) fn peek(&self) -> std::time::Instant {
        *self.now.lock().unwrap()
    }

    pub(crate) fn sandbox(self: &std::sync::Arc<Self>) -> crate::wall_budget::SandboxClock {
        crate::wall_budget::SandboxClock::new(self.clone())
    }
}

impl forge_types::MonotonicClock for ScriptedClock {
    fn now(&self) -> std::time::Instant {
        let read = self.peek();
        self.advance(self.per_read);
        read
    }

    fn sleep(&self, duration: std::time::Duration) {
        self.advance(duration);
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
    repo: std::sync::Arc<dyn forge_storage::GlobalsRepo>,
) -> std::sync::Arc<dyn crate::ScriptGlobals> {
    std::sync::Arc::new(RepoGlobals::new(repo))
}

#[async_trait::async_trait]
impl crate::ScriptGlobals for RepoGlobals {
    async fn get(&self, key: &str) -> Result<Option<forge_types::Variant>, crate::ScriptError> {
        Ok(self.repo.get(key).await?)
    }

    async fn set(
        &self,
        key: &str,
        value: forge_types::Variant,
        persisted: bool,
        caused_by: forge_types::EventId,
    ) -> Result<Option<forge_types::Variant>, crate::ScriptError> {
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
    ) -> Result<crate::GlobalIncrement, crate::ScriptError> {
        self.record(caused_by);
        let previous = self.repo.get(key).await?;
        let current = self.repo.incr(key, delta).await?;
        Ok(crate::GlobalIncrement { previous, current })
    }

    async fn delete(
        &self,
        key: &str,
        caused_by: forge_types::EventId,
    ) -> Result<crate::GlobalDeletion, crate::ScriptError> {
        self.record(caused_by);
        let previous = self.repo.get(key).await?;
        if self.repo.delete(key).await? {
            Ok(crate::GlobalDeletion::Deleted { previous })
        } else {
            Ok(crate::GlobalDeletion::Missing)
        }
    }
}

const TEST_API_BUDGET: std::time::Duration = std::time::Duration::from_secs(30);

pub(crate) fn api_publishing_to(
    publisher: std::sync::Arc<dyn forge_events::EventPublisher>,
    globals: std::sync::Arc<dyn crate::ScriptGlobals>,
    caused_by: forge_types::EventId,
) -> crate::ForgeApi {
    crate::ForgeApi::new(
        publisher,
        globals,
        caused_by,
        std::time::Instant::now() + TEST_API_BUDGET,
    )
}

pub(crate) fn quiet_api_until(
    globals: std::sync::Arc<dyn crate::ScriptGlobals>,
    deadline: std::time::Instant,
) -> crate::ForgeApi {
    crate::ForgeApi::new(
        std::sync::Arc::new(forge_events::test_support::NullPublisher),
        globals,
        forge_types::EventId::new(),
        deadline,
    )
}

pub(crate) fn quiet_api(globals: std::sync::Arc<dyn crate::ScriptGlobals>) -> crate::ForgeApi {
    quiet_api_until(globals, std::time::Instant::now() + TEST_API_BUDGET)
}

pub(crate) fn host_over(
    backend: &forge_storage_sqlite::SqliteBackend,
    bus: std::sync::Arc<dyn forge_events::EventPublisher>,
    capabilities: crate::runner::ScriptCapabilities,
) -> crate::runner::ScriptHost {
    use forge_storage::DataProvider as _;

    crate::runner::ScriptHost {
        globals: repo_globals(backend.globals_repo()),
        settings: backend.settings_repo(),
        bus,
        capabilities,
    }
}
