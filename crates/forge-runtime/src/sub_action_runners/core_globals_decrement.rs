use std::sync::Arc;

use async_trait::async_trait;
use forge_events::{Event, EventSource};
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_storage::GlobalsRepo;
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

pub struct CoreGlobalsDecrementRunner {
    globals: Arc<dyn GlobalsRepo>,
}

impl CoreGlobalsDecrementRunner {
    pub fn new(globals: Arc<dyn GlobalsRepo>) -> Self {
        Self { globals }
    }
}

#[async_trait]
impl SubActionRunner for CoreGlobalsDecrementRunner {
    fn id(&self) -> &str {
        "core.globals.decrement"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Globals
    }

    fn label(&self) -> &str {
        "Decrement Global"
    }

    fn summary(&self) -> &str {
        "Decrement a numeric global variable by an amount"
    }

    fn search_text(&self) -> &str {
        "decrement global variable counter subtract minus"
    }

    fn icon_name(&self) -> &str {
        "minus"
    }

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert("key".to_owned(), Variant::String(String::new()));
        cfg.insert("amount".to_owned(), Variant::Int(1));
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Text {
                key: "key",
                label: "Variable Name",
                placeholder: "counter",
            },
            FormField::Integer {
                key: "amount",
                label: "Amount",
                min: 0,
                max: i64::MAX,
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("key").and_then(|v| v.as_str()) {
            Some(s) if !s.is_empty() => Ok(()),
            _ => Err(RegistryError::UnknownKindId(
                "core.globals.decrement: key is required".to_owned(),
            )),
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();

        let key_template = config
            .get("key")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let amount = config.get("amount").and_then(|v| v.as_int()).unwrap_or(1);

        let resolved_key = super::interpolate::interpolate_with_globals(
            key_template,
            ctx.arg_stack,
            self.globals.as_ref(),
        )
        .await;

        let outcome = match self.globals.incr(&resolved_key, -amount).await {
            Ok(new_val) => {
                let new_val_json = match &new_val {
                    Variant::Int(i) => serde_json::Value::from(*i),
                    _ => serde_json::Value::String(new_val.to_string()),
                };
                ctx.publisher.publish(Event::caused_by(
                    EventSource::Core,
                    "global.incr",
                    serde_json::json!({
                        "key": resolved_key,
                        "delta": -amount,
                        "new_value": new_val_json,
                    }),
                    ctx.parent_event_id,
                ));
                SubActionOutcome::Success
            }
            Err(e) => SubActionOutcome::Failed(e.to_string()),
        };

        let duration_ms = (OffsetDateTime::now_utc() - started_at)
            .whole_milliseconds()
            .max(0) as u64;

        (
            SubActionTelemetry {
                index: ctx.index,
                kind: "core.globals.decrement".to_owned(),
                started_at,
                duration_ms,
                outcome,
            },
            None,
        )
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use forge_events::EventPublisher;
    use forge_storage::{GlobalEntry, StorageError};
    use forge_types::{EventId, VariantKind};
    use std::collections::BTreeMap;
    use std::sync::Mutex;

    struct NullPublisher;
    impl EventPublisher for NullPublisher {
        fn publish(&self, _event: Event) {}
    }

    /// In-memory `GlobalsRepo` faithful to the SQLite backend's `incr` contract:
    /// missing key -> `NotFound`, non-numeric -> `TypeMismatch`, numeric -> add & return.
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
        fn snapshot(&self, key: &str) -> Option<Variant> {
            self.map.lock().unwrap().get(key).cloned()
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
                Some(Variant::Float(f)) => {
                    let nv = Variant::float(f + amount as f64).expect("finite");
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

    fn cfg(key: &str, amount: i64) -> SubActionConfig {
        let mut c = SubActionConfig::new();
        c.insert("key".to_owned(), Variant::String(key.to_owned()));
        c.insert("amount".to_owned(), Variant::Int(amount));
        c
    }

    async fn run(globals: Arc<MapGlobals>, config: &SubActionConfig) -> SubActionOutcome {
        let runner = CoreGlobalsDecrementRunner::new(globals);
        let stack = ArgStack::new();
        let ctx = RunContext {
            arg_stack: &stack,
            index: 0,
            parent_event_id: EventId::new(),
            publisher: &NullPublisher,
        };
        runner.execute(config, &ctx).await.0.outcome
    }

    #[tokio::test]
    async fn decrement_applies_negated_amount_to_int_global() {
        for (start, amount, expected) in [(10, 3, 7), (10, -5, 15), (0, 1, -1)] {
            let globals = Arc::new(MapGlobals::with([("counter", Variant::Int(start))]));
            let outcome = run(globals.clone(), &cfg("counter", amount)).await;
            assert!(
                matches!(outcome, SubActionOutcome::Success),
                "{start} - {amount} should succeed"
            );
            assert_eq!(
                globals.snapshot("counter"),
                Some(Variant::Int(expected)),
                "{start} - {amount} should store {expected}"
            );
        }
    }

    #[tokio::test]
    async fn decrement_missing_key_reports_failed() {
        let globals = Arc::new(MapGlobals::default());
        let outcome = run(globals, &cfg("ghost", 1)).await;
        assert!(matches!(outcome, SubActionOutcome::Failed(_)));
    }

    #[tokio::test]
    async fn decrement_non_numeric_global_reports_failed() {
        let globals = Arc::new(MapGlobals::with([(
            "counter",
            Variant::String("nope".to_owned()),
        )]));
        let outcome = run(globals.clone(), &cfg("counter", 1)).await;
        assert!(matches!(outcome, SubActionOutcome::Failed(_)));
        assert_eq!(
            globals.snapshot("counter"),
            Some(Variant::String("nope".to_owned())),
            "value must be untouched on failure"
        );
    }
}
