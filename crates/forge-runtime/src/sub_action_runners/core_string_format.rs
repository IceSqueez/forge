use async_trait::async_trait;
use forge_registry::{
    FormField, ProducedVariable, RegistryError, RunContext, StepTimer, SubActionCategory,
    SubActionConfigExt, SubActionIo, SubActionRunner,
};
use forge_types::{ArgStack, SubActionConfig, SubActionTelemetry, Variant, VariantKind};

pub struct CoreStringFormatRunner;

#[async_trait]
impl SubActionRunner for CoreStringFormatRunner {
    fn id(&self) -> &str {
        "core.string.format"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Util
    }

    fn label(&self) -> &str {
        "String - Format Template"
    }

    fn summary(&self) -> &str {
        "Compose a string from a template with %variable% placeholders"
    }

    fn search_text(&self) -> &str {
        "string format template compose placeholder variable"
    }

    fn icon_name(&self) -> &str {
        "template"
    }

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert("template".to_owned(), Variant::String(String::new()));
        cfg.insert(
            "into_var".to_owned(),
            Variant::String("string.formatted".to_owned()),
        );
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::TextArea {
                key: "template",
                label: "Template",
            },
            FormField::Text {
                key: "into_var",
                label: "Output Variable",
                placeholder: "string.formatted",
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
                label: "Formatted string".to_owned(),
            }],
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, "core.string.format");

        let template = config.str("template").unwrap_or("").to_owned();
        let into_var = super::interpolate::sanitize_var_name(
            config
                .str_nonempty("into_var")
                .unwrap_or("string.formatted"),
        );

        let new_stack = ctx
            .arg_stack
            .clone()
            .set(into_var, Variant::String(template));

        (timer.success(), Some(new_stack))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use forge_events::{Event, EventPublisher};
    use forge_types::EventId;

    struct NullPublisher;
    impl EventPublisher for NullPublisher {
        fn publish(&self, _event: Event) {}
    }

    #[tokio::test]
    async fn format_writes_template_verbatim_without_reinterpolating_vars() {
        let mut cfg = SubActionConfig::new();
        cfg.insert(
            "template".to_owned(),
            Variant::String("Hi %name%".to_owned()),
        );
        // %name% is already resolved upstream; the runner must NOT re-interpolate it,
        // even when the arg stack carries a matching binding.
        let stack = ArgStack::new().set("name".to_owned(), Variant::String("Alice".to_owned()));
        let ctx = RunContext::leaf(&stack, 0, EventId::new(), &NullPublisher);
        let out = CoreStringFormatRunner.execute(&cfg, &ctx).await.1.unwrap();
        assert_eq!(
            out.get("string.formatted").and_then(|v| v.as_str()),
            Some("Hi %name%")
        );
    }
}
