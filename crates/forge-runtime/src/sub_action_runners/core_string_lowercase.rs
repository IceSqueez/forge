use async_trait::async_trait;
use forge_registry::{
    ConfigSpec, ProducedVariable, RunContext, StepTimer, SubActionCategory, SubActionIo,
    SubActionRunner, TextField,
};
use forge_types::{ArgStack, SubActionConfig, SubActionTelemetry, Variant, VariantKind};

use super::core_string_shared::{INTO_RESULT, INTO_VAR_KEY, read_source_into, source_field};

const SOURCE: TextField = source_field("Hello World");

const CONFIG: ConfigSpec = ConfigSpec::new(&[&SOURCE, &INTO_RESULT]);

pub struct CoreStringLowercaseRunner;

#[async_trait]
impl SubActionRunner for CoreStringLowercaseRunner {
    fn id(&self) -> &str {
        "core.string.lowercase"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Util
    }

    fn label(&self) -> &str {
        "String - Lowercase"
    }

    fn summary(&self) -> &str {
        "Convert a string to lowercase"
    }

    fn search_text(&self) -> &str {
        "string lowercase lower case convert"
    }

    fn icon_name(&self) -> &str {
        "letter-case-lower"
    }

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    fn scope_io(&self) -> SubActionIo {
        SubActionIo {
            produces: vec![ProducedVariable {
                output_name_key: INTO_VAR_KEY.to_owned(),
                kind: VariantKind::String,
                label: "Lowercased string".to_owned(),
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

        let result = source.to_lowercase();
        let new_stack = ctx.arg_stack.clone().set(into_var, Variant::String(result));

        (timer.success(), Some(new_stack))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_events::test_support::NullPublisher;

    use forge_types::EventId;

    #[tokio::test]
    async fn lowercase_folds_source_into_result_var() {
        let mut cfg = SubActionConfig::new();
        cfg.insert(
            "source".to_owned(),
            Variant::String("HELLO Wörld".to_owned()),
        );
        let stack = ArgStack::new();
        let ctx = RunContext::leaf(&stack, 0, EventId::new(), &NullPublisher);
        let out = CoreStringLowercaseRunner
            .execute(&cfg, &ctx)
            .await
            .1
            .unwrap();
        assert_eq!(
            out.get("string.result").and_then(|v| v.as_str()),
            Some("hello wörld")
        );
    }
}
