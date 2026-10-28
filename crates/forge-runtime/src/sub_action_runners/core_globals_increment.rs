use std::sync::Arc;

use async_trait::async_trait;
use forge_events::EventSource;
use forge_registry::{
    FormField, RegistryError, RunContext, StepTimer, SubActionCategory, SubActionConfigExt,
    SubActionRunner,
};
use forge_storage::GlobalsRepo;
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant};

use crate::global_mutation::GlobalMutation;

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

        let mutation = GlobalMutation {
            globals: self.globals.as_ref(),
            publisher: ctx.publisher,
            source: EventSource::Core,
            caused_by: ctx.parent_event_id,
        };
        let outcome = match mutation.increment(&resolved_name, amount).await {
            Ok(_increment) => SubActionOutcome::Success,
            Err(e) => SubActionOutcome::Failed(e.to_string()),
        };

        (timer.finish(outcome), None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_events::{Event, EventPublisher};
    use forge_registry::RunContext;
    use forge_storage::{GlobalEntry, StorageError};
    use forge_types::{EventId, VariantKind};
    use std::collections::BTreeMap;
    use std::sync::Mutex;
    use time::OffsetDateTime;

    #[derive(Default)]
    struct MapGlobals {
        map: Mutex<BTreeMap<String, Variant>>,
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
