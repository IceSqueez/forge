use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_storage::GlobalsRepo;
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

pub struct CoreGlobalsGetRunner {
    globals: Arc<dyn GlobalsRepo>,
}

impl CoreGlobalsGetRunner {
    pub fn new(globals: Arc<dyn GlobalsRepo>) -> Self {
        Self { globals }
    }
}

#[async_trait]
impl SubActionRunner for CoreGlobalsGetRunner {
    fn id(&self) -> &str {
        "core.globals.get"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Globals
    }

    fn label(&self) -> &str {
        "Get Global"
    }

    fn summary(&self) -> &str {
        "Read a global variable into an argument"
    }

    fn search_text(&self) -> &str {
        "get global variable read load"
    }

    fn icon_name(&self) -> &str {
        "database-import"
    }

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = SubActionConfig::new();
        cfg.insert("name".to_owned(), Variant::String(String::new()));
        cfg.insert("into_arg".to_owned(), Variant::String(String::new()));
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::DynamicSelect {
                key: "name",
                label: "Variable Name",
                options_key: "global.names",
            },
            FormField::Text {
                key: "into_arg",
                label: "Output Variable",
                placeholder: "result",
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        let name_ok = config
            .get("name")
            .and_then(|v| v.as_str())
            .is_some_and(|s| !s.is_empty());
        let arg_ok = config
            .get("into_arg")
            .and_then(|v| v.as_str())
            .is_some_and(|s| !s.is_empty());
        if name_ok && arg_ok {
            Ok(())
        } else {
            Err(RegistryError::UnknownKindId(
                "core.globals.get: name and into_arg are required".to_owned(),
            ))
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
        let into_template = config
            .get("into_arg")
            .and_then(|v| v.as_str())
            .unwrap_or_default();

        let resolved_name =
            super::interpolate::sanitize_var_name(&ctx.arg_stack.interpolate(name_template));
        let resolved_into =
            super::interpolate::sanitize_var_name(&ctx.arg_stack.interpolate(into_template));

        let (outcome, updated_stack) = match self.globals.get(&resolved_name).await {
            Ok(value) => {
                let variant = value.unwrap_or(Variant::String(String::new()));
                let new_stack = ctx.arg_stack.clone().set(resolved_into, variant);
                (SubActionOutcome::Success, Some(new_stack))
            }
            Err(e) => (SubActionOutcome::Failed(e.to_string()), None),
        };

        let duration_ms = (OffsetDateTime::now_utc() - started_at)
            .whole_milliseconds()
            .max(0) as u64;

        (
            SubActionTelemetry {
                index: ctx.index,
                kind: "core.globals.get".to_owned(),
                started_at,
                duration_ms,
                outcome,
            },
            updated_stack,
        )
    }
}
