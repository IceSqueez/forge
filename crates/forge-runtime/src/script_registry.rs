use std::collections::HashMap;
use std::sync::Arc;

use forge_events::{Event, EventSource};
use forge_storage::{ScriptRecord, ScriptRepo};
use forge_types::ScriptId;
use tokio::sync::RwLock;
use tracing::warn;

use crate::EventBus;

pub struct CompiledScript {
    pub record: ScriptRecord,
}

pub struct ScriptRegistry {
    inner: Arc<RwLock<HashMap<ScriptId, Arc<CompiledScript>>>>,
}

#[derive(Debug, thiserror::Error)]
pub enum ScriptRegistryError {
    #[error("storage: {0}")]
    Storage(String),
    #[error("compile: {0}")]
    Compile(String),
}

impl ScriptRegistry {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn load_all(&self, repo: &dyn ScriptRepo) -> Result<(), ScriptRegistryError> {
        let records = repo
            .list_enabled()
            .await
            .map_err(|e| ScriptRegistryError::Storage(e.to_string()))?;

        let mut new_map = HashMap::new();
        for record in records {
            if let Err(e) = forge_script::validate_syntax(&record.body) {
                warn!(
                    script_id = %record.id,
                    script_name = %record.name,
                    error = ?e,
                    "script has invalid syntax, skipping"
                );
                continue;
            }
            new_map.insert(record.id, Arc::new(CompiledScript { record }));
        }

        let mut guard = self.inner.write().await;
        *guard = new_map;
        Ok(())
    }

    /// Hot-reloads a single script after a save.
    ///
    /// Validates syntax before acquiring the write lock. On success the registry is
    /// updated and a `script.reloaded` event is published. The lock is released before
    /// `bus.publish` so no lock is held across an async operation.
    pub async fn reload(
        &self,
        record: ScriptRecord,
        bus: &EventBus,
    ) -> Result<(), ScriptRegistryError> {
        forge_script::validate_syntax(&record.body)
            .map_err(|e| ScriptRegistryError::Compile(e.to_string()))?;

        let script_id = record.id;
        let name = record.name.clone();

        {
            let mut guard = self.inner.write().await;
            guard.insert(script_id, Arc::new(CompiledScript { record }));
        }

        bus.publish(Event::new(
            EventSource::Core,
            "script.reloaded",
            serde_json::json!({"script_id": script_id.to_string(), "name": name}),
        ));

        Ok(())
    }

    pub async fn get(&self, id: ScriptId) -> Option<Arc<CompiledScript>> {
        let guard = self.inner.read().await;
        guard.get(&id).cloned()
    }

    pub async fn get_by_name(&self, name: &str) -> Option<Arc<CompiledScript>> {
        let guard = self.inner.read().await;
        guard.values().find(|c| c.record.name == name).cloned()
    }

    pub async fn remove(&self, id: ScriptId) -> bool {
        let mut guard = self.inner.write().await;
        guard.remove(&id).is_some()
    }

    pub async fn count(&self) -> usize {
        let guard = self.inner.read().await;
        guard.len()
    }
}

