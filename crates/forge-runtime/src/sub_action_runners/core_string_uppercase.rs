use async_trait::async_trait;
use forge_registry::{
    FormField, ProducedVariable, RegistryError, RunContext, StepTimer, SubActionCategory,
    SubActionConfigExt, SubActionIo, SubActionRunner,
};
use forge_types::{ArgStack, SubActionConfig, SubActionTelemetry, Variant, VariantKind};

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

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert("source".to_owned(), Variant::String(String::new()));
        cfg.insert(
            "into_var".to_owned(),
            Variant::String("string.result".to_owned()),
        );
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Text {
                key: "source",
                label: "Source",
                placeholder: "Hello World",
            },
            FormField::Text {
                key: "into_var",
                label: "Output Variable",
                placeholder: "string.result",
            },
        ]
    }

    fn validate_config(&self, _config: &SubActionConfig) -> Result<(), RegistryError> {
        Ok(())
    }

    fn scope_io(&self) -> SubActionIo {
        SubActionIo {
            produces: vec![ProducedVariable {
                output_name_key: "into_var".to_owned(),
                kind: VariantKind::String,
                label: "Uppercased string".to_owned(),
            }],
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, "core.string.uppercase");

        let source = config.str("source").unwrap_or("");
        let into_var = super::interpolate::sanitize_var_name(
            config.str_nonempty("into_var").unwrap_or("string.result"),
        );

        let result = source.to_uppercase();
        let new_stack = ctx.arg_stack.clone().set(into_var, Variant::String(result));

        (timer.success(), Some(new_stack))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
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
        // Mass-migration safety net: a migrated runner must route telemetry
        // through StepTimer verbatim - kind/index from ctx, empty arg maps.
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
