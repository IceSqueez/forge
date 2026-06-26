use async_trait::async_trait;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

/// Values written here survive only for the duration of the current action execution; they are not persisted.
pub struct CoreArgsSetRunner;

#[async_trait]
impl SubActionRunner for CoreArgsSetRunner {
    fn id(&self) -> &str {
        "core.args.set"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Util
    }

    fn label(&self) -> &str {
        "Set Local Arg"
    }

    fn summary(&self) -> &str {
        "Set a variable in the current execution's ArgStack"
    }

    fn search_text(&self) -> &str {
        "set local arg variable execution stack transient"
    }

    fn icon_name(&self) -> &str {
        "variable"
    }

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert("name".to_owned(), Variant::String(String::new()));
        cfg.insert("value".to_owned(), Variant::String(String::new()));
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Text {
                key: "name",
                label: "Variable Name",
                placeholder: "my_var",
            },
            FormField::Text {
                key: "value",
                label: "Value",
                placeholder: "hello",
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("name").and_then(|v| v.as_str()) {
            Some(s) if !s.is_empty() => Ok(()),
            _ => Err(RegistryError::UnknownKindId(
                "core.args.set: name is required".to_owned(),
            )),
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();

        let name_template = config
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let name = ctx.arg_stack.interpolate(name_template);

        let value = match config.get("value") {
            Some(Variant::String(s)) => {
                let interpolated = ctx.arg_stack.interpolate(s);
                super::interpolate::parse_variant(&interpolated)
            }
            Some(v) => v.clone(),
            None => Variant::String(String::new()),
        };

        let new_stack = ctx.arg_stack.clone().set(name, value);

        let duration_ms = (OffsetDateTime::now_utc() - started_at)
            .whole_milliseconds()
            .max(0) as u64;

        (
            SubActionTelemetry {
                index: ctx.index,
                kind: "core.args.set".to_owned(),
                started_at,
                duration_ms,
                outcome: SubActionOutcome::Success,
            },
            Some(new_stack),
        )
    }
}
