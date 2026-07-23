use std::sync::Arc;

use async_trait::async_trait;
use forge_events::{Event, EventSource};
use forge_registry::{
    FormField, RegistryError, RunContext, StepTimer, SubActionCategory, SubActionConfigExt,
    SubActionRunner,
};
use forge_storage::GlobalsRepo;
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant};

pub struct CoreGlobalsIncrementRunner {
    globals: Arc<dyn GlobalsRepo>,
}

impl CoreGlobalsIncrementRunner {
    pub fn new(globals: Arc<dyn GlobalsRepo>) -> Self {
        Self { globals }
    }
}

#[async_trait]
impl SubActionRunner for CoreGlobalsIncrementRunner {
    fn id(&self) -> &str {
        "core.globals.increment"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Globals
    }

    fn label(&self) -> &str {
        "Increment Global"
    }

    fn summary(&self) -> &str {
        "Increment a numeric global variable by an amount"
    }

    fn search_text(&self) -> &str {
        "increment global variable counter add"
    }

    fn icon_name(&self) -> &str {
        "plus"
    }

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert("name".to_owned(), Variant::String(String::new()));
        cfg.insert("amount".to_owned(), Variant::Int(1));
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::DynamicSelect {
                key: "name",
                label: "Variable Name",
                options_key: "global.names",
            },
            FormField::Integer {
                key: "amount",
                label: "Amount",
                min: i64::MIN,
                max: i64::MAX,
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        config.require_str("name").map(|_| ())
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, "core.globals.increment");

        let name_template = config.str("name").unwrap_or_default();
        let amount = config.int("amount").unwrap_or(1);

        let resolved_name =
            forge_types::strip_var_decoration(&ctx.arg_stack.interpolate(name_template));

        let outcome = match self.globals.incr(&resolved_name, amount).await {
            Ok(new_val) => {
                let new_val_json = match &new_val {
                    Variant::Int(i) => serde_json::Value::from(*i),
                    _ => serde_json::Value::String(new_val.to_string()),
                };
                ctx.publisher.publish(Event::caused_by(
                    EventSource::Core,
                    "global.incremented",
                    serde_json::json!({
                        "key": resolved_name,
                        "delta": amount,
                        "new_value": new_val_json,
                    }),
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
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use forge_events::EventPublisher;
    use forge_registry::RunContext;
    use forge_storage::{GlobalEntry, StorageError};
    use forge_types::{EventId, VariantKind};
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
        async fn incr(&self, name: &str, amount: i64) -> Result<Variant, StorageError> {
            let mut map = self.map.lock().unwrap();
            match map.get(name).cloned() {
                None => Err(StorageError::NotFound {
                    key: name.to_owned(),
                }),
                Some(Variant::Int(i)) => {
                    let nv = Variant::Int(i + amount);
                    map.insert(name.to_owned(), nv.clone());
                    Ok(nv)
                }
                Some(other) => Err(StorageError::TypeMismatch {
                    name: name.to_owned(),
                    actual: VariantKind::from_variant(&other).label().to_owned(),
                }),
            }
        }
    }

    fn cfg(name: &str, amount: i64) -> SubActionConfig {
        let mut c = SubActionConfig::new();
        c.insert("name".to_owned(), Variant::String(name.to_owned()));
        c.insert("amount".to_owned(), Variant::Int(amount));
        c
    }

    #[tokio::test]
    async fn increment_runner_emits_global_incremented_with_positive_delta() {
        let globals = Arc::new(MapGlobals::with([("counter", Variant::Int(10))]));
        let events: Arc<Mutex<Vec<Event>>> = Arc::new(Mutex::new(Vec::new()));
        let publisher = CapturingPublisher(Arc::clone(&events));
        let parent = EventId::new();
        let stack = ArgStack::new();
        let ctx = RunContext::leaf(&stack, 0, parent, &publisher);
        let runner = CoreGlobalsIncrementRunner::new(globals);
        let outcome = runner.execute(&cfg("counter", 5), &ctx).await.0.outcome;

        assert!(matches!(outcome, SubActionOutcome::Success));
        let captured = events.lock().unwrap();
        let ev = &captured[0];
        assert_eq!(ev.kind, "global.incremented");
        assert_eq!(ev.caused_by, Some(parent));
        assert_eq!(ev.payload["key"], "counter");
        assert_eq!(ev.payload["delta"].as_i64(), Some(5));
        assert_eq!(ev.payload["new_value"].as_i64(), Some(15));
    }

    #[tokio::test]
    async fn increment_missing_key_reports_failed() {
        let globals = Arc::new(MapGlobals::default());
        let stack = ArgStack::new();
        let ctx = RunContext::leaf(&stack, 0, EventId::new(), &NullPublisher);
        let runner = CoreGlobalsIncrementRunner::new(globals);
        let outcome = runner.execute(&cfg("ghost", 1), &ctx).await.0.outcome;
        assert!(matches!(outcome, SubActionOutcome::Failed(_)));
    }

    struct NullPublisher;
    impl EventPublisher for NullPublisher {
        fn publish(&self, _event: Event) {}
    }
}
