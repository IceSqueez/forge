use std::sync::Arc;

use async_trait::async_trait;
use forge_events::{EventSource, GlobalArrayAppended, TypedEvent};
use forge_registry::{
    ConfigProblem, ConfigSpec, IntegerEditor, IntegerField, RunContext, StepTimer,
    SubActionCategory, SubActionRunner, TextEditor, TextField, TypedField, ValueBounds,
};
use forge_storage::{GlobalsRepo, StorageError};
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, VariantKind};

use super::core_globals_shared::{GLOBAL_BY_KEY, global_name};

const UNBOUNDED_LENGTH: i64 = 0;

const VALUE: TextField = TextField {
    key: "value",
    label: "Value to Append",
    editor: TextEditor::Line {
        placeholder: "item",
    },
    default: "",
    required: false,
};

const MAX_LENGTH: IntegerField = IntegerField {
    key: "max_length",
    label: "Max Length (0 = unbounded)",
    editor: IntegerEditor::Number,
    default: UNBOUNDED_LENGTH,
    bounds: ValueBounds {
        min: Some(UNBOUNDED_LENGTH),
        max: None,
    },
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&GLOBAL_BY_KEY, &VALUE, &MAX_LENGTH]);

fn read_config(
    config: &SubActionConfig,
    args: &ArgStack,
) -> Result<(String, String, i64), ConfigProblem> {
    Ok((
        global_name(&GLOBAL_BY_KEY, config, args)?,
        VALUE.read(config, args)?,
        MAX_LENGTH.read(config, args)?,
    ))
}

pub struct CoreGlobalsArrayAppendRunner {
    globals: Arc<dyn GlobalsRepo>,
}

impl CoreGlobalsArrayAppendRunner {
    pub fn new(globals: Arc<dyn GlobalsRepo>) -> Self {
        Self { globals }
    }
}

#[async_trait]
impl SubActionRunner for CoreGlobalsArrayAppendRunner {
    fn id(&self) -> &str {
        "core.globals.array_append"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Globals
    }

    fn label(&self) -> &str {
        "Append to Global Array"
    }

    fn summary(&self) -> &str {
        "Append a value to a global array variable"
    }

    fn search_text(&self) -> &str {
        "append push array global list add item"
    }

