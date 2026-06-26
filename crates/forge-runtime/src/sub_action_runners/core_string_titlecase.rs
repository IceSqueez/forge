use async_trait::async_trait;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

pub struct CoreStringTitlecaseRunner;

#[async_trait]
impl SubActionRunner for CoreStringTitlecaseRunner {
    fn id(&self) -> &str {
        "core.string.titlecase"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Util
    }

    fn label(&self) -> &str {
        "String — Titlecase"
    }

    fn summary(&self) -> &str {
        "Capitalize the first character of each whitespace-delimited word"
    }

    fn search_text(&self) -> &str {
        "string titlecase title case capitalize word"
    }

    fn icon_name(&self) -> &str {
        "letter-case"
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
                placeholder: "hello world",
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
        let into_var = config
            .get("into_var")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or("string.result")
            .to_owned();

        let result = to_titlecase(source);
        let new_stack = ctx.arg_stack.clone().set(into_var, Variant::String(result));

        let duration_ms = (OffsetDateTime::now_utc() - started_at)
            .whole_milliseconds()
            .max(0) as u64;

        (
            SubActionTelemetry {
                index: ctx.index,
                kind: "core.string.titlecase".to_owned(),
                started_at,
                duration_ms,
                outcome: SubActionOutcome::Success,
            },
            Some(new_stack),
        )
    }
}

/// Word boundaries are Unicode whitespace only; hyphens and underscores are not boundaries.
fn to_titlecase(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut capitalize_next = true;
    for c in s.chars() {
        if c.is_whitespace() {
            result.push(c);
            capitalize_next = true;
        } else if capitalize_next {
            result.extend(c.to_uppercase());
            capitalize_next = false;
        } else {
            result.extend(c.to_lowercase());
        }
    }
    result
}
