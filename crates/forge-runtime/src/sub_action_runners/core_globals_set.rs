use std::sync::Arc;

use async_trait::async_trait;
use forge_events::EventSource;
use forge_registry::references::GLOBAL_OPTIONS_KEY;
use forge_registry::{
    ConfigProblem, ConfigSpec, ReferenceField, RunContext, StepTimer, SubActionCategory,
    SubActionRunner, TextEditor, TextField, ToggleField, TypedField,
};
use forge_storage::GlobalsRepo;
use forge_types::{
    ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, strip_var_decoration,
};

use super::core_globals_shared::NAME_KEY;
use crate::global_mutation::GlobalMutation;

const NAME: TextField = TextField {
    key: NAME_KEY,
    label: "Variable Name",
    editor: TextEditor::Line {
        placeholder: "my_counter",
    },
    default: "",
    required: true,
};

const VALUE: TextField = TextField {
    key: "value",
    label: "Value",
    editor: TextEditor::Line { placeholder: "42" },
    default: "",
    required: false,
};

const PERSISTED: ToggleField = ToggleField {
    key: "persisted",
    label: "Persist across restarts",
    default: false,
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&NAME, &VALUE, &PERSISTED]);

fn read_config(
    config: &SubActionConfig,
    args: &ArgStack,
) -> Result<(String, String, bool), ConfigProblem> {
    Ok((
        strip_var_decoration(&NAME.read(config, args)?),
        VALUE.read(config, args)?,
        PERSISTED.read(config, args)?,
    ))
}

pub struct CoreGlobalsSetRunner {
    globals: Arc<dyn GlobalsRepo>,
}

impl CoreGlobalsSetRunner {
    pub fn new(globals: Arc<dyn GlobalsRepo>) -> Self {
        Self { globals }
    }
}

#[async_trait]
impl SubActionRunner for CoreGlobalsSetRunner {
    fn id(&self) -> &str {
        "core.globals.set"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Globals
    }

    fn label(&self) -> &str {
        "Set Global"
    }

    fn summary(&self) -> &str {
        "Set a global variable to a value"
    }

    fn search_text(&self) -> &str {
        "set global variable store write"
    }

    fn icon_name(&self) -> &str {
        "database"
    }

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    fn declared_references(&self) -> Vec<ReferenceField> {
        vec![ReferenceField {
            key: NAME_KEY,
            options_key: GLOBAL_OPTIONS_KEY,
        }]
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, self.id());
        let (name, raw, persisted) = match read_config(config, ctx.arg_stack) {
            Ok(values) => values,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        let variant = super::variant_text::parse_variant(&raw);

        let mutation = GlobalMutation {
            globals: self.globals.as_ref(),
            publisher: ctx.publisher,
            source: EventSource::Core,
            caused_by: ctx.parent_event_id,
        };
        let outcome = match mutation.set(&name, variant, persisted).await {
            Ok(_previous) => SubActionOutcome::Success,
            Err(e) => SubActionOutcome::Failed(e.to_string()),
        };

        (timer.finish(outcome), None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sub_action_runners::variant_text::parse_variant;
    use forge_events::{Event, EventPublisher};
    use forge_registry::RunContext;
    use forge_storage::{GlobalEntry, StorageError};
    use forge_types::Variant;
    use forge_types::{ArgStack, EventId};
    use std::collections::BTreeMap;
    use std::sync::Mutex;
    use time::OffsetDateTime;

    struct NullPublisher;
    impl EventPublisher for NullPublisher {
        fn publish(&self, _event: Event) {}
    }

    #[derive(Default)]
    struct MapGlobals {
        map: Mutex<BTreeMap<String, (Variant, bool)>>,
    }

    impl MapGlobals {
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

    #[test]
    fn parse_variant_infers_type_per_input_format() {
        assert!(matches!(parse_variant("42"), Variant::Int(42)));
        assert!(matches!(parse_variant("-7"), Variant::Int(-7)));
        let f = parse_variant("3.99");
        assert!(matches!(f, Variant::Float(x) if (x - 3.99).abs() < 1e-10));
        assert!(matches!(parse_variant("true"), Variant::Bool(true)));
        assert!(matches!(parse_variant("TRUE"), Variant::Bool(true)));
        assert!(matches!(parse_variant("false"), Variant::Bool(false)));
        assert!(matches!(parse_variant("False"), Variant::Bool(false)));
        assert!(matches!(parse_variant("hello"), Variant::String(s) if s == "hello"));
        assert!(matches!(parse_variant(""), Variant::String(s) if s.is_empty()));
    }

    #[tokio::test]
    async fn set_runner_honors_configured_persisted_flag() {
        for (configured, expected) in [(Some(true), true), (Some(false), false), (None, false)] {
            let globals = Arc::new(MapGlobals::default());
            let mut config = SubActionConfig::new();
            config.insert("name".to_owned(), Variant::String("counter".to_owned()));
            config.insert("value".to_owned(), Variant::String("42".to_owned()));
            if let Some(p) = configured {
                config.insert("persisted".to_owned(), Variant::Bool(p));
            }

            let runner = CoreGlobalsSetRunner::new(Arc::clone(&globals) as Arc<dyn GlobalsRepo>);
            let stack = ArgStack::new();
            let ctx = RunContext::leaf(&stack, 0, EventId::new(), &NullPublisher);
            let outcome = runner.execute(&config, &ctx).await.0.outcome;

            assert!(matches!(outcome, SubActionOutcome::Success));
            assert_eq!(
                globals.persisted_flag("counter"),
                Some(expected),
                "configured persisted {configured:?} must reach set() as {expected}"
            );
        }
    }
}
