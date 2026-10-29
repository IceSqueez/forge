use async_trait::async_trait;
use forge_registry::{
    ConfigSpec, ProducedVariable, RunContext, StepTimer, SubActionCategory, SubActionIo,
    SubActionRunner, TextField,
};
use forge_types::{ArgStack, SubActionConfig, SubActionTelemetry, Variant, VariantKind};

use super::core_string_shared::{INTO_RESULT, INTO_VAR_KEY, read_source_into, source_field};

const SOURCE: TextField = source_field("Hello World");

const CONFIG: ConfigSpec = ConfigSpec::new(&[&SOURCE, &INTO_RESULT]);

pub struct CoreStringUppercaseRunner;

#[async_trait]
impl SubActionRunner for CoreStringUppercaseRunner {
    fn id(&self) -> &str {
        "core.string.uppercase"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Util
    }

    fn label(&self) -> &str {
        "String - Uppercase"
    }

    fn summary(&self) -> &str {
        "Convert a string to uppercase"
    }

    fn search_text(&self) -> &str {
        "string uppercase upper case convert"
    }

    fn icon_name(&self) -> &str {
        "letter-case-upper"
    }

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    fn scope_io(&self) -> SubActionIo {
        SubActionIo {
            produces: vec![ProducedVariable {
                output_name_key: INTO_VAR_KEY.to_owned(),
                kind: VariantKind::String,
                label: "Uppercased string".to_owned(),
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
        let (source, into_var) = match read_source_into(&SOURCE, config, ctx.arg_stack) {
            Ok(values) => values,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };

        let result = source.to_uppercase();
        let new_stack = ctx.arg_stack.clone().set(into_var, Variant::String(result));

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

    #[tokio::test]
    async fn uppercase_emits_facade_telemetry_shape() {
        let mut cfg = SubActionConfig::new();
        cfg.insert("source".to_owned(), Variant::String("hi".to_owned()));
        let stack = ArgStack::new();
        let ctx = RunContext::leaf(&stack, 4, EventId::new(), &NullPublisher);
        let tel = CoreStringUppercaseRunner.execute(&cfg, &ctx).await.0;
        assert_eq!(tel.kind, "core.string.uppercase");
        assert_eq!(tel.index, 4);
        assert!(tel.args_in.is_empty() && tel.produced.is_empty());
        assert!(matches!(tel.outcome, SubActionOutcome::Success));
    }

    #[tokio::test]
    async fn uppercase_folds_source_into_result_var() {
        let mut cfg = SubActionConfig::new();
        cfg.insert("source".to_owned(), Variant::String("café".to_owned()));
        let stack = ArgStack::new();
        let ctx = RunContext::leaf(&stack, 0, EventId::new(), &NullPublisher);
        let out = CoreStringUppercaseRunner
            .execute(&cfg, &ctx)
            .await
            .1
            .unwrap();
        assert_eq!(
            out.get("string.result").and_then(|v| v.as_str()),
            Some("CAFÉ")
        );
    }
}
