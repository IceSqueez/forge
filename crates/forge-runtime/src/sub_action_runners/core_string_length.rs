use async_trait::async_trait;
use forge_registry::{
    FormField, ProducedVariable, RegistryError, RunContext, StepTimer, SubActionCategory,
    SubActionConfigExt, SubActionIo, SubActionRunner,
};
use forge_types::{ArgStack, SubActionConfig, SubActionTelemetry, Variant, VariantKind};

pub struct CoreStringLengthRunner;

#[async_trait]
impl SubActionRunner for CoreStringLengthRunner {
    fn id(&self) -> &str {
        "core.string.length"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Util
    }

    fn label(&self) -> &str {
        "String - Length"
    }

    fn summary(&self) -> &str {
        "Get the length of a string in bytes or Unicode scalar values (chars)"
    }

    fn search_text(&self) -> &str {
        "string length size count chars bytes"
    }

    fn icon_name(&self) -> &str {
        "ruler"
    }

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert("source".to_owned(), Variant::String(String::new()));
        cfg.insert("mode".to_owned(), Variant::String("chars".to_owned()));
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
                placeholder: "Hello",
            },
            FormField::Select {
                key: "mode",
                label: "Mode",
                options: &["chars", "bytes"],
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
                kind: VariantKind::Int,
                label: "String length".to_owned(),
            }],
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, "core.string.length");

        let source = config.str("source").unwrap_or("");
        let mode = config.str("mode").unwrap_or("chars");
        let into_var = forge_types::strip_var_decoration(
            config.str_nonempty("into_var").unwrap_or("string.result"),
        );

        // "bytes" mode returns UTF-8 byte count, which differs from char count for non-ASCII input.
        let length = if mode == "bytes" {
            source.len() as i64
        } else {
            source.chars().count() as i64
        };

        let new_stack = ctx.arg_stack.clone().set(into_var, Variant::Int(length));

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

    async fn length_of(mode: &str) -> i64 {
        let mut cfg = SubActionConfig::new();
        cfg.insert("source".to_owned(), Variant::String("Привіт".to_owned()));
        cfg.insert("mode".to_owned(), Variant::String(mode.to_owned()));
        let stack = ArgStack::new();
        let ctx = RunContext::leaf(&stack, 0, EventId::new(), &NullPublisher);
        let out = CoreStringLengthRunner.execute(&cfg, &ctx).await.1.unwrap();
        out.get("string.result").and_then(|v| v.as_int()).unwrap()
    }

    #[tokio::test]
    async fn length_bytes_mode_exceeds_char_count_for_non_ascii() {
        assert_eq!(length_of("chars").await, 6);
        assert_eq!(length_of("bytes").await, 12);
    }
}
