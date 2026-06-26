use async_trait::async_trait;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

pub struct CoreStringTrimRunner;

#[async_trait]
impl SubActionRunner for CoreStringTrimRunner {
    fn id(&self) -> &str {
        "core.string.trim"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Util
    }

    fn label(&self) -> &str {
        "String — Trim Whitespace"
    }

    fn summary(&self) -> &str {
        "Remove leading and/or trailing whitespace from a string"
    }

    fn search_text(&self) -> &str {
        "string trim strip whitespace leading trailing both"
    }

    fn icon_name(&self) -> &str {
        "text-wrap"
    }

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert("source".to_owned(), Variant::String(String::new()));
        cfg.insert("mode".to_owned(), Variant::String("both".to_owned()));
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
                placeholder: "  hello  ",
            },
            FormField::Select {
                key: "mode",
                label: "Mode",
                options: &["both", "left", "right"],
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
        let mode = config
            .get("mode")
            .and_then(|v| v.as_str())
            .unwrap_or("both");
        let into_var = config
            .get("into_var")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or("string.result")
            .to_owned();

        let result = match mode {
            "left" => source.trim_start().to_owned(),
            "right" => source.trim_end().to_owned(),
            _ => source.trim().to_owned(),
        };

        let new_stack = ctx.arg_stack.clone().set(into_var, Variant::String(result));

        let duration_ms = (OffsetDateTime::now_utc() - started_at)
            .whole_milliseconds()
            .max(0) as u64;

        (
            SubActionTelemetry {
                index: ctx.index,
                kind: "core.string.trim".to_owned(),
                started_at,
                duration_ms,
                outcome: SubActionOutcome::Success,
            },
            Some(new_stack),
        )
    }
}