    fn icon_name(&self) -> &str {
        "list-plus"
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
        let (resolved_key, raw_value, max_len) = match read_config(config, ctx.arg_stack) {
            Ok(values) => values,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        let item = super::variant_text::parse_variant(&raw_value);
        let element_json = item.to_plain_json();
        let bound =
            (max_len > UNBOUNDED_LENGTH).then(|| usize::try_from(max_len).unwrap_or(usize::MAX));

        let outcome = match self.globals.array_append(&resolved_key, item, bound).await {
            Ok(new_len) => {
                GlobalArrayAppended {
                    key: resolved_key.clone(),
                    new_length: new_len,
                    element: element_json,
                }
                .publish(
                    ctx.publisher,
                    EventSource::Core,
                    Some(ctx.parent_event_id),
                );
                SubActionOutcome::Success
            }
            Err(StorageError::TypeMismatch { actual, .. }) => SubActionOutcome::Failed(format!(
                "core.globals.array_append: expected array, found {}",
                VariantKind::from_contract_name(&actual)
                    .map_or(actual.as_str(), |kind| kind.label())
            )),
            Err(e) => SubActionOutcome::Failed(e.to_string()),
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
    use forge_types::EventId;
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
        map: Mutex<BTreeMap<String, (Variant, bool)>>,
    }

    impl MapGlobals {
        fn with(entries: impl IntoIterator<Item = (&'static str, Variant)>) -> Self {
            Self::seeded(entries.into_iter().map(|(k, v)| (k, v, false)))
        }
        fn seeded(entries: impl IntoIterator<Item = (&'static str, Variant, bool)>) -> Self {
            let map = entries
                .into_iter()
                .map(|(k, v, p)| (k.to_owned(), (v, p)))
                .collect();
            Self {
                map: Mutex::new(map),
            }
        }
        fn array(&self, key: &str) -> Vec<Variant> {
            match self.map.lock().unwrap().get(key) {
                Some((Variant::Array(a), _)) => a.clone(),
                other => panic!("expected array at {key}, got {other:?}"),
            }
        }
        fn persisted_flag(&self, key: &str) -> Option<bool> {
            self.map.lock().unwrap().get(key).map(|(_, p)| *p)
        }
    }

    #[async_trait]
    impl GlobalsRepo for MapGlobals {
        async fn get(&self, name: &str) -> Result<Option<Variant>, StorageError> {
            Ok(self.map.lock().unwrap().get(name).map(|(v, _)| v.clone()))
        }
        async fn set(&self, name: &str, value: Variant, p: bool) -> Result<(), StorageError> {
            self.map.lock().unwrap().insert(name.to_owned(), (value, p));
            Ok(())
        }
        async fn persisted(&self, name: &str) -> Result<Option<bool>, StorageError> {
            Ok(self.map.lock().unwrap().get(name).map(|(_, p)| *p))
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

    fn cfg(key: &str, value: &str, max_length: i64) -> SubActionConfig {
        let mut c = SubActionConfig::new();
        c.insert("key".to_owned(), Variant::String(key.to_owned()));
        c.insert("value".to_owned(), Variant::String(value.to_owned()));
        c.insert("max_length".to_owned(), Variant::Int(max_length));
        c
    }

    async fn run(globals: Arc<MapGlobals>, config: &SubActionConfig) -> SubActionOutcome {
        let runner = CoreGlobalsArrayAppendRunner::new(globals);
        let stack = ArgStack::new();
        let ctx = RunContext::leaf(&stack, 0, EventId::new(), &NullPublisher);
        runner.execute(config, &ctx).await.0.outcome
    }

    #[tokio::test]
    async fn array_append_adds_parsed_value_at_end() {
        let globals = Arc::new(MapGlobals::with([(
            "list",
            Variant::Array(vec![Variant::Int(1), Variant::Int(2)]),
        )]));
        let outcome = run(globals.clone(), &cfg("list", "3", 0)).await;
        assert!(matches!(outcome, SubActionOutcome::Success));
        assert_eq!(
            globals.array("list"),
            vec![Variant::Int(1), Variant::Int(2), Variant::Int(3)]
        );
    }

    #[tokio::test]
    async fn array_append_creates_single_element_array_when_key_missing() {
        let globals = Arc::new(MapGlobals::default());
        let outcome = run(globals.clone(), &cfg("fresh", "hello", 0)).await;
        assert!(matches!(outcome, SubActionOutcome::Success));
        assert_eq!(
            globals.array("fresh"),
            vec![Variant::String("hello".to_owned())]
        );
    }

    #[tokio::test]
    async fn array_append_to_created_global_is_session() {
        let globals = Arc::new(MapGlobals::default());
        let outcome = run(globals.clone(), &cfg("fresh", "hello", 0)).await;
        assert!(matches!(outcome, SubActionOutcome::Success));
        assert_eq!(globals.persisted_flag("fresh"), Some(false));
    }

    #[tokio::test]
    async fn array_append_preserves_existing_persisted_flag() {
        let globals = Arc::new(MapGlobals::seeded([(
            "list",
            Variant::Array(vec![Variant::Int(1)]),
            true,
        )]));
        let outcome = run(globals.clone(), &cfg("list", "2", 0)).await;
        assert!(matches!(outcome, SubActionOutcome::Success));
        assert_eq!(globals.persisted_flag("list"), Some(true));
    }

    #[tokio::test]
    async fn array_append_non_array_global_reports_failed() {
        let globals = Arc::new(MapGlobals::with([("list", Variant::Int(5))]));
        let outcome = run(globals, &cfg("list", "x", 0)).await;
        assert!(matches!(outcome, SubActionOutcome::Failed(_)));
    }

    #[tokio::test]
    async fn array_append_past_max_length_drains_oldest_fifo() {
        let globals = Arc::new(MapGlobals::with([(
            "list",
            Variant::Array(vec![Variant::Int(1), Variant::Int(2), Variant::Int(3)]),
        )]));
        let outcome = run(globals.clone(), &cfg("list", "4", 3)).await;
        assert!(matches!(outcome, SubActionOutcome::Success));
        assert_eq!(
            globals.array("list"),
            vec![Variant::Int(2), Variant::Int(3), Variant::Int(4)]
        );
    }

    #[tokio::test]
    async fn array_append_max_length_zero_is_unbounded() {
        let globals = Arc::new(MapGlobals::with([(
            "list",
            Variant::Array(vec![Variant::Int(1), Variant::Int(2), Variant::Int(3)]),
        )]));
        run(globals.clone(), &cfg("list", "4", 0)).await;
        assert_eq!(globals.array("list").len(), 4, "0 must not bound the array");
    }

    #[tokio::test]
    async fn array_append_runner_emits_global_array_appended_with_element_and_length() {
        let globals = Arc::new(MapGlobals::with([(
            "list",
            Variant::Array(vec![Variant::Int(1), Variant::Int(2)]),
        )]));
        let events: Arc<Mutex<Vec<Event>>> = Arc::new(Mutex::new(Vec::new()));
        let publisher = CapturingPublisher(Arc::clone(&events));
        let parent = EventId::new();
        let stack = ArgStack::new();
        let ctx = RunContext::leaf(&stack, 0, parent, &publisher);
        let runner = CoreGlobalsArrayAppendRunner::new(globals);
        let outcome = runner.execute(&cfg("list", "3", 0), &ctx).await.0.outcome;

        assert!(matches!(outcome, SubActionOutcome::Success));
        let captured = events.lock().unwrap();
        let ev = &captured[0];
        assert_eq!(ev.kind, "global.array_appended");
        assert_eq!(ev.caused_by, Some(parent));
        assert_eq!(ev.payload["key"], "list");
        assert_eq!(ev.payload["new_length"].as_u64(), Some(3));
        assert_eq!(ev.payload["element"].as_i64(), Some(3));
    }
}