impl Default for ScriptRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::EventBus;
    use forge_storage::ScriptRepo;
    use forge_storage_sqlite::SqliteBackend;
    use forge_types::{ScriptContract, ScriptId};
    use std::sync::Arc;
    use time::OffsetDateTime;

    async fn make_dp() -> Arc<SqliteBackend> {
        Arc::new(
            SqliteBackend::open_with_key(":memory:", [0xab; 32])
                .await
                .unwrap(),
        )
    }

    fn make_record(name: &str, body: &str, enabled: bool) -> ScriptRecord {
        let ts = OffsetDateTime::now_utc();
        ScriptRecord {
            id: ScriptId::new(),
            name: name.to_owned(),
            body: body.to_owned(),
            contract: ScriptContract::default(),
            body_hash: "test".to_owned(),
            enabled,
            created_at: ts,
            last_modified: ts,
        }
    }

    #[tokio::test]
    async fn count_is_zero_on_new() {
        let registry = ScriptRegistry::new();
        assert_eq!(registry.count().await, 0);
    }

    #[tokio::test]
    async fn load_all_includes_only_enabled_scripts() {
        let dp = make_dp().await;
        let r1 = make_record("greet", "let x = 1;", true);
        let r2 = make_record("farewell", "let y = 2;", true);
        let r3 = make_record("disabled_script", "let z = 3;", false);

        ScriptRepo::save(dp.as_ref(), r1).await.unwrap();
        ScriptRepo::save(dp.as_ref(), r2).await.unwrap();
        ScriptRepo::save(dp.as_ref(), r3).await.unwrap();

        let registry = ScriptRegistry::new();
        registry.load_all(dp.as_ref()).await.unwrap();

        assert_eq!(registry.count().await, 2);
    }

    #[tokio::test]
    async fn load_all_skips_scripts_with_invalid_syntax() {
        let dp = make_dp().await;
        let valid = make_record("valid_script", "let x = 1;", true);
        let invalid = make_record("broken_script", "@@@not valid rhai@@@", true);

        ScriptRepo::save(dp.as_ref(), valid).await.unwrap();
        ScriptRepo::save(dp.as_ref(), invalid).await.unwrap();

        let registry = ScriptRegistry::new();
        registry.load_all(dp.as_ref()).await.unwrap();

        assert_eq!(registry.count().await, 1);
    }

    #[tokio::test]
    async fn get_by_name_finds_loaded_script() {
        let dp = make_dp().await;
        let record = make_record("my_script", "let x = 42;", true);
        ScriptRepo::save(dp.as_ref(), record).await.unwrap();

        let registry = ScriptRegistry::new();
        registry.load_all(dp.as_ref()).await.unwrap();

        let found = registry.get_by_name("my_script").await;
        assert!(found.is_some());
        assert_eq!(found.unwrap().record.name, "my_script");
    }

    #[tokio::test]
    async fn get_by_name_returns_none_for_unknown() {
        let dp = make_dp().await;
        let registry = ScriptRegistry::new();
        registry.load_all(dp.as_ref()).await.unwrap();

        let found = registry.get_by_name("nonexistent").await;
        assert!(found.is_none());
    }

    #[tokio::test]
    async fn reload_adds_new_entry_and_emits_event() {
        let registry = ScriptRegistry::new();
        let bus = EventBus::new();
        let mut sub = bus.subscribe();

        let record = make_record("fresh_script", "let x = 1;", true);
        let expected_name = record.name.clone();

        registry.reload(record, &bus).await.unwrap();

        assert_eq!(registry.count().await, 1);

        let event = tokio::time::timeout(std::time::Duration::from_millis(200), sub.recv())
            .await
            .unwrap()
            .unwrap();

        assert_eq!(event.kind, "script.reloaded");
        assert_eq!(event.payload["name"].as_str(), Some(expected_name.as_str()));
    }

    #[tokio::test]
    async fn reload_replaces_existing_entry() {
        let registry = ScriptRegistry::new();
        let bus = EventBus::new();
        let id = ScriptId::new();
        let ts = OffsetDateTime::now_utc();

        let original = ScriptRecord {
            id,
            name: "editable".to_owned(),
            body: "let x = 1;".to_owned(),
            contract: ScriptContract::default(),
            body_hash: "v1".to_owned(),
            enabled: true,
            created_at: ts,
            last_modified: ts,
        };
        registry.reload(original, &bus).await.unwrap();
        assert_eq!(registry.count().await, 1);

        let updated = ScriptRecord {
            id,
            name: "editable".to_owned(),
            body: "let x = 2;".to_owned(),
            contract: ScriptContract::default(),
            body_hash: "v2".to_owned(),
            enabled: true,
            created_at: ts,
            last_modified: ts,
        };
        registry.reload(updated, &bus).await.unwrap();

        assert_eq!(registry.count().await, 1);
        let found = registry.get_by_name("editable").await.unwrap();
        assert_eq!(found.record.body, "let x = 2;");
    }

    #[tokio::test]
    async fn remove_returns_true_for_existing() {
        let registry = ScriptRegistry::new();
        let bus = EventBus::new();
        let record = make_record("removable", "1 + 1;", true);
        let id = record.id;

        registry.reload(record, &bus).await.unwrap();
        assert!(registry.remove(id).await);
        assert_eq!(registry.count().await, 0);
    }

    #[tokio::test]
    async fn remove_returns_false_for_missing() {
        let registry = ScriptRegistry::new();
        let ghost = ScriptId::new();
        assert!(!registry.remove(ghost).await);
    }

    #[tokio::test]
    async fn concurrent_reads_and_reload_do_not_deadlock() {
        let registry = Arc::new(ScriptRegistry::new());
        let bus = EventBus::new();

        let seed = make_record("concurrent_script", "let v = 99;", true);
        registry.reload(seed, &bus).await.unwrap();

        let readers: Vec<_> = (0..10)
            .map(|_| {
                let reg = Arc::clone(&registry);
                tokio::spawn(async move {
                    for _ in 0..50 {
                        let _ = reg.get_by_name("concurrent_script").await;
                    }
                })
            })
            .collect();

        let writer = {
            let reg = Arc::clone(&registry);
            let bus_clone = EventBus::new();
            tokio::spawn(async move {
                for i in 0..10u32 {
                    let r = make_record("concurrent_script", &format!("let v = {i};"), true);
                    reg.reload(r, &bus_clone).await.unwrap();
                }
            })
        };

        for handle in readers {
            handle.await.unwrap();
        }
        writer.await.unwrap();

        let found = registry.get_by_name("concurrent_script").await;
        assert!(
            found.is_some(),
            "script must still be present after concurrent access"
        );
    }

    #[tokio::test]
    async fn reload_rejects_invalid_syntax() {
        let registry = ScriptRegistry::new();
        let bus = EventBus::new();
        let mut sub = bus.subscribe();

        let bad = make_record("broken", "@@@invalid@@@", true);
        let result = registry.reload(bad, &bus).await;
        assert!(
            matches!(result, Err(ScriptRegistryError::Compile(_))),
            "invalid syntax must return Compile error"
        );
        assert_eq!(registry.count().await, 0);

        let event_result =
            tokio::time::timeout(std::time::Duration::from_millis(50), sub.recv()).await;
        assert!(
            event_result.is_err(),
            "no event should be published on compile failure"
        );
    }

    #[tokio::test]
    async fn get_returns_some_for_known_id() {
        let registry = ScriptRegistry::new();
        let bus = EventBus::new();
        let record = make_record("by_id_script", "let a = 5;", true);
        let id = record.id;

        registry.reload(record, &bus).await.unwrap();

        let found = registry.get(id).await;
        assert!(found.is_some());
        assert_eq!(found.unwrap().record.id, id);
    }

    #[tokio::test]
    async fn get_returns_none_for_unknown_id() {
        let registry = ScriptRegistry::new();
        let ghost = ScriptId::new();
        assert!(registry.get(ghost).await.is_none());
    }
}
