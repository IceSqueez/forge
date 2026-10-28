use std::sync::Arc;

use async_trait::async_trait;
use forge_events::{Event, EventPublisher, EventSource};
use forge_script::{GlobalDeletion, GlobalIncrement, ScriptGlobals, ScriptGlobalsError};
use forge_storage::{GlobalsRepo, StorageError};
use forge_types::{EventId, Variant};

pub(crate) struct GlobalMutation<'a> {
    pub globals: &'a dyn GlobalsRepo,
    pub publisher: &'a dyn EventPublisher,
    pub source: EventSource,
    pub caused_by: EventId,
}

impl GlobalMutation<'_> {
    pub async fn set(
        &self,
        key: &str,
        value: Variant,
        persisted: bool,
    ) -> Result<Option<Variant>, StorageError> {
        let previous = self.globals.get(key).await.ok().flatten();
        let new_value_json = value.to_plain_json();
        let prev_value_json = previous
            .as_ref()
            .map(Variant::to_plain_json)
            .unwrap_or(serde_json::Value::Null);
        self.globals.set(key, value, persisted).await?;
        self.publish(
            "global.set",
            serde_json::json!({
                "key": key,
                "new_value": new_value_json,
                "prev_value": prev_value_json,
            }),
        );
        Ok(previous)
    }

    pub async fn increment(&self, key: &str, delta: i64) -> Result<GlobalIncrement, StorageError> {
        let previous = self.globals.get(key).await.ok().flatten();
        let current = self.globals.incr(key, delta).await?;
        let new_value_json = match &current {
            Variant::Int(i) => serde_json::Value::from(*i),
            _ => serde_json::Value::String(current.to_string()),
        };
        self.publish(
            "global.incremented",
            serde_json::json!({
                "key": key,
                "delta": delta,
                "new_value": new_value_json,
            }),
        );
        Ok(GlobalIncrement { previous, current })
    }

    pub async fn delete(&self, key: &str) -> Result<GlobalDeletion, StorageError> {
        let previous = self.globals.get(key).await.ok().flatten();
        if !self.globals.delete(key).await? {
            return Ok(GlobalDeletion::Missing);
        }
        self.publish("global.deleted", serde_json::json!({ "key": key }));
        Ok(GlobalDeletion::Deleted { previous })
    }

    fn publish(&self, kind: &str, payload: serde_json::Value) {
        self.publisher
            .publish(Event::caused_by(self.source, kind, payload, self.caused_by));
    }
}

pub struct RuntimeScriptGlobals {
    globals: Arc<dyn GlobalsRepo>,
    publisher: Arc<dyn EventPublisher>,
}

impl RuntimeScriptGlobals {
    pub fn new(globals: Arc<dyn GlobalsRepo>, publisher: Arc<dyn EventPublisher>) -> Self {
        Self { globals, publisher }
    }

    fn mutation(&self, caused_by: EventId) -> GlobalMutation<'_> {
        GlobalMutation {
            globals: self.globals.as_ref(),
            publisher: self.publisher.as_ref(),
            source: EventSource::Rhai,
            caused_by,
        }
    }
}

fn port_error(error: StorageError) -> ScriptGlobalsError {
    ScriptGlobalsError(error.to_string())
}

#[async_trait]
impl ScriptGlobals for RuntimeScriptGlobals {
    async fn get(&self, key: &str) -> Result<Option<Variant>, ScriptGlobalsError> {
        self.globals.get(key).await.map_err(port_error)
    }

    async fn set(
        &self,
        key: &str,
        value: Variant,
        persisted: bool,
        caused_by: EventId,
    ) -> Result<Option<Variant>, ScriptGlobalsError> {
        self.mutation(caused_by)
            .set(key, value, persisted)
            .await
            .map_err(port_error)
    }

    async fn increment(
        &self,
        key: &str,
        delta: i64,
        caused_by: EventId,
    ) -> Result<GlobalIncrement, ScriptGlobalsError> {
        self.mutation(caused_by)
            .increment(key, delta)
            .await
            .map_err(port_error)
    }

    async fn delete(
        &self,
        key: &str,
        caused_by: EventId,
    ) -> Result<GlobalDeletion, ScriptGlobalsError> {
        self.mutation(caused_by)
            .delete(key)
            .await
            .map_err(port_error)
    }
}
