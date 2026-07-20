use async_trait::async_trait;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

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

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();

        let source = config.get("source").and_then(|v| v.as_str()).unwrap_or("");
        let into_var = super::interpolate::sanitize_var_name(
            config
                .get("into_var")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .unwrap_or("string.result"),
        );

        let result = source.to_lowercase();
        let new_stack = ctx.arg_stack.clone().set(into_var, Variant::String(result));

        let duration_ms = (OffsetDateTime::now_utc() - started_at)
            .whole_milliseconds()
            .max(0) as u64;

        (
            SubActionTelemetry {
                index: ctx.index,
                kind: "core.string.lowercase".to_owned(),
                started_at,
                duration_ms,
                outcome: SubActionOutcome::Success,
            },
            Some(new_stack),
        )
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
