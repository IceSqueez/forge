use async_trait::async_trait;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

/// Values written here survive only for the duration of the current action execution; they are not persisted.
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

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert("name".to_owned(), Variant::String(String::new()));
        cfg.insert("value".to_owned(), Variant::String(String::new()));
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Text {
                key: "name",
                label: "Variable Name",
                placeholder: "my_var",
            },
            FormField::Text {
                key: "value",
                label: "Value",
                placeholder: "hello",
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("name").and_then(|v| v.as_str()) {
            Some(s) if !s.is_empty() => Ok(()),
            _ => Err(RegistryError::UnknownKindId(
                "core.args.set: name is required".to_owned(),
            )),
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();

        let name_template = config
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let name = super::interpolate::sanitize_var_name(&ctx.arg_stack.interpolate(name_template));

        let value = match config.get("value") {
            Some(Variant::String(s)) => {
                let interpolated = ctx.arg_stack.interpolate(s);
                super::interpolate::parse_variant(&interpolated)
            }
            Some(v) => v.clone(),
            None => Variant::String(String::new()),
        };

        let new_stack = ctx.arg_stack.clone().set(name, value);

        let duration_ms = (OffsetDateTime::now_utc() - started_at)
            .whole_milliseconds()
            .max(0) as u64;

        (
            SubActionTelemetry {
                args_in: ::std::collections::BTreeMap::new(),
                produced: ::std::collections::BTreeMap::new(),
                index: ctx.index,
                kind: "core.args.set".to_owned(),
                started_at,
                duration_ms,
                outcome: SubActionOutcome::Success,
            },
            Some(new_stack),
        )
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use forge_events::{Event, EventPublisher};
    use forge_types::EventId;

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
        // A String config value routes through parse_variant: "42" becomes Int(42),
        // not a String left verbatim.
        let stack = run(&cfg("answer", Variant::String("42".to_owned()))).await;
        assert!(matches!(stack.get("answer"), Some(Variant::Int(42))));
    }

    #[tokio::test]
    async fn pretyped_non_string_value_is_stored_without_reparsing() {
        // Float(2.0) Displays as "2" and would re-parse to Int(2); the runner must keep
        // a pre-typed Variant verbatim instead of round-tripping it through string parsing.
        let stack = run(&cfg("ratio", Variant::float(2.0).unwrap())).await;
        assert!(matches!(stack.get("ratio"), Some(Variant::Float(f)) if (*f - 2.0).abs() < 1e-12));
    }
}
