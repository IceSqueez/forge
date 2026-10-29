use async_trait::async_trait;
use forge_registry::{
    ConfigProblem, ConfigSpec, ProducedVariable, RunContext, StepTimer, SubActionCategory,
    SubActionIo, SubActionRunner, TextEditor, TextField, TypedField, ValueField,
};
use forge_types::{
    ArgStack, SubActionConfig, SubActionTelemetry, Variant, VariantKind, strip_var_decoration,
};

const NAME_KEY: &str = "name";

const NAME: TextField = TextField {
    key: NAME_KEY,
    label: "Variable Name",
    editor: TextEditor::Line {
        placeholder: "my_var",
    },
    default: "",
    required: true,
};

const VALUE: ValueField = ValueField {
    key: "value",
    label: "Value",
    placeholder: "hello",
    default: "",
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&NAME, &VALUE]);

fn read_config(
    config: &SubActionConfig,
    args: &ArgStack,
) -> Result<(String, Variant), ConfigProblem> {
    let name = strip_var_decoration(&NAME.read(config, args)?);
    let value = match VALUE.read(config, args)? {
        Variant::String(text) => super::variant_text::parse_variant(&text),
        other => other,
    };
    Ok((name, value))
}

pub struct CoreArgsSetRunner;

#[async_trait]
impl SubActionRunner for CoreArgsSetRunner {
    fn id(&self) -> &str {
        "core.args.set"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Util
    }

    fn label(&self) -> &str {
        "Set Local Arg"
    }

    fn summary(&self) -> &str {
        "Set a variable in the current execution's ArgStack"
    }

    fn search_text(&self) -> &str {
        "set local arg variable execution stack transient"
    }

    fn icon_name(&self) -> &str {
        "variable"
    }

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    fn scope_io(&self) -> SubActionIo {
        SubActionIo {
            produces: vec![ProducedVariable {
                output_name_key: NAME_KEY.to_owned(),
                kind: VariantKind::String,
                label: "Argument value".to_owned(),
            }],
            ..SubActionIo::default()
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, self.id());
        let (name, value) = match read_config(config, ctx.arg_stack) {
            Ok(values) => values,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        let new_stack = ctx.arg_stack.clone().set(name, value);
        (timer.success(), Some(new_stack))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_events::{Event, EventPublisher};
    use forge_types::{EventId, SubActionOutcome};

    struct NullPublisher;
    impl EventPublisher for NullPublisher {
        fn publish(&self, _event: Event) {}
    }

    fn cfg(name: &str, value: Variant) -> SubActionConfig {
        let mut c = SubActionConfig::new();
        c.insert("name".to_owned(), Variant::String(name.to_owned()));
        c.insert("value".to_owned(), value);
        c
    }

    async fn run(config: &SubActionConfig) -> ArgStack {
        let stack = ArgStack::new();
        let ctx = RunContext::leaf(&stack, 0, EventId::new(), &NullPublisher);
        let (telemetry, new_stack) = CoreArgsSetRunner.execute(config, &ctx).await;
        assert!(matches!(telemetry.outcome, SubActionOutcome::Success));
        new_stack.expect("args.set always returns a mutated stack")
    }

    #[tokio::test]
    async fn string_value_is_type_inferred_when_bound_to_arg_stack() {
        let stack = run(&cfg("answer", Variant::String("42".to_owned()))).await;
        assert!(matches!(stack.get("answer"), Some(Variant::Int(42))));
    }

    #[tokio::test]
    async fn pretyped_non_string_value_is_stored_without_reparsing() {
        let stack = run(&cfg("ratio", Variant::float(2.0).unwrap())).await;
        assert!(matches!(stack.get("ratio"), Some(Variant::Float(f)) if (*f - 2.0).abs() < 1e-12));
    }
}
