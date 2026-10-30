use std::sync::Arc;

use async_trait::async_trait;
use forge_events::{EventSource, GlobalDecremented, TypedEvent};
use forge_registry::{
    ConfigProblem, ConfigSpec, RunContext, StepTimer, SubActionCategory, SubActionRunner,
    TypedField,
};
use forge_storage::GlobalsRepo;
use forge_types::{
    ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, UserText, Variant,
};

use super::core_globals_shared::{AMOUNT, GLOBAL_BY_KEY, global_name};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&GLOBAL_BY_KEY, &AMOUNT]);

fn read_config(config: &SubActionConfig, args: &ArgStack) -> Result<(String, i64), ConfigProblem> {
    Ok((
        global_name(&GLOBAL_BY_KEY, config, args)?,
        AMOUNT.read(config, args)?,
    ))
}

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

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, self.id());
        let (resolved_key, amount) = match read_config(config, ctx.arg_stack) {
            Ok(values) => values,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };

        let Some(delta) = amount.checked_neg() else {
            return (
                timer.failed(UserText::unlocalized(format!(
                    "core.globals.decrement: amount {amount} cannot be negated"
                ))),
                None,
            );
        };

        let outcome = match self.globals.incr(&resolved_key, delta).await {
            Ok(new_val) => {
                let new_val_json = match &new_val {
                    Variant::Int(i) => serde_json::Value::from(*i),
                    _ => serde_json::Value::String(new_val.to_string()),
                };
                GlobalDecremented {
                    key: resolved_key.clone(),
                    delta: amount,
                    new_value: new_val_json,
                }
                .publish(
                    ctx.publisher,
                    EventSource::Core,
                    Some(ctx.parent_event_id),
                );
                SubActionOutcome::Success
            }
            Err(e) => SubActionOutcome::Failed(UserText::unlocalized(e.to_string())),
        };

        (timer.finish(outcome), None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_events::Event;
    use forge_events::EventPublisher;
    use forge_storage::{GlobalEntry, StorageError};
    use forge_types::{EventId, VariantKind};
    use std::collections::BTreeMap;
    use std::sync::Mutex;
    use time::OffsetDateTime;

    struct NullPublisher;
    impl EventPublisher for NullPublisher {
        fn publish(&self, _event: Event) {}
    }

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
        let ctx = RunContext::leaf(&stack, 0, EventId::new(), &NullPublisher);
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

    #[tokio::test]
    async fn decrement_by_i64_min_fails_and_leaves_the_global_untouched() {
        let globals = Arc::new(MapGlobals::with([("counter", Variant::Int(5))]));
        let outcome = run(globals.clone(), &cfg("counter", i64::MIN)).await;
        assert!(
            matches!(&outcome, SubActionOutcome::Failed(reason) if reason.contains("negated")),
            "got {outcome:?}"
        );
        assert_eq!(globals.snapshot("counter"), Some(Variant::Int(5)));
    }

    async fn capture(globals: Arc<MapGlobals>, config: &SubActionConfig) -> Vec<Event> {
        let events: Arc<Mutex<Vec<Event>>> = Arc::new(Mutex::new(Vec::new()));
        let publisher = CapturingPublisher(Arc::clone(&events));
        let stack = ArgStack::new();
        let ctx = RunContext::leaf(&stack, 0, EventId::new(), &publisher);
        let runner = CoreGlobalsDecrementRunner::new(globals);
        let _ = runner.execute(config, &ctx).await;
        let captured = events.lock().unwrap();
        captured.clone()
    }

    #[tokio::test]
    async fn decrement_runner_emits_global_decremented_with_positive_delta() {
        let globals = Arc::new(MapGlobals::with([("counter", Variant::Int(10))]));
        let events = capture(globals, &cfg("counter", 3)).await;
        let ev = &events[0];
        assert_eq!(ev.kind, "global.decremented");
        assert_eq!(ev.payload["key"], "counter");
        assert_eq!(ev.payload["delta"].as_i64(), Some(3));
        assert_eq!(ev.payload["new_value"].as_i64(), Some(7));
    }

    #[tokio::test]
    async fn decrement_never_emits_incremented_with_negative_delta() {
        let globals = Arc::new(MapGlobals::with([("counter", Variant::Int(10))]));
        let events = capture(globals, &cfg("counter", 4)).await;
        assert!(
            !events.iter().any(|e| e.kind.contains("incr")),
            "decrement must never masquerade as an increment"
        );
        assert!(
            events
                .iter()
                .all(|e| e.payload["delta"].as_i64() != Some(-4)),
            "sign lives in the verb; delta stays positive"
        );
    }
}
