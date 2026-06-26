use async_trait::async_trait;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

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
        "String — Format Template"
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

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();

        let template = config
            .get("template")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let into_var = config
            .get("into_var")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or("string.formatted")
            .to_owned();

        let new_stack = ctx
            .arg_stack
            .clone()
            .set(into_var, Variant::String(template));

        let duration_ms = (OffsetDateTime::now_utc() - started_at)
            .whole_milliseconds()
            .max(0) as u64;

        (
            SubActionTelemetry {
                index: ctx.index,
                kind: "core.string.format".to_owned(),
                started_at,
                duration_ms,
                outcome: SubActionOutcome::Success,
            },
            Some(new_stack),
        )
    }
}
