use async_trait::async_trait;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

pub struct CoreStringConcatRunner;

#[async_trait]
impl SubActionRunner for CoreStringConcatRunner {
    fn id(&self) -> &str {
        "core.string.concat"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Util
    }

    fn label(&self) -> &str {
        "String - Concat"
    }

    fn summary(&self) -> &str {
        "Join a list of strings with an optional separator"
    }

    fn search_text(&self) -> &str {
        "string concat join concatenate parts separator"
    }

    fn icon_name(&self) -> &str {
        "text-plus"
    }

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert("parts".to_owned(), Variant::String(String::new()));
        cfg.insert("separator".to_owned(), Variant::String(String::new()));
        cfg.insert(
            "into_var".to_owned(),
            Variant::String("string.result".to_owned()),
        );
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::TextArea {
                key: "parts",
                label: "Parts (one per line)",
            },
            FormField::Text {
                key: "separator",
                label: "Separator",
                placeholder: "",
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

        let parts: Vec<String> = match config.get("parts") {
            Some(Variant::Array(arr)) => arr
                .iter()
                .map(|v| v.as_str().unwrap_or("").to_owned())
                .collect(),
            Some(v) => v
                .as_str()
                .unwrap_or("")
                .lines()
                .map(str::to_owned)
                .collect(),
            None => vec![],
        };

        let separator = config
            .get("separator")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let into_var = config
            .get("into_var")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or("string.result")
            .to_owned();

        let result = parts.join(separator);
        let new_stack = ctx.arg_stack.clone().set(into_var, Variant::String(result));

        let duration_ms = (OffsetDateTime::now_utc() - started_at)
            .whole_milliseconds()
            .max(0) as u64;

        (
            SubActionTelemetry {
                index: ctx.index,
                kind: "core.string.concat".to_owned(),
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

    async fn run(cfg: &SubActionConfig, stack: &ArgStack) -> Option<ArgStack> {
        let ctx = RunContext::leaf(stack, 0, EventId::new(), &NullPublisher);
        CoreStringConcatRunner.execute(cfg, &ctx).await.1
    }

    #[tokio::test]
    async fn concat_joins_array_parts_with_separator_into_result_var() {
        let mut cfg = SubActionConfig::new();
        cfg.insert(
            "parts".to_owned(),
            Variant::Array(vec![
                Variant::String("a".to_owned()),
                Variant::String("b".to_owned()),
                Variant::String("c".to_owned()),
            ]),
        );
        cfg.insert("separator".to_owned(), Variant::String("-".to_owned()));
        let out = run(&cfg, &ArgStack::new()).await.unwrap();
        assert_eq!(
            out.get("string.result").and_then(|v| v.as_str()),
            Some("a-b-c")
        );
    }
}
