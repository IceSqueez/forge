use std::sync::Arc;

use async_trait::async_trait;
use forge_events::{Event, EventSource};
use forge_registry::{
    FormField, RegistryError, RunContext, StepTimer, SubActionCategory, SubActionConfigExt,
    SubActionRunner,
};
use forge_storage::GlobalsRepo;
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant};

pub struct CoreGlobalsDeleteRunner {
    globals: Arc<dyn GlobalsRepo>,
}

impl CoreGlobalsDeleteRunner {
    pub fn new(globals: Arc<dyn GlobalsRepo>) -> Self {
        Self { globals }
    }
}

#[async_trait]
impl SubActionRunner for CoreGlobalsDeleteRunner {
    fn id(&self) -> &str {
        "core.globals.delete"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Globals
    }

    fn label(&self) -> &str {
        "Delete Global"
    }

    fn summary(&self) -> &str {
        "Remove a global variable"
    }

    fn search_text(&self) -> &str {
        "delete global variable remove clear"
    }

    fn icon_name(&self) -> &str {
        "database-minus"
    }

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert("name".to_owned(), Variant::String(String::new()));
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::DynamicSelect {
            key: "name",
            label: "Variable Name",
            options_key: "global.names",
        }]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        config.require_str("name").map(|_| ())
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, "core.globals.delete");

        let name_template = config.str("name").unwrap_or_default();

        let resolved_name =
            forge_types::strip_var_decoration(&ctx.arg_stack.interpolate(name_template));

        let outcome = match self.globals.delete(&resolved_name).await {
            Ok(_existed) => {
                ctx.publisher.publish(Event::caused_by(
                    EventSource::Core,
                    "global.deleted",
                    serde_json::json!({ "key": resolved_name }),
                    ctx.parent_event_id,
                ));
                SubActionOutcome::Success
            }
            Err(e) => SubActionOutcome::Failed(e.to_string()),
        };

        (timer.finish(outcome), None)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use forge_events::EventPublisher;
    use forge_registry::RunContext;
    use forge_storage::{GlobalEntry, StorageError};
    use forge_types::EventId;
    use std::collections::BTreeMap;
    use std::sync::Mutex;
    use time::OffsetDateTime;

    struct CapturingPublisher(Arc<Mutex<Vec<Event>>>);
    impl EventPublisher for CapturingPublisher {
        fn publish(&self, event: Event) {
            self.0.lock().unwrap().push(event);
        }
    }

    #[derive(Default)]
    struct MapGlobals {
        map: Mutex<BTreeMap<String, Variant>>,
    }

    impl MapGlobals {
        fn with(entries: impl IntoIterator<Item = (&'static str, Variant)>) -> Self {
            let map = entries
                .into_iter()
                .map(|(k, v)| (k.to_owned(), v))
                .collect();
            Self {
                map: Mutex::new(map),
            }
        }
    }

    #[async_trait]
    impl GlobalsRepo for MapGlobals {
        async fn get(&self, name: &str) -> Result<Option<Variant>, StorageError> {
            Ok(self.map.lock().unwrap().get(name).cloned())
        }
        async fn set(&self, name: &str, value: Variant, _p: bool) -> Result<(), StorageError> {
            self.map.lock().unwrap().insert(name.to_owned(), value);
            Ok(())
        }
        async fn delete(&self, name: &str) -> Result<bool, StorageError> {
            Ok(self.map.lock().unwrap().remove(name).is_some())
        }
        async fn list(&self) -> Result<Vec<GlobalEntry>, StorageError> {
            Ok(vec![])
        }
        async fn storage_bytes(&self) -> Result<u64, StorageError> {
            Ok(0)
        }
        async fn last_save_at(&self) -> Result<Option<OffsetDateTime>, StorageError> {
            Ok(None)
        }
        async fn incr(&self, _name: &str, _amount: i64) -> Result<Variant, StorageError> {
            Ok(Variant::Int(0))
        }
    }

    fn cfg(name: &str) -> SubActionConfig {
        let mut c = SubActionConfig::new();
        c.insert("name".to_owned(), Variant::String(name.to_owned()));
        c
    }

    #[tokio::test]
    async fn delete_runner_emits_global_deleted_carrying_key() {
        let globals = Arc::new(MapGlobals::with([("temp", Variant::Int(1))]));
        let events: Arc<Mutex<Vec<Event>>> = Arc::new(Mutex::new(Vec::new()));
        let publisher = CapturingPublisher(Arc::clone(&events));
        let parent = EventId::new();
        let stack = ArgStack::new();
        let ctx = RunContext::leaf(&stack, 0, parent, &publisher);
        let runner = CoreGlobalsDeleteRunner::new(globals);
        let outcome = runner.execute(&cfg("temp"), &ctx).await.0.outcome;

        assert!(matches!(outcome, SubActionOutcome::Success));
        let captured = events.lock().unwrap();
        let ev = &captured[0];
        assert_eq!(ev.kind, "global.deleted");
        assert_eq!(ev.caused_by, Some(parent));
        assert_eq!(ev.payload["key"], "temp");
    }
}
